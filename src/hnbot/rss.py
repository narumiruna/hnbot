from dataclasses import dataclass
from datetime import UTC
from datetime import datetime

import feedparser
import httpx


@dataclass
class HNEntry:
    title: str
    link: str
    comment_url: str
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


def get_hn_feed(points: int = 100) -> HNFeed:
    url = f"https://hnrss.org/newest?points={points}"

    resp = httpx.get(url, follow_redirects=True)
    resp.raise_for_status()

    parsed_dict = feedparser.parse(resp.content)

    return HNFeed(
        title=parsed_dict["feed"]["title"],
        entries=[
            HNEntry(
                title=entry["title"],
                link=entry["link"],
                comment_url=entry["comments"],
                published_at=parse_datetime(entry["published_parsed"]),
            )
            for entry in parsed_dict["entries"]
        ],
    )
