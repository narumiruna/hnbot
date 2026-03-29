from hnbot.article import Article
from hnbot.article import generate_article


def test_generate_article_uses_parse(monkeypatch) -> None:
    expected = Article(title="T", content="C")
    captured: dict[str, str] = {}

    def fake_parse(prompt: str, text_format: type[Article], instructions: str | None = None) -> Article:
        captured["prompt"] = prompt
        captured["instructions"] = instructions or ""
        assert text_format is Article
        return expected

    monkeypatch.setattr("hnbot.article.parse", fake_parse)
    article = generate_article("input-html", lang="Taiwanese")

    assert article == expected
    assert captured["prompt"] == "input-html"
    assert "Taiwanese" in captured["instructions"]
