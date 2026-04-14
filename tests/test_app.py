import asyncio
from dataclasses import dataclass
from datetime import UTC
from datetime import datetime

import httpx
import pytest
from openai import BadRequestError

from hnbot.app import App
from hnbot.app import ArticlePipeline
from hnbot.app import Notifier
from hnbot.rss import HNEntry
from hnbot.rss import HNFeed
from hnbot.settings import Settings


@dataclass
class FakeArticle:
    url: str = "https://telegra.ph/fake"
    summary: str = "整合摘要"

    def create_page(self) -> str:
        return self.url


class FakeRedis:
    def __init__(self) -> None:
        self._data: dict[str, str] = {}

    async def exists(self, key: str) -> bool:
        return key in self._data

    async def set(self, key: str, value: str) -> None:
        self._data[key] = value

    async def aclose(self) -> None:
        pass


def _entry(entry_id: str) -> HNEntry:
    return HNEntry(
        title=f"title-{entry_id}",
        link=f"https://example.com/{entry_id}",
        comment_url=f"https://news.ycombinator.com/item?id={entry_id}",
        id=entry_id,
        published_at=datetime.now(UTC),
        points=100,
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


async def _close_app_client(app: App) -> None:
    await app.http_client.aclose()


@pytest.mark.anyio
async def test_process_entry_retries_on_429_then_success(monkeypatch) -> None:
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

    async def fake_generate_article(_content: str, _settings_obj: Settings) -> FakeArticle:
        return FakeArticle()

    monkeypatch.setattr(app.http_client, "get", fake_get)
    monkeypatch.setattr("hnbot.app.generate_article", fake_generate_article)
    monkeypatch.setattr("hnbot.app.send_message", fake_send_message)

    try:
        assert await app._process_feed_entry(_entry("1")) is True
        assert call_count["count"] == 2
    finally:
        await _close_app_client(app)


@pytest.mark.anyio
async def test_process_entry_skips_after_retry_exhausted(monkeypatch) -> None:
    app = App(_settings())
    app.redis_client = FakeRedis()
    call_count = {"count": 0}

    async def fake_get(url: str) -> httpx.Response:
        call_count["count"] += 1
        raise _http_status_error(429, url, retry_after="0")

    monkeypatch.setattr(app.http_client, "get", fake_get)

    try:
        assert await app._process_feed_entry(_entry("2")) is False
        assert call_count["count"] == 3
    finally:
        await _close_app_client(app)


@pytest.mark.anyio
async def test_process_entry_skips_invalid_prompt_error(monkeypatch) -> None:
    app = App(_settings())
    app.redis_client = FakeRedis()

    async def fake_get(url: str) -> httpx.Response:
        request = httpx.Request("GET", url)
        return httpx.Response(200, request=request, text="<p>biology content</p>")

    async def fake_generate_article(_content: str, _settings_obj: Settings) -> FakeArticle:
        request = httpx.Request("POST", "https://api.openai.com/v1/responses")
        response = httpx.Response(400, request=request)
        raise BadRequestError(
            "Invalid prompt: limited access to this content for safety reasons.",
            response=response,
            body={"error": {"code": "invalid_prompt"}},
        )

    monkeypatch.setattr(app.http_client, "get", fake_get)
    monkeypatch.setattr("hnbot.app.generate_article", fake_generate_article)

    try:
        assert await app._process_feed_entry(_entry("3")) is False
        assert "hnbot:entry:3" not in app.redis_client._data
    finally:
        await _close_app_client(app)


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

    monkeypatch.setattr("hnbot.app.get_hn_feed", fake_get_hn_feed)
    monkeypatch.setattr(app, "_process_entry_pipeline", fake_process_entry_pipeline)

    app.run()

    assert f"hnbot:entry:{entry_one.id}" not in fake_redis._data
    assert fake_redis._data[f"hnbot:entry:{entry_two.id}"] == entry_two.comment_url


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

    async def fake_generate_article(content: str, settings: Settings) -> FakeArticle:
        entry_id = content.rsplit(" ", 1)[-1]
        delay = {"201": 0.06, "202": 0.02, "203": 0.01}[entry_id]
        await asyncio.sleep(delay)
        return FakeArticle(url=f"https://telegra.ph/{entry_id}")

    async def fake_send_message(message: str, _settings_obj: Settings) -> None:
        # First line is "<b>title-NNN</b>"; extract the entry number.
        first_line = message.splitlines()[0]  # e.g. "<b>title-201</b>"
        entry_num = first_line.split("-")[-1].removesuffix("</b>")
        send_order.append(entry_num)

    monkeypatch.setattr("hnbot.app.get_hn_feed", fake_get_hn_feed)
    monkeypatch.setattr(app.http_client, "get", fake_get)
    monkeypatch.setattr("hnbot.app.generate_article", fake_generate_article)
    monkeypatch.setattr("hnbot.app.send_message", fake_send_message)

    app.run()

    assert fetch_active["max"] == 1
    assert send_order != ["201", "202", "203"]


def test_notifier_build_message_escapes_text_and_links() -> None:
    notifier = Notifier()
    entry = HNEntry(
        title="title-<1>",
        link="https://example.com/?a=<tag>",
        comment_url="https://news.ycombinator.com/item?id=1&x=<z>",
        id="1",
        published_at=datetime.now(UTC),
        points=123,
    )
    article = FakeArticle(url="https://telegra.ph/fake?x=<x>", summary="摘要 <b>text</b>")

    message = notifier.build_message(entry, article, article.url)

    assert message.startswith("<b>title-&lt;1&gt;</b>  ⭐ 123\n\n")
    assert "摘要 &lt;b&gt;text&lt;/b&gt;" in message
    assert 'href="https://example.com/?a=&lt;tag&gt;"' in message
    assert 'href="https://news.ycombinator.com/item?id=1&amp;x=&lt;z&gt;"' in message
    assert 'href="https://telegra.ph/fake?x=&lt;x&gt;"' in message


def test_notifier_build_message_omits_points_when_missing() -> None:
    notifier = Notifier()
    entry = HNEntry(
        title="title-1",
        link="https://example.com/1",
        comment_url="https://news.ycombinator.com/item?id=1",
        id="1",
        published_at=datetime.now(UTC),
        points=None,
    )
    article = FakeArticle()

    message = notifier.build_message(entry, article, article.url)

    assert message.startswith("<b>title-1</b>\n\n")
    assert "⭐" not in message


def test_article_pipeline_generates_article_and_page_url(monkeypatch) -> None:
    pipeline = ArticlePipeline()
    calls: dict[str, object] = {}

    async def fake_generate_article(content: str, settings: Settings) -> FakeArticle:
        calls["content"] = content
        return FakeArticle(url="https://telegra.ph/from-pipeline", summary="summary")

    monkeypatch.setattr("hnbot.app.generate_article", fake_generate_article)

    article, page_url = asyncio.run(pipeline.generate("markdown-content", Settings(bot_token="x", chat_id="y")))

    assert calls["content"] == "markdown-content"
    assert article.summary == "summary"
    assert page_url == "https://telegra.ph/from-pipeline"
