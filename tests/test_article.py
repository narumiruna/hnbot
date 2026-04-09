from hnbot.article import Article
from hnbot.article import Section


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
