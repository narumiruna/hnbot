import re
from collections.abc import Sequence
from dataclasses import dataclass
from datetime import UTC
from datetime import datetime
from urllib.parse import parse_qs
from urllib.parse import urlparse

import feedparser
import httpx
from tenacity import RetryCallState

from hnbot.http_retry import log_transient_http_retry
from hnbot.http_retry import retry_transient_http_errors


@dataclass
class HNEntry:
    title: str
    link: str
    comment_url: str
    id: str
    published_at: datetime
    points: int | None
    num_comments: int | None = None


@dataclass
class HNFeed:
    title: str
    entries: list[HNEntry]


def parse_datetime(published_parsed: Sequence[int]) -> datetime:
    year, month, day, hour, minute, second = published_parsed[:6]
    return datetime(
        year,
        month,
        day,
        hour,
        minute,
        second,
        tzinfo=UTC,
    )


def parse_id(url: str) -> str:
    # Example link: https://news.ycombinator.com/item?id=12345678
    parsed_url = urlparse(url)
    parsed_qs = parse_qs(parsed_url.query)
    return parsed_qs["id"][0]


def parse_points(description: str) -> int | None:
    match = re.search(r"Points:\s*(\d+)", description)
    if match is None:
        return None
    return int(match.group(1))


def parse_num_comments(description: str) -> int | None:
    match = re.search(r"#\s*Comments:\s*(\d+)", description)
    if match is None:
        return None
    return int(match.group(1))


def _parse_feed(content: bytes) -> HNFeed:
    parsed_dict = feedparser.parse(content)

    entries = [
        HNEntry(
            title=entry["title"],
            link=entry["link"],
            comment_url=entry["comments"],
            id=parse_id(entry["comments"]),
            published_at=parse_datetime(entry["published_parsed"]),
            points=parse_points(entry.get("summary", "")),
            num_comments=parse_num_comments(entry.get("summary", "")),
        )
        for entry in parsed_dict["entries"]
    ]
    entries.reverse()

    return HNFeed(
        title=parsed_dict["feed"]["title"],
        entries=entries,
    )


def _log_feed_retry(retry_state: RetryCallState) -> None:
    log_transient_http_retry(retry_state, subject="HN RSS feed")


@retry_transient_http_errors(before_sleep=_log_feed_retry)
async def _fetch_feed_content(client: httpx.AsyncClient, url: str) -> bytes:
    resp = await client.get(url)
    resp.raise_for_status()
    return resp.content


async def get_hn_feed(client: httpx.AsyncClient, points: int) -> HNFeed:
    url = f"https://hnrss.org/newest?points={points}"
    content = await _fetch_feed_content(client, url)
    return _parse_feed(content)
