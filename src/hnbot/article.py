import html

from loguru import logger
from pydantic import BaseModel

from hnbot.lazy import parse
from hnbot.page import create_page

INSTRUCTIONS = """
You are filling a structured response with two fields: Article.title and Article.content.
Do not output system notes, parser/debug messages, or meta commentary.

Global requirements:
- Translate and rewrite the input into {lang}.
- Do not add new facts, entities, events, or claims beyond the input.
- Preserve materially important points while improving clarity and flow.
- Never include diagnostic/error strings in any field, including tokens like INVALID_JSON, INVALID_TOKEN, UTF8, or parser failure messages.

Article.title requirements:
- A clear, specific blog-post title in {lang}.
- No emoji in title.
- Avoid generic titles such as "Article" or "Summary".

Article.content requirements:
- A professional, neutral blog post body in {lang}.
- Plain text paragraphs only (no HTML, Markdown, JSON, or code fences).
- Use one or more sections.
- Each section begins with a standalone heading line in this exact shape: <emoji> <section title>.
- Section title must be specific and in {lang}.
- Body starts on the next line and may contain one or more paragraphs.
- Use heading lines only for section boundaries (no nested headings).
- Final section should function as a closing summary and only restate earlier points.
- Keep transitions smooth and the full article cohesive.
"""  # noqa: E501


class Article(BaseModel):
    content: str
    title: str | None = None

    def build_text(self) -> str:
        if self.title:
            return f"📝 {self.title}\n\n{self.content}"
        return self.content

    def create_page(self) -> str:
        page_url = create_page(
            self.title or "HN Article",
            html.escape(self.content).replace("\n", "<br>"),
        )

        logger.info("Telegraph page created: {}", page_url)
        return page_url


def generate_article(html_content: str, lang: str = "Taiwanese") -> Article:
    if not html_content.strip():
        return Article(content="[No content provided]")

    article = parse(
        html_content,
        text_format=Article,
        instructions=INSTRUCTIONS.format(lang=lang),
    )
    logger.info("Article generated with title: {}", article.title)
    return article
