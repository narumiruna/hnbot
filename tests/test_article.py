import asyncio

from hnbot.article import Article
from hnbot.article import Section
from hnbot.article import generate_article_async


def test_generate_article_async_parses_full_article(monkeypatch) -> None:
    captured: dict[str, object] = {}

    async def fake_async_parse(
        prompt: str,
        text_format: type[Article],
        instructions: str | None = None,
    ) -> Article:
        captured["prompt"] = prompt
        captured["text_format"] = text_format
        captured["instructions"] = instructions
        return Article(
            title="測試標題",
            summary="測試摘要",
            sections=[
                Section(title="重點", emoji="📌", content="第一段"),
                Section(title="結論", emoji="✅", content="第二段"),
            ],
        )

    monkeypatch.setattr("hnbot.article.async_parse", fake_async_parse)

    result = asyncio.run(generate_article_async("mock input"))

    assert result.title == "測試標題"
    assert result.summary == "測試摘要"
    assert [section.title for section in result.sections] == ["重點", "結論"]
    assert captured["prompt"] == "mock input"
    assert captured["text_format"] is Article
    assert isinstance(captured["instructions"], str)


def test_create_page_uses_rendered_sections(monkeypatch) -> None:
    captured: dict[str, str] = {}

    def fake_create_page(title: str, html_content: str) -> str:
        captured["title"] = title
        captured["html_content"] = html_content
        return "https://telegra.ph/fake"

    monkeypatch.setattr("hnbot.article.create_page", fake_create_page)
    article = Article(
        title="整體標題",
        summary="整體摘要",
        sections=[
            Section(title="背景", emoji="🧩", content="第一行\n第二行"),
            Section(title="收斂", emoji="🎯", content="最後一段"),
        ],
    )

    page_url = article.create_page()

    assert page_url == "https://telegra.ph/fake"
    assert captured["title"] == "整體標題"
    assert "🧩 背景<br><br>第一行<br>第二行" in captured["html_content"]
    assert "🎯 收斂<br><br>最後一段" in captured["html_content"]
