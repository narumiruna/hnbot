import asyncio
import html
from datetime import UTC
from datetime import datetime
from email.utils import parsedate_to_datetime

import httpx
import logfire
import redis
from aiogram import Bot
from aiogram.client.default import DefaultBotProperties
from aiogram.enums import ParseMode
from loguru import logger
from tenacity import RetryCallState
from tenacity import retry
from tenacity import retry_if_exception
from tenacity import stop_after_attempt
from tenacity import wait_exponential_jitter

from hnbot.article import generate_article_async
from hnbot.article import summarize_async
from hnbot.rss import HNEntry
from hnbot.rss import get_hn_feed_async
from hnbot.settings import Settings
from hnbot.utils import html_to_markdown

_DEFAULT_WAIT = wait_exponential_jitter(initial=1, max=8)


def _is_transient_fetch_error(exc: BaseException) -> bool:
    if isinstance(exc, httpx.HTTPStatusError):
        status_code = exc.response.status_code
        return status_code == 429 or 500 <= status_code < 600

    return isinstance(exc, httpx.RequestError)


def _retry_after_seconds(exc: BaseException) -> float | None:
    if not isinstance(exc, httpx.HTTPStatusError):
        return None

    retry_after = exc.response.headers.get("Retry-After")
    if retry_after is None:
        return None

    try:
        return max(float(retry_after), 0.0)
    except ValueError:
        parsed_dt = parsedate_to_datetime(retry_after)
        if parsed_dt.tzinfo is None:
            parsed_dt = parsed_dt.replace(tzinfo=UTC)
        return max((parsed_dt - datetime.now(UTC)).total_seconds(), 0.0)


def _retry_wait(retry_state: RetryCallState) -> float:
    if retry_state.outcome is None:
        return _DEFAULT_WAIT(retry_state)

    exc = retry_state.outcome.exception()
    if exc is None:
        return _DEFAULT_WAIT(retry_state)

    retry_after_seconds = _retry_after_seconds(exc)
    if retry_after_seconds is not None:
        return retry_after_seconds

    return _DEFAULT_WAIT(retry_state)


def _log_retry(retry_state: RetryCallState) -> None:
    if retry_state.outcome is None:
        return

    exc = retry_state.outcome.exception()
    if exc is None:
        return

    if len(retry_state.args) < 2:
        return

    entry = retry_state.args[1]
    if not isinstance(entry, HNEntry):
        return

    logger.warning(
        "Transient fetch error for entry {} on attempt {}: {}",
        entry.id,
        retry_state.attempt_number,
        exc,
    )


async def send_message(message: str, settings: Settings) -> None:
    with logfire.span(
        "hnbot.telegram.send_message",
        chat_id=settings.chat_id,
        message_chars=len(message),
    ):
        async with Bot(
            token=settings.bot_token,
            default=DefaultBotProperties(
                parse_mode=ParseMode.HTML,
            ),
        ) as bot:
            await bot.send_message(chat_id=settings.chat_id, text=message)


