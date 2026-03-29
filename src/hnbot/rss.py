from dataclasses import dataclass

import feedparser
import httpx


@dataclass
class HNEntry:
    title: str
    link: str
    comments: str


@dataclass
class HNFeed:
    title: str
    entries: list[HNEntry]


def get_hn_feed(points: int) -> HNFeed:
    url = f"https://hnrss.org/newest?points={points}"
    resp = httpx.get(url)
    resp.raise_for_status()

    parsed_dict = feedparser.parse(resp.content)

    title = parsed_dict["feed"]["title"]

    entries = []
    for entry in parsed_dict["entries"]:
        title = entry["title"]
        link = entry["link"]
        comments = entry["comments"]
        entries.append(HNEntry(title=title, link=link, comments=comments))

    return HNFeed(title=title, entries=entries)
