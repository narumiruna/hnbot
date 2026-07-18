import asyncio
import html
import signal
from collections.abc import Awaitable
from collections.abc import Callable
from typing import Protocol
from typing import cast
from urllib.parse import urlparse

import httpx
import redis.asyncio as aioredis
from aiogram import Bot
from aiogram.client.default import DefaultBotProperties
from aiogram.enums import ParseMode
from loguru import logger
from openai import BadRequestError
from tenacity import RetryCallState
from tenacity import retry
from tenacity import stop_after_attempt

from hnbot.article import Article
from hnbot.article import generate_article
from hnbot.http_retry import log_transient_http_retry
from hnbot.http_retry import retry_transient_http_errors
from hnbot.rss import HNEntry
from hnbot.rss import get_hn_feed
from hnbot.settings import Settings
from hnbot.utils import html_to_markdown


def _extract_domain(url: str) -> str | None:
    host = urlparse(url).hostname
    if not host:
        return None
    return host.removeprefix("www.")


def _log_comment_fetch_retry(retry_state: RetryCallState) -> None:
    subject = "HN comments"
    if len(retry_state.args) >= 2:
        entry = retry_state.args[1]
        if isinstance(entry, HNEntry):
            subject = f"HN comments for entry {entry.id}"

    log_transient_http_retry(retry_state, subject=subject)


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


class RedisClient(Protocol):
    def exists(self, key: str) -> Awaitable[object]: ...

    def set(self, key: str, value: str) -> Awaitable[object]: ...

    def aclose(self) -> Awaitable[object]: ...


class CommentFetcher:
    def __init__(self, http_client: httpx.AsyncClient, settings: Settings) -> None:
        self.http_client = http_client
        self.settings = settings

    @retry_transient_http_errors(before_sleep=_log_comment_fetch_retry)
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
        title_line = f'📰 <b><a href="{html.escape(entry.link)}">{html.escape(entry.title)}</a></b>'

        meta_parts: list[str] = []
        if entry.points is not None:
            meta_parts.append(f"⭐ {entry.points}")
        if entry.num_comments is not None:
            meta_parts.append(f"💬 {entry.num_comments}")
        domain = _extract_domain(entry.link)
        if domain:
            meta_parts.append(f"🌐 {html.escape(domain)}")

        header = title_line if not meta_parts else f"{title_line}\n{' · '.join(meta_parts)}"
        message_parts = [header]

        if article.summary:
            message_parts.append(f"{html.escape(article.summary)}")

        message_parts.append(
            f'💬 <a href="{html.escape(entry.comment_url)}">討論</a>  ·  📝 <a href="{html.escape(page_url)}">筆記</a>'
        )
        return "\n\n".join(message_parts)

    async def send(self, message: str) -> None:
        if self.settings is None:
            raise ValueError("Notifier settings are not configured.")
        await send_message(message, self.settings)


async def _with_optional_semaphore[T](
    coro_factory: Callable[[], Awaitable[T]], semaphore: asyncio.Semaphore | None
) -> T:
    if semaphore is None:
        return await coro_factory()
    async with semaphore:
        return await coro_factory()


class App:
    def __init__(self, settings: Settings) -> None:
        self.settings = settings
        self.redis_client = cast(
            RedisClient,
            aioredis.Redis(
                host=settings.redis_host,
                port=settings.redis_port,
                db=settings.redis_db,
            ),
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

    def serve(self, poll_interval_seconds: float) -> None:
        try:
            asyncio.run(self._serve_async(poll_interval_seconds))
        except (asyncio.CancelledError, KeyboardInterrupt):
            logger.info("Service stopped")

    async def _run_async(self) -> None:
        try:
            await self._run_feed_batch()
        finally:
            await self._close_clients()

    async def _serve_async(self, poll_interval_seconds: float) -> None:
        loop = asyncio.get_running_loop()
        current_task = asyncio.current_task()
        sigterm_handler_installed = False

        if current_task is not None:
            try:
                loop.add_signal_handler(signal.SIGTERM, current_task.cancel)
                sigterm_handler_installed = True
            except (NotImplementedError, RuntimeError):
                pass

        try:
            await self._serve_loop(poll_interval_seconds)
        finally:
            if sigterm_handler_installed:
                loop.remove_signal_handler(signal.SIGTERM)
            await self._close_clients()

    async def _serve_loop(
        self,
        poll_interval_seconds: float,
        *,
        sleep: Callable[[float], Awaitable[None]] = asyncio.sleep,
    ) -> None:
        while True:
            try:
                await self._run_feed_batch()
            # A service batch is an isolation boundary; cancellation still propagates as BaseException.
            except Exception:  # noqa: BLE001
                logger.exception(
                    "Feed batch failed; retrying in {} seconds",
                    poll_interval_seconds,
                )

            await sleep(poll_interval_seconds)

    async def _close_clients(self) -> None:
        await self.http_client.aclose()
        await self.redis_client.aclose()

    async def _run_feed_batch(self) -> None:
        feed = await get_hn_feed(self.http_client, points=self.settings.feed_points)
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
        if not tasks:
            return

        results = await asyncio.gather(*tasks, return_exceptions=True)
        exceptions: list[Exception] = []

        for entry, result in zip(entries, results, strict=True):
            if isinstance(result, asyncio.CancelledError):
                raise result
            if isinstance(result, Exception):
                logger.opt(exception=result).error("Unhandled error processing entry {}", entry.id)
                exceptions.append(result)

        if exceptions:
            raise ExceptionGroup("One or more feed entries failed", exceptions)

    @retry(
        stop=stop_after_attempt(3),
        reraise=True,
    )
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
            content = await _with_optional_semaphore(lambda: self.fetcher.fetch(entry), fetch_semaphore)
        except httpx.HTTPError:
            logger.exception("Failed to fetch comments for entry {}", entry.id)
            return False

        try:
            article, page_url = await _with_optional_semaphore(
                lambda: self.pipeline.generate(content, self.settings), pipeline_semaphore
            )
        except BadRequestError as exc:
            logger.warning("Skipping entry {} due to non-processable LLM input: {}", entry.id, exc)
            return False
        except (RuntimeError, ValueError):
            logger.exception("Failed to generate/send article for entry {}", entry.id)
            return False

        message = self.notifier.build_message(entry, article, page_url)

        await self.notifier.send(message)

        logger.info("Successfully processed entry {}", entry.id)
        return True