class App:
    def __init__(self, settings: Settings) -> None:
        self.settings = settings
        self.redis_client = redis.Redis(
            host=settings.redis_host,
            port=settings.redis_port,
            db=settings.redis_db,
        )
        self.http_client = httpx.AsyncClient(
            timeout=httpx.Timeout(settings.http_timeout_seconds),
            headers={"User-Agent": settings.http_user_agent},
        )

    def run(self) -> None:
        asyncio.run(self._run_async())

    def process_entry(self, entry: HNEntry) -> bool:
        return asyncio.run(self._process_feed_entry(entry))

    async def _run_async(self) -> None:
        with logfire.span("hnbot.run.feed_batch"):
            try:
                await self._run_feed_batch()
            finally:
                await self.http_client.aclose()

    async def _run_feed_batch(self) -> None:
        with logfire.span("hnbot.run.fetch_feed"):
            feed = await get_hn_feed_async(self.http_client)
        await self._process_feed_entries(feed.entries, feed.title)

    async def _process_feed_entries(self, entries: list[HNEntry], feed_title: str) -> None:
        with logfire.span(
            "hnbot.run.process_entries",
            feed_title=feed_title,
            entry_count=len(entries),
        ):
            # Sleep for a bit to avoid hitting the feed too quickly
            await asyncio.sleep(0.5)

            fetch_semaphore = asyncio.Semaphore(self.settings.comments_fetch_concurrency)
            pipeline_semaphore = asyncio.Semaphore(self.settings.article_pipeline_concurrency)

            tasks = [
                asyncio.create_task(
                    self._process_feed_entry(
                        entry,
                        fetch_semaphore=fetch_semaphore,
                        pipeline_semaphore=pipeline_semaphore,
                    )
                )
                for entry in entries
            ]
            if tasks:
                await asyncio.gather(*tasks)

    async def _process_feed_entry(
        self,
        entry: HNEntry,
        fetch_semaphore: asyncio.Semaphore | None = None,
        pipeline_semaphore: asyncio.Semaphore | None = None,
    ) -> bool:
        with logfire.span(
            "hnbot.run.entry",
            entry_id=entry.id,
            comment_url=entry.comment_url,
            link=entry.link,
        ):
            key = f"hnbot:entry:{entry.id}"
            already_processed = bool(self.redis_client.exists(key))

            if already_processed:
                logger.info("Already processed entry with id: {}", entry.id)
                return True

            if await self._process_entry_pipeline(
                entry,
                fetch_semaphore=fetch_semaphore,
                pipeline_semaphore=pipeline_semaphore,
            ):
                self.redis_client.set(key, entry.comment_url)
                logger.info("Marked entry as processed: {}", entry.id)
                return True

            logger.warning("Skipping entry after failed processing: {}", entry.id)
            return False

    @retry(
        stop=stop_after_attempt(3),
        wait=_retry_wait,
        retry=retry_if_exception(_is_transient_fetch_error),
        before_sleep=_log_retry,
        reraise=True,
    )
    async def _fetch_comment_markdown(self, entry: HNEntry) -> str:
        with logfire.span(
            "hnbot.entry.fetch_comment_markdown",
            entry_id=entry.id,
            comment_url=entry.comment_url,
            max_chars=self.settings.max_comment_markdown_chars,
        ):
            resp = await self.http_client.get(entry.comment_url)
            resp.raise_for_status()
            content = html_to_markdown(resp.text)
            content_len = len(content)
            if content_len <= self.settings.max_comment_markdown_chars:
                return content

            logger.info(
                "Truncating markdown for entry {} from {} to {} chars",
                entry.id,
                content_len,
                self.settings.max_comment_markdown_chars,
            )
            with logfire.span(
                "hnbot.entry.truncate_comment_markdown",
                entry_id=entry.id,
                original_chars=content_len,
                max_chars=self.settings.max_comment_markdown_chars,
            ):
                return content[: self.settings.max_comment_markdown_chars]

    async def _process_entry_pipeline(
        self,
        entry: HNEntry,
        fetch_semaphore: asyncio.Semaphore | None = None,
        pipeline_semaphore: asyncio.Semaphore | None = None,
    ) -> bool:
        with logfire.span(
            "hnbot.run.entry.process",
            entry_id=entry.id,
            comment_url=entry.comment_url,
            link=entry.link,
        ):
            logger.info("Processing entry with id: {}", entry.id)

            try:
                if fetch_semaphore is None:
                    content = await self._fetch_comment_markdown(entry)
                else:
                    async with fetch_semaphore:
                        content = await self._fetch_comment_markdown(entry)
            except httpx.HTTPError:
                logger.exception("Failed to fetch comments for entry {}", entry.id)
                return False

            try:
                if pipeline_semaphore is None:
                    summary, page_url = await asyncio.gather(
                        self._summarize(content, entry.id),
                        self._generate_page(content, entry.id),
                    )
                else:
                    async with pipeline_semaphore:
                        summary, page_url = await asyncio.gather(
                            self._summarize(content, entry.id),
                            self._generate_page(content, entry.id),
                        )
            except (RuntimeError, ValueError):
                logger.exception("Failed to generate/send article for entry {}", entry.id)
                return False

            escaped_title = html.escape(entry.title)
            message_parts = [f"<b>{escaped_title}</b>"]
            if summary:
                message_parts.append(html.escape(summary))
            message_parts.append(
                f'🔗 <a href="{html.escape(entry.link)}">原文連結</a>  '
                f'💬 <a href="{html.escape(entry.comment_url)}">HN 討論</a>  '
                f'📝 <a href="{html.escape(page_url)}">完整筆記</a>'
            )
            message = "\n\n".join(message_parts)

            with logfire.span("hnbot.entry.send_message", entry_id=entry.id, chat_id=self.settings.chat_id):
                await send_message(message, self.settings)

            logger.info("Successfully processed entry {}", entry.id)
            return True

    async def _summarize(self, content: str, entry_id: str) -> str:
        with logfire.span("hnbot.entry.summarize", entry_id=entry_id):
            summary = await summarize_async(content)
            return summary.text

    async def _generate_page(self, content: str, entry_id: str) -> str:
        with logfire.span("hnbot.entry.generate_article", entry_id=entry_id):
            article = await generate_article_async(content)
        with logfire.span("hnbot.entry.create_page", entry_id=entry_id):
            return await asyncio.to_thread(article.create_page)
