import asyncio
from dataclasses import dataclass
from datetime import UTC
from datetime import datetime

import httpx

from hnbot.app import App
from hnbot.rss import HNEntry
from hnbot.rss import HNFeed
from hnbot.settings import Settings


@dataclass
class FakeArticle:
    url: str = "https://telegra.ph/fake"

    def create_page(self) -> str:
        return self.url


@dataclass
class FakeSummary:
    text: str = "測試摘要"


class FakeRedis:
    def __init__(self) -> None:
        self._data: dict[str, str] = {}

    async def exists(self, key: str) -> bool:
        return key in self._data

    async def set(self, key: str, value: str) -> None:
        self._data[key] = value


def _entry(entry_id: str) -> HNEntry:
    return HNEntry(
        title=f"title-{entry_id}",
        link=f"https://example.com/{entry_id}",
        comment_url=f"https://news.ycombinator.com/item?id={entry_id}",
        id=entry_id,
        published_at=datetime.now(UTC),
    )


def _settings(**overrides: object) -> Settings:
    return Settings.model_validate(
        {
            "bot_token": "bot-token",
            "chat_id": "chat-id",
            **overrides,
        }
    )


def _http_status_error(status: int, url: str, retry_after: str | None = None) -> httpx.HTTPStatusError:
    request = httpx.Request("GET", url)
    headers = {"Retry-After": retry_after} if retry_after is not None else None
    response = httpx.Response(status, request=request, headers=headers)
    return httpx.HTTPStatusError("status error", request=request, response=response)


def _close_app_client(app: App) -> None:
    asyncio.run(app.http_client.aclose())


def test_process_entry_retries_on_429_then_success(monkeypatch) -> None:
    app = App(_settings())
    app.redis_client = FakeRedis()
    call_count = {"count": 0}

    async def fake_get(url: str) -> httpx.Response:
        call_count["count"] += 1
        if call_count["count"] == 1:
            raise _http_status_error(429, url, retry_after="0")
        request = httpx.Request("GET", url)
        return httpx.Response(200, request=request, text="<p>comment markdown</p>")

    async def fake_send_message(_message: str, _settings_obj: Settings) -> None:
        return None

    async def fake_generate_article(_content: str) -> FakeArticle:
        return FakeArticle()

    async def fake_summarize_async(_content: str) -> FakeSummary:
        return FakeSummary()

    monkeypatch.setattr(app.http_client, "get", fake_get)
    monkeypatch.setattr("hnbot.app.generate_article_async", fake_generate_article)
    monkeypatch.setattr("hnbot.app.summarize_async", fake_summarize_async)
    monkeypatch.setattr("hnbot.app.send_message", fake_send_message)

    try:
        assert app.process_entry(_entry("1")) is True
        assert call_count["count"] == 2
    finally:
        _close_app_client(app)


def test_process_entry_skips_after_retry_exhausted(monkeypatch) -> None:
    app = App(_settings())
    app.redis_client = FakeRedis()
    call_count = {"count": 0}

    async def fake_get(url: str) -> httpx.Response:
        call_count["count"] += 1
        raise _http_status_error(429, url, retry_after="0")

    monkeypatch.setattr(app.http_client, "get", fake_get)

    try:
        assert app.process_entry(_entry("2")) is False
        assert call_count["count"] == 3
    finally:
        _close_app_client(app)


def test_run_continues_and_marks_only_success(monkeypatch) -> None:
    app = App(_settings())
    fake_redis = FakeRedis()
    app.redis_client = fake_redis

    entry_one = _entry("101")
    entry_two = _entry("102")
    feed = HNFeed(title="HN", entries=[entry_one, entry_two])
    process_results = {"101": False, "102": True}

    async def fake_get_hn_feed(_client, **_kwargs: object) -> HNFeed:
        return feed

    async def fake_process_entry_pipeline(entry: HNEntry, **_kwargs: object) -> bool:
        return process_results[entry.id]

    monkeypatch.setattr("hnbot.app.get_hn_feed_async", fake_get_hn_feed)
    monkeypatch.setattr(app, "_process_entry_pipeline", fake_process_entry_pipeline)

    app.run()

    assert f"hnbot:entry:{entry_one.id}" not in fake_redis._data
    assert fake_redis._data[f"hnbot:entry:{entry_two.id}"] == entry_two.comment_url


