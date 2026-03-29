import asyncio
import os

import httpx
import redis
from aiogram import Bot
from aiogram.client.default import DefaultBotProperties
from aiogram.enums import ParseMode
from loguru import logger

from hnbot.article import generate_article
from hnbot.rss import HNEntry
from hnbot.rss import get_hn_feed
from hnbot.utils import html_to_markdown


async def send_message(message: str) -> None:
    bot_token = os.getenv("BOT_TOKEN")
    if bot_token is None:
        logger.error("BOT_TOKEN is not set")
        return

    chat_id = os.getenv("CHAT_ID")
    if chat_id is None:
        logger.error("CHAT_ID is not set")
        return

    async with Bot(
        token=bot_token,
        default=DefaultBotProperties(
            parse_mode=ParseMode.HTML,
        ),
    ) as bot:
        await bot.send_message(chat_id=chat_id, text=message)


class App:
    def __init__(self) -> None:
        self.redis_client = redis.Redis(host="localhost", port=6379, db=0)

    def run(self) -> None:
        feed = get_hn_feed()
        for entry in feed.entries:
            key = f"hnbot:entry:{entry.id}"

            if self.redis_client.exists(key):
                logger.info("Already processed entry with id: {}", entry.id)
                continue

            self.process_entry(entry)

            self.redis_client.set(key, entry.comment_url)

    def process_entry(self, entry: HNEntry) -> None:
        resp = httpx.get(entry.comment_url, follow_redirects=True)
        resp.raise_for_status()

        content = html_to_markdown(resp.text)

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

        asyncio.run(send_message(message))
