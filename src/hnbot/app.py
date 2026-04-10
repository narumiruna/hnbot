import asyncio
import html
from collections.abc import Awaitable
from datetime import UTC
from datetime import datetime
from email.utils import parsedate_to_datetime
from typing import Protocol

import httpx
import redis.asyncio as aioredis
from aiogram import Bot
from aiogram.client.default import DefaultBotProperties
from aiogram.enums import ParseMode
from loguru import logger
from tenacity import RetryCallState
from tenacity import retry
from tenacity import retry_if_exception
from tenacity import stop_after_attempt
from tenacity import wait_exponential_jitter

from hnbot.article import Article
from hnbot.article import generate_article
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
    async with Bot(
        token=settings.bot_token,
        default=DefaultBotProperties(
            parse_mode=ParseMode.HTML,
        ),
    ) as bot:
        await bot.send_message(chat_id=settings.chat_id, text=message)


class SummaryCarrier(Protocol):
    summary: str


class CommentFetcher:
    def __init__(self, http_client: httpx.AsyncClient, settings: Settings) -> None:
        self.http_client = http_client
        self.settings = settings

    @retry(
        stop=stop_after_attempt(3),
        wait=_retry_wait,
        retry=retry_if_exception(_is_transient_fetch_error),
        before_sleep=_log_retry,
        reraise=True,
    )
    async def _fetch_with_retry(self, entry: HNEntry) -> str:
        resp = await self.http_client.get(entry.comment_url)
        resp.raise_for_status()
        return html_to_markdown(resp.text)

    async def fetch(self, entry: HNEntry) -> str:
        return await self._fetch_with_retry(entry)


class ArticlePipeline:
    async def generate(self, content: str, settings: Settings) -> tuple[Article, str]:
        article = await generate_article(content, settings)
        page_url = await asyncio.to_thread(article.create_page)
        return article, page_url


class Notifier:
    def __init__(self, settings: Settings | None = None) -> None:
        self.settings = settings

    def build_message(self, entry: HNEntry, article: SummaryCarrier, page_url: str) -> str:
        escaped_title = html.escape(entry.title)
        message_parts = [f"<b>{escaped_title}</b>"]
        if article.summary:
            message_parts.append(html.escape(article.summary))
        message_parts.append(
            f'🔗 <a href="{html.escape(entry.link)}">原文連結</a>  '
            f'💬 <a href="{html.escape(entry.comment_url)}">HN 討論</a>  '
            f'📝 <a href="{html.escape(page_url)}">完整筆記</a>'
        )
        return "\n\n".join(message_parts)

    async def send(self, message: str) -> None:
        if self.settings is None:
            raise ValueError("Notifier settings are not configured.")
        await send_message(message, self.settings)


async def _with_optional_semaphore[T](coro: Awaitable[T], semaphore: asyncio.Semaphore | None) -> T:
    if semaphore is None:
        return await coro
    async with semaphore:
        return await coro


class App:
    def __init__(self, settings: Settings) -> None:
        self.settings = settings
        self.redis_client = aioredis.Redis(
            host=settings.redis_host,
            port=settings.redis_port,
            db=settings.redis_db,
        )
        self.http_client = httpx.AsyncClient(
            timeout=httpx.Timeout(settings.http_timeout_seconds),
            headers={"User-Agent": settings.http_user_agent},
        )
        self.fetcher = CommentFetcher(self.http_client, self.settings)
        self.pipeline = ArticlePipeline()
        self.notifier = Notifier(self.settings)

    def run(self) -> None:
        asyncio.run(self._run_async())

    async def _run_async(self) -> None:
        try:
            await self._run_feed_batch()
        finally:
            await self.http_client.aclose()
            await self.redis_client.aclose()

    async def _run_feed_batch(self) -> None:
        feed = await get_hn_feed_async(self.http_client, points=self.settings.feed_points)
        await self._process_feed_entries(feed.entries, feed.title)

    async def _process_feed_entries(self, entries: list[HNEntry], feed_title: str) -> None:
        # Sleep for a bit to avoid hitting the feed too quickly
        await asyncio.sleep(self.settings.batch_sleep_seconds)

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
        key = f"hnbot:entry:{entry.id}"
        already_processed = bool(await self.redis_client.exists(key))

        if already_processed:
            logger.info("Already processed entry with id: {}", entry.id)
            return True

        if await self._process_entry_pipeline(
            entry,
            fetch_semaphore=fetch_semaphore,
            pipeline_semaphore=pipeline_semaphore,
        ):
            await self.redis_client.set(key, entry.comment_url)
            logger.info("Marked entry as processed: {}", entry.id)
            return True

        logger.warning("Skipping entry after failed processing: {}", entry.id)
        return False

    async def _process_entry_pipeline(
        self,
        entry: HNEntry,
        fetch_semaphore: asyncio.Semaphore | None = None,
        pipeline_semaphore: asyncio.Semaphore | None = None,
    ) -> bool:
        logger.info("Processing entry with id: {}", entry.id)

        try:
            content = await _with_optional_semaphore(self.fetcher.fetch(entry), fetch_semaphore)
        except httpx.HTTPError:
            logger.exception("Failed to fetch comments for entry {}", entry.id)
            return False

        try:
            article, page_url = await _with_optional_semaphore(
                self.pipeline.generate(content, self.settings), pipeline_semaphore
            )
        except (RuntimeError, ValueError):
            logger.exception("Failed to generate/send article for entry {}", entry.id)
            return False

        message = self.notifier.build_message(entry, article, page_url)

        await self.notifier.send(message)

        logger.info("Successfully processed entry {}", entry.id)
        return True
