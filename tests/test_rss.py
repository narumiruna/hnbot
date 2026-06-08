import time
from datetime import UTC
from datetime import datetime
from pathlib import Path

import httpx
import pytest

from hnbot.rss import _parse_feed
from hnbot.rss import get_hn_feed
from hnbot.rss import parse_datetime
from hnbot.rss import parse_num_comments
from hnbot.rss import parse_points


@pytest.fixture
def sample_rss() -> bytes:
    path = Path("tests/data/sample_rss.xml")
    with path.open("rb") as f:
        return f.read()


@pytest.fixture
def no_http_retry_wait(monkeypatch) -> None:
    monkeypatch.setattr("hnbot.http_retry._DEFAULT_WAIT", lambda _retry_state: 0.0)


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


def test_parse_num_comments_returns_integer() -> None:
    assert parse_num_comments("<p>Points: 123</p><p># Comments: 45</p>") == 45


def test_parse_num_comments_returns_none_when_missing() -> None:
    assert parse_num_comments("<p>Points: 123</p>") is None


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
    assert entry.num_comments is None
    assert isinstance(entry.published_at, datetime)
    assert entry.published_at.tzinfo == UTC


@pytest.mark.anyio
async def test_get_hn_feed_retries_connect_timeout_then_success(
    sample_rss: bytes, monkeypatch, no_http_retry_wait
) -> None:
    client = httpx.AsyncClient()
    call_count = {"count": 0}

    async def fake_get(url: str) -> httpx.Response:
        call_count["count"] += 1
        request = httpx.Request("GET", url)
        if call_count["count"] == 1:
            raise httpx.ConnectTimeout("connect timed out", request=request)
        return httpx.Response(200, request=request, content=sample_rss)

    monkeypatch.setattr(client, "get", fake_get)

    try:
        feed = await get_hn_feed(client, points=200)
    finally:
        await client.aclose()

    assert feed.title == "Hacker News: Best Comments"
    assert call_count["count"] == 2


@pytest.mark.anyio
async def test_get_hn_feed_raises_after_transient_retry_exhausted(monkeypatch, no_http_retry_wait) -> None:
    client = httpx.AsyncClient()
    call_count = {"count": 0}

    async def fake_get(url: str) -> httpx.Response:
        call_count["count"] += 1
        request = httpx.Request("GET", url)
        raise httpx.ConnectTimeout("connect timed out", request=request)

    monkeypatch.setattr(client, "get", fake_get)

    try:
        with pytest.raises(httpx.ConnectTimeout):
            await get_hn_feed(client, points=200)
    finally:
        await client.aclose()

    assert call_count["count"] == 3
