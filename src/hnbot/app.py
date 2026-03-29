import redis
from loguru import logger

from hnbot.rss import get_hn_feed


class App:
    def __init__(self) -> None:
        self.redis_client = redis.Redis(host="localhost", port=6379, db=0)

    def run(self) -> None:
        feed = get_hn_feed()
        for entry in feed.entries:
            if self.redis_client.exists(entry.id):
                logger.info("Already processed entry with id: %s", entry.id)
                continue

            self.redis_client.set(entry.id, entry.comment_url)
