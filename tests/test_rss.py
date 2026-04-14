import time
from datetime import UTC
from datetime import datetime
from pathlib import Path

import pytest

from hnbot.rss import _parse_feed
from hnbot.rss import parse_datetime
from hnbot.rss import parse_points


@pytest.fixture
def sample_rss() -> bytes:
    path = Path("tests/data/sample_rss.xml")
    with path.open("rb") as f:
        return f.read()


def test_parse_datetime_uses_only_calendar_fields() -> None:
    published_parsed = time.struct_time((2026, 4, 10, 16, 22, 26, 4, 100, 0))

    assert parse_datetime(published_parsed) == datetime(2026, 4, 10, 16, 22, 26, tzinfo=UTC)


def test_parse_feed_entries_are_reversed(sample_rss: bytes) -> None:
    feed = _parse_feed(sample_rss)
    # reverse() makes the last XML item (id=200) first and the first XML item (id=100) last
    assert feed.entries[0].id == "47737182"
    assert feed.entries[1].id == "47737434"


def test_parse_points_returns_integer() -> None:
    assert parse_points("<p>Points: 123 # Comments: 10</p>") == 123


def test_parse_points_returns_none_when_missing() -> None:
    assert parse_points("<p>No score available</p>") is None


def test_parse_feed_entry_fields(sample_rss: bytes) -> None:
    feed = _parse_feed(sample_rss)
    entry = next(entry for entry in feed.entries if entry.id == "47737182")
    assert entry.title == (
        'New comment by hackingonempty in "I run multiple $10K MRR companies on a $20/month tech stack"'
    )
    assert entry.link == "https://news.ycombinator.com/item?id=47737182"
    assert entry.comment_url == "https://news.ycombinator.com/item?id=47737182"
    assert entry.id == "47737182"
    assert entry.points is None
    assert isinstance(entry.published_at, datetime)
    assert entry.published_at.tzinfo == UTC
