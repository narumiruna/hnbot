import html

from loguru import logger
from pydantic import BaseModel

from hnbot.llm import async_parse
from hnbot.llm import parse
from hnbot.page import create_page

SUMMARIZE_INSTRUCTIONS = """
Task:
Read the input and write a concise summary in Traditional Chinese (台灣正體中文) of no more than 100 characters.

Hard constraints:
- The summary MUST be written in Traditional Chinese (台灣正體中文).
- Keep the summary to 100 characters or fewer.
- Focus on the single most important point, finding, or topic.
- Do not introduce facts, entities, or numbers not found in the input.
- Use plain prose only — no bullet points, no line breaks, no markdown.
"""

INSTRUCTIONS = """
Task:
Convert the input into a coherent blog post written entirely in {lang}.

Hard constraints:
- Preserve all materially important information from the input.
- Do not add new facts, entities, events, numbers, or claims.
- Use a professional, neutral, easy-to-read tone.
- Simplify complex wording when needed, but keep original meaning.
- Plain text only: do NOT use Markdown or HTML anywhere in the output.
  Forbidden: # headings, ## headings, **bold**, *italic*, bullet markers (-, *, +),
  numbered list markers (1. 2. 3.), <tag> HTML tags.

Output schema (use these exact labels, nothing else before "Title:" or between "Title:" and "Content:"):
Title:
<one specific title in {lang}, no emoji, not generic like "Article" or "Summary">

Content:
<one or more sections in {lang}>

Section rules:
- Each section starts with exactly one heading line in the format: <emoji> <section title>
  Example: 🔍 研究背景
- The section title must be specific and written in {lang}.
- The section body starts on the next line after the heading and contains one or more plain-text paragraphs.
- Do not add extra heading lines inside the same section.
- Do not use Markdown or HTML heading syntax, bullet markers, or numbered list markers
  anywhere — not even inside section bodies.
- Keep transitions smooth and the whole post cohesive.
- The final section is a closing section that only restates earlier points.

Final checks:
- Include opening, body, and closing coverage.
- Ensure all content is in {lang}.
- Ensure every constraint above is satisfied.
"""


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


async def generate_article_async(html_content: str, lang: str = "Traditional Chinese (台灣正體中文)") -> Article:
    if not html_content.strip():
        return Article(content="[No content provided]")

    article = await async_parse(
        html_content,
        text_format=Article,
        instructions=INSTRUCTIONS.format(lang=lang),
    )
    logger.info("Article generated with title: {}", article.title)
    return article


class Summary(BaseModel):
    text: str


async def summarize_async(content: str) -> Summary:
    if not content.strip():
        return Summary(text="")

    summary = await async_parse(
        content,
        text_format=Summary,
        instructions=SUMMARIZE_INSTRUCTIONS,
    )
    logger.info("Summary generated: {}", summary.text)
    return summary
