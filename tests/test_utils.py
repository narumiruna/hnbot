from hnbot.utils import html_to_markdown
from hnbot.utils import normalize_whitespace


def test_html_to_markdown_converts_string_content() -> None:
    html = """
        <h2>Title</h2>
        <p>Read <a href="https://example.com">linked text</a>.</p>
        <img src="diagram.png" alt="diagram">
    """

    assert html_to_markdown(html) == "Title\n-----\nRead linked text."


def test_normalize_whitespace_strips_and_joins() -> None:
    text = "  hello  \n  world  \n  "
    assert normalize_whitespace(text) == "hello\nworld"


def test_normalize_whitespace_removes_blank_lines() -> None:
    text = "first\n\n\n  \nsecond"
    assert normalize_whitespace(text) == "first\nsecond"


def test_normalize_whitespace_empty_string() -> None:
    assert normalize_whitespace("") == ""


def test_normalize_whitespace_only_whitespace() -> None:
    assert normalize_whitespace("   \n  \n   ") == ""