def test_process_entry_truncates_markdown_above_limit(monkeypatch) -> None:
    cap = 20_000
    app = App(_settings(max_comment_markdown_chars=cap))
    app.redis_client = FakeRedis()
    captured_content: dict[str, str] = {}

    async def fake_get(url: str) -> httpx.Response:
        request = httpx.Request("GET", url)
        long_comment = "<p>" + ("a" * (cap + 100)) + "</p>"
        return httpx.Response(200, request=request, text=long_comment)

    async def fake_send_message(_message: str, _settings_obj: Settings) -> None:
        return None

    async def fake_generate_article(content: str) -> FakeArticle:
        captured_content["value"] = content
        return FakeArticle()

    async def fake_summarize_async(_content: str) -> FakeSummary:
        return FakeSummary()

    monkeypatch.setattr(app.http_client, "get", fake_get)
    monkeypatch.setattr("hnbot.app.generate_article_async", fake_generate_article)
    monkeypatch.setattr("hnbot.app.summarize_async", fake_summarize_async)
    monkeypatch.setattr("hnbot.app.send_message", fake_send_message)

    try:
        assert app.process_entry(_entry("103")) is True
    finally:
        _close_app_client(app)

    assert len(captured_content["value"]) == cap
    assert captured_content["value"] == "a" * cap


def test_process_entry_keeps_markdown_when_within_limit(monkeypatch) -> None:
    app = App(_settings(max_comment_markdown_chars=20_000))
    app.redis_client = FakeRedis()
    captured_content: dict[str, str] = {}

    async def fake_get(url: str) -> httpx.Response:
        request = httpx.Request("GET", url)
        return httpx.Response(200, request=request, text="<p>short comment</p>")

    async def fake_send_message(_message: str, _settings_obj: Settings) -> None:
        return None

    async def fake_generate_article(content: str) -> FakeArticle:
        captured_content["value"] = content
        return FakeArticle()

    async def fake_summarize_async(_content: str) -> FakeSummary:
        return FakeSummary()

    monkeypatch.setattr(app.http_client, "get", fake_get)
    monkeypatch.setattr("hnbot.app.generate_article_async", fake_generate_article)
    monkeypatch.setattr("hnbot.app.summarize_async", fake_summarize_async)
    monkeypatch.setattr("hnbot.app.send_message", fake_send_message)

    try:
        assert app.process_entry(_entry("104")) is True
    finally:
        _close_app_client(app)

    assert captured_content["value"] == "short comment"


def test_run_allows_parallel_generation_with_serial_comment_fetch(monkeypatch) -> None:
    app = App(
        _settings(
            comments_fetch_concurrency=1,
            article_pipeline_concurrency=3,
        )
    )
    app.redis_client = FakeRedis()

    feed = HNFeed(title="HN", entries=[_entry("201"), _entry("202"), _entry("203")])
    fetch_active = {"value": 0, "max": 0}
    send_order: list[str] = []

    async def fake_get_hn_feed(_client, **_kwargs: object) -> HNFeed:
        return feed

    async def fake_get(url: str) -> httpx.Response:
        fetch_active["value"] += 1
        fetch_active["max"] = max(fetch_active["max"], fetch_active["value"])
        await asyncio.sleep(0.01)
        fetch_active["value"] -= 1
        request = httpx.Request("GET", url)
        item_id = url.split("=")[-1]
        return httpx.Response(200, request=request, text=f"<p>comment {item_id}</p>")

    async def fake_generate_article(content: str) -> FakeArticle:
        entry_id = content.rsplit(" ", 1)[-1]
        delay = {"201": 0.06, "202": 0.02, "203": 0.01}[entry_id]
        await asyncio.sleep(delay)
        return FakeArticle(url=f"https://telegra.ph/{entry_id}")

    async def fake_summarize_async(_content: str) -> FakeSummary:
        return FakeSummary()

    async def fake_send_message(message: str, _settings_obj: Settings) -> None:
        # First line is "<b>title-NNN</b>"; extract the entry number.
        first_line = message.splitlines()[0]  # e.g. "<b>title-201</b>"
        entry_num = first_line.split("-")[-1].removesuffix("</b>")
        send_order.append(entry_num)

    monkeypatch.setattr("hnbot.app.get_hn_feed_async", fake_get_hn_feed)
    monkeypatch.setattr(app.http_client, "get", fake_get)
    monkeypatch.setattr("hnbot.app.generate_article_async", fake_generate_article)
    monkeypatch.setattr("hnbot.app.summarize_async", fake_summarize_async)
    monkeypatch.setattr("hnbot.app.send_message", fake_send_message)

    app.run()

    assert fetch_active["max"] == 1
    assert send_order != ["201", "202", "203"]
