from dataclasses import dataclass
from datetime import UTC
from datetime import datetime

import httpx

from hnbot.app import MAX_COMMENT_MARKDOWN_CHARS
from hnbot.app import App
from hnbot.rss import HNEntry
from hnbot.rss import HNFeed


@dataclass
class FakeArticle:
    url: str = "https://telegra.ph/fake"

    def create_page(self) -> str:
        return self.url


class FakeRedis:
    def __init__(self) -> None:
        self._data: dict[str, str] = {}

    def exists(self, key: str) -> bool:
        return key in self._data

    def set(self, key: str, value: str) -> None:
        self._data[key] = value


def _entry(entry_id: str) -> HNEntry:
    return HNEntry(
        title=f"title-{entry_id}",
        link=f"https://example.com/{entry_id}",
        comment_url=f"https://news.ycombinator.com/item?id={entry_id}",
        id=entry_id,
        published_at=datetime.now(UTC),
    )


def _http_status_error(status: int, url: str, retry_after: str | None = None) -> httpx.HTTPStatusError:
    request = httpx.Request("GET", url)
    headers = {"Retry-After": retry_after} if retry_after is not None else None
    response = httpx.Response(status, request=request, headers=headers)
    return httpx.HTTPStatusError("status error", request=request, response=response)


def test_process_entry_retries_on_429_then_success(monkeypatch) -> None:
    app = App()
    call_count = {"count": 0}

    def fake_get(url: str) -> httpx.Response:
        call_count["count"] += 1
        if call_count["count"] == 1:
            raise _http_status_error(429, url, retry_after="0")
        request = httpx.Request("GET", url)
        return httpx.Response(200, request=request, text="<p>comment markdown</p>")

    async def fake_send_message(_message: str) -> None:
        return None

    monkeypatch.setattr(app.http_client, "get", fake_get)
    monkeypatch.setattr("hnbot.app.generate_article", lambda _content: FakeArticle())
    monkeypatch.setattr("hnbot.app.send_message", fake_send_message)

    try:
        assert app.process_entry(_entry("1")) is True
        assert call_count["count"] == 2
    finally:
        app.http_client.close()


def test_process_entry_skips_after_retry_exhausted(monkeypatch) -> None:
    app = App()
    call_count = {"count": 0}

    def fake_get(url: str) -> httpx.Response:
        call_count["count"] += 1
        raise _http_status_error(429, url, retry_after="0")

    monkeypatch.setattr(app.http_client, "get", fake_get)

    try:
        assert app.process_entry(_entry("2")) is False
        assert call_count["count"] == 3
    finally:
        app.http_client.close()


def test_run_continues_and_marks_only_success(monkeypatch) -> None:
    app = App()
    fake_redis = FakeRedis()
    app.redis_client = fake_redis

    entry_one = _entry("101")
    entry_two = _entry("102")
    feed = HNFeed(title="HN", entries=[entry_one, entry_two])
    process_results = {"101": False, "102": True}

    def fake_process_entry(entry: HNEntry) -> bool:
        return process_results[entry.id]

    monkeypatch.setattr("hnbot.app.get_hn_feed", lambda: feed)
    monkeypatch.setattr(app, "process_entry", fake_process_entry)

    try:
        app.run()
    finally:
        app.http_client.close()

    assert f"hnbot:entry:{entry_one.id}" not in fake_redis._data
    assert fake_redis._data[f"hnbot:entry:{entry_two.id}"] == entry_two.comment_url


def test_process_entry_truncates_markdown_above_limit(monkeypatch) -> None:
    app = App()
    captured_content: dict[str, str] = {}

    def fake_get(url: str) -> httpx.Response:
        request = httpx.Request("GET", url)
        long_comment = "<p>" + ("a" * (MAX_COMMENT_MARKDOWN_CHARS + 100)) + "</p>"
        return httpx.Response(200, request=request, text=long_comment)

    async def fake_send_message(_message: str) -> None:
        return None

    def fake_generate_article(content: str) -> FakeArticle:
        captured_content["value"] = content
        return FakeArticle()

    monkeypatch.setattr(app.http_client, "get", fake_get)
    monkeypatch.setattr("hnbot.app.generate_article", fake_generate_article)
    monkeypatch.setattr("hnbot.app.send_message", fake_send_message)

    try:
        assert app.process_entry(_entry("103")) is True
    finally:
        app.http_client.close()

    assert len(captured_content["value"]) == MAX_COMMENT_MARKDOWN_CHARS
    assert captured_content["value"] == "a" * MAX_COMMENT_MARKDOWN_CHARS


def test_process_entry_keeps_markdown_when_within_limit(monkeypatch) -> None:
    app = App()
    captured_content: dict[str, str] = {}

    def fake_get(url: str) -> httpx.Response:
        request = httpx.Request("GET", url)
        return httpx.Response(200, request=request, text="<p>short comment</p>")

    async def fake_send_message(_message: str) -> None:
        return None

    def fake_generate_article(content: str) -> FakeArticle:
        captured_content["value"] = content
        return FakeArticle()

    monkeypatch.setattr(app.http_client, "get", fake_get)
    monkeypatch.setattr("hnbot.app.generate_article", fake_generate_article)
    monkeypatch.setattr("hnbot.app.send_message", fake_send_message)

    try:
        assert app.process_entry(_entry("104")) is True
    finally:
        app.http_client.close()

    assert captured_content["value"] == "short comment"
