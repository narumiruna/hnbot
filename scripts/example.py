import asyncio
from pathlib import Path

import httpx
import typer
from dotenv import find_dotenv
from dotenv import load_dotenv

from hnbot.article import generate_article
from hnbot.settings import get_settings
from hnbot.utils import html_to_markdown


def main(id: int = 47581701) -> None:
    load_dotenv(find_dotenv(), override=True)

    settings = get_settings()

    tmp_dir = Path("./tmp")
    tmp_dir.mkdir(exist_ok=True)

    url = f"https://news.ycombinator.com/item?id={id}"

    resp = httpx.get(url)
    resp.raise_for_status()

    (tmp_dir / "sample.html").write_text(resp.text, encoding="utf-8")

    markdown = html_to_markdown(resp.text)
    print(f"Markdown content: {markdown[:100]}...")

    (tmp_dir / "sample.md").write_text(markdown, encoding="utf-8")

    article = asyncio.run(generate_article(markdown, settings))

    print(f"Generated article title: {article.title}")
    print(f"Generated article summary: {article.summary}")
    print(f"Generated article content: {article.render_content_text()[:100]}...")

    (tmp_dir / "article.txt").write_text(article.render_content_text(), encoding="utf-8")

    page_url = article.create_page()
    print(f"Telegraph page URL: {page_url}")


if __name__ == "__main__":
    typer.run(main)
