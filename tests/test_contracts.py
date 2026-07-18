import json
from datetime import datetime
from pathlib import Path

import httpx

from hnbot.app import Notifier
from hnbot.article import Article
from hnbot.http_retry import retry_after_seconds
from hnbot.page import _sanitize_telegraph_html
from hnbot.rss import HNEntry
from hnbot.utils import html_to_markdown


def _contracts() -> dict:
    return json.loads(Path("tests/contracts/parity.json").read_text(encoding="utf-8"))


def test_shared_contract_fixtures_match_python_behavior() -> None:
    contracts = _contracts()

    assert html_to_markdown(contracts["html_to_markdown"]["input"]) == contracts["html_to_markdown"]["expected"]

    request = httpx.Request("GET", "https://example.com")
    for case in contracts["retry_after"]:
        response = httpx.Response(429, request=request, headers={"Retry-After": case["value"]})
        error = httpx.HTTPStatusError("limited", request=request, response=response)
        assert retry_after_seconds(error) == case["expected_seconds"]

    article = Article.model_validate(contracts["article"]["value"])
    assert article.render_content_text() == contracts["article"]["rendered"]

    assert _sanitize_telegraph_html(contracts["sanitizer"]["input"]) == contracts["sanitizer"]["expected"]

    message = contracts["message"]
    entry_data = message["entry"]
    entry = HNEntry(
        title=entry_data["title"],
        link=entry_data["link"],
        comment_url=entry_data["comment_url"],
        id=entry_data["id"],
        published_at=datetime.fromisoformat(entry_data["published_at"]),
        points=entry_data["points"],
        num_comments=entry_data["num_comments"],
    )
    article = Article.model_validate(message["article"])
    assert Notifier().build_message(entry, article, message["page_url"]) == message["expected"]
