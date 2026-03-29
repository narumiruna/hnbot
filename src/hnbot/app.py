import httpx
import redis
from loguru import logger

from hnbot.article import generate_article
from hnbot.rss import HNEntry
from hnbot.rss import get_hn_feed
from hnbot.utils import html_to_markdown


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
            break

    def process_entry(self, entry: HNEntry) -> None:
        resp = httpx.get(entry.comment_url, follow_redirects=True)
        resp.raise_for_status()

        content = html_to_markdown(resp.text)

        article = generate_article(content)
        page_url = article.create_page()
