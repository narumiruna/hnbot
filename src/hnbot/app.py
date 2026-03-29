import asyncio
import time
from datetime import UTC
from datetime import datetime
from email.utils import parsedate_to_datetime

import httpx
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

from hnbot.article import generate_article
from hnbot.rss import HNEntry
from hnbot.rss import get_hn_feed
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


class App:
    def __init__(self, settings: Settings) -> None:
        self.settings = settings
        self.redis_client = redis.Redis(
            host=settings.redis_host,
            port=settings.redis_port,
            db=settings.redis_db,
        )
        self.http_client = httpx.Client(
            timeout=httpx.Timeout(settings.http_timeout_seconds),
            headers={"User-Agent": settings.http_user_agent},
        )

    def run(self) -> None:
        try:
            feed = get_hn_feed()

            # Sleep for a bit to avoid hitting the feed too quickly
            time.sleep(0.5)

            for entry in feed.entries:
                key = f"hnbot:entry:{entry.id}"

                if self.redis_client.exists(key):
                    logger.info("Already processed entry with id: {}", entry.id)
                    continue

                if self.process_entry(entry):
                    self.redis_client.set(key, entry.comment_url)
                    logger.info("Marked entry as processed: {}", entry.id)
                else:
                    logger.warning("Skipping entry after failed processing: {}", entry.id)
        finally:
            self.http_client.close()

    @retry(
        stop=stop_after_attempt(3),
        wait=_retry_wait,
        retry=retry_if_exception(_is_transient_fetch_error),
        before_sleep=_log_retry,
        reraise=True,
    )
    def _fetch_comment_markdown(self, entry: HNEntry) -> str:
        resp = self.http_client.get(entry.comment_url)
        resp.raise_for_status()
        content = html_to_markdown(resp.text)
        if len(content) <= self.settings.max_comment_markdown_chars:
            return content

        logger.info(
            "Truncating markdown for entry {} from {} to {} chars",
            entry.id,
            len(content),
            self.settings.max_comment_markdown_chars,
        )
        return content[: self.settings.max_comment_markdown_chars]

    def process_entry(self, entry: HNEntry) -> bool:
        logger.info("Processing entry with id: {}", entry.id)

        try:
            content = self._fetch_comment_markdown(entry)
        except httpx.HTTPError:
            logger.exception("Failed to fetch comments for entry {}", entry.id)
            return False

        try:
            article = generate_article(content)
            page_url = article.create_page()

            message = "\n\n".join(
                [
                    entry.title,
                    f"Link: {entry.link}",
                    f"Comments: {entry.comment_url}",
                    f"Note: {page_url}",
                ]
            )

            asyncio.run(send_message(message, self.settings))
        except (RuntimeError, ValueError):
            logger.exception("Failed to generate/send article for entry {}", entry.id)
            return False

        logger.info("Successfully processed entry {}", entry.id)
        return True
