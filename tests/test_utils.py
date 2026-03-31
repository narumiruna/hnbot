from hnbot.utils import normalize_whitespace


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
