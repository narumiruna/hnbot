from hnbot.article import INSTRUCTIONS
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


def test_generate_article_returns_sentinel_for_empty_input(monkeypatch) -> None:
    def fake_parse(prompt: str, text_format: type[Article], instructions: str | None = None) -> Article:
        msg = f"parse should not run for empty input: {prompt=} {text_format=} {instructions=}"
        raise AssertionError(msg)

    monkeypatch.setattr("hnbot.article.parse", fake_parse)
    article = generate_article(" \n\t ")
    assert article.title is None
    assert article.content == "[No content provided]"


def test_instructions_are_field_oriented_and_no_global_plain_text_rule() -> None:
    assert "Article.title" in INSTRUCTIONS
    assert "Article.content" in INSTRUCTIONS
    assert "Respond in plain text only." not in INSTRUCTIONS
    assert "The output must follow this order:" not in INSTRUCTIONS
    assert "INVALID_JSON" in INSTRUCTIONS
