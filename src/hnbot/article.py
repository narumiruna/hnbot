import html

from loguru import logger
from pydantic import BaseModel

from hnbot.lazy import parse
from hnbot.page import create_page

INSTRUCTIONS = """
Extract, reorganize, and translate the input text into {lang} as a readable blog post.
Do not add new facts, entities, events, or claims beyond the input.
Preserve all materially important points from the input while improving clarity and flow.

Generate a clear, specific blog post title in {lang} that reflects the reorganized key points and important content.
Do not use generic titles such as “Article” or “Summary”.

Rewrite the content as a professional, neutral blog post in plain and accessible language.
Prioritize readability and coherent narrative flow for general readers.
If the input is legal, technical, or otherwise complex, simplify wording while preserving essential facts and meaning.

Title:
Title line in {lang}

Content:
One or more content sections in {lang}
Each content section must follow all rules below:
- The first line must be a standalone section heading in this exact pattern: <emoji> <section title>
- The section title must be specific and written in {lang}
- The section body must start on the next line and may contain one or more paragraphs
- Use section heading lines only for section boundaries; do not add extra heading lines inside the same section
- Do not use Markdown, HTML, bullet markers, or numbered list markers as section labels
The final content section must act as the closing section and only restate points already present in earlier sections.
The title line itself must not include an emoji.
Maintain smooth transitions between paragraphs and keep the full post cohesive.


Final checks:
Ensure the output is a complete, readable blog post with a title and coherent sections, including opening, body, and closing coverage.
Ensure all important points from the input are preserved without adding new facts.
Ensure all content is translated into {lang} and written in a professional, neutral tone.
Ensure every requirement above is satisfied.
Make only minimal revisions during final review.

ALL output MUST be written entirely in {lang}.
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
