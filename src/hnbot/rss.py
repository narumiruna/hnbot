from dataclasses import dataclass
from datetime import UTC
from datetime import datetime
from urllib.parse import parse_qs
from urllib.parse import urlparse

import feedparser
import httpx


@dataclass
class HNEntry:
    title: str
    link: str
    comment_url: str
    id: str
    published_at: datetime


@dataclass
class HNFeed:
    title: str
    entries: list[HNEntry]


def parse_datetime(published_parsed: list[int]) -> datetime:
    year, month, day, hour, minute, second, microsecond, _, _ = published_parsed
    return datetime(
        year,
        month,
        day,
        hour,
        minute,
        second,
        microsecond,
        tzinfo=UTC,
    )


def parse_id(url: str) -> str:
    # Example link: https://news.ycombinator.com/item?id=12345678
    parsed_url = urlparse(url)
    parsed_qs = parse_qs(parsed_url.query)
    return parsed_qs["id"][0]


def _parse_feed(content: bytes) -> HNFeed:
    parsed_dict = feedparser.parse(content)

    entries = [
        HNEntry(
            title=entry["title"],
            link=entry["link"],
            comment_url=entry["comments"],
            id=parse_id(entry["comments"]),
            published_at=parse_datetime(entry["published_parsed"]),
        )
        for entry in parsed_dict["entries"]
    ]
    entries.reverse()

    return HNFeed(
        title=parsed_dict["feed"]["title"],
        entries=entries,
    )


def get_hn_feed(points: int = 100) -> HNFeed:
    url = f"https://hnrss.org/newest?points={points}"

    resp = httpx.get(url)
    resp.raise_for_status()

    return _parse_feed(resp.content)


async def get_hn_feed_async(client: httpx.AsyncClient, points: int = 100) -> HNFeed:
    url = f"https://hnrss.org/newest?points={points}"

    resp = await client.get(url)
    resp.raise_for_status()

    return _parse_feed(resp.content)
