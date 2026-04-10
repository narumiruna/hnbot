import html

from chonkie.chunker import RecursiveChunker
from loguru import logger
from pydantic import BaseModel
from pydantic import Field

from hnbot.llm import async_parse
from hnbot.page import create_page
from hnbot.settings import Settings

ARTICLE_INSTRUCTIONS = """
Task:
Convert the input into a coherent blog post written entirely in {lang}. Return output strictly as the given schema.

Hard constraints:
- Preserve all materially important information from the input.
- Do not add new facts, entities, events, numbers, or claims.
- Use a professional, neutral, easy-to-read tone.
- Simplify complex wording when needed, but keep original meaning.
- Summary must be <= 500 characters.
- Each section.content must be <= 1000 characters.
- All content must be less than 5000 characters in total.

Section rules:
- Every section title must be specific and in {lang}.
- Every section emoji must be exactly one emoji.
- Every section body can have one or more paragraphs.
- Keep transitions smooth and the whole post cohesive
- The final section should be a closing section that only restates earlier points.

Final checks:
- Include opening, body, and closing coverage.
- Ensure all content is in {lang}.
- Ensure every constraint is satisfied.
"""


class Section(BaseModel):
    title: str = Field(..., description="The title of the section.")
    emoji: str = Field(..., description="An emoji to represent the section.")
    content: str = Field(
        ...,
        description=(
            "The content of the section, which may include multiple paragraphs and formatting. "
            "The content should be less than 1000 characters."
        ),
    )


class Article(BaseModel):
    title: str = Field(..., description="The title of the article.")
    summary: str = Field(..., description="A brief summary of the article.")
    sections: list[Section] = Field(..., description="A list of sections in the article.")

    def render_content_text(self) -> str:
        rendered_sections = [f"{section.emoji} {section.title}\n\n{section.content}" for section in self.sections]
        return "\n\n".join(rendered_sections)

    def create_page(self) -> str:
        text_content = self.render_content_text()
        page_url = create_page(
            self.title,
            html.escape(text_content).replace("\n", "<br>"),
        )

        logger.info("Telegraph page created: {}", page_url)
        return page_url


async def _generate_article(html_content: str, settings: Settings) -> Article:
    if not html_content.strip():
        return Article(
            title="無內容",
            summary="沒有可處理的內容。",
            sections=[Section(title="內容狀態", emoji="📌", content="[No content provided]")],
        )

    article = await async_parse(
        html_content,
        text_format=Article,
        instructions=ARTICLE_INSTRUCTIONS.format(lang=settings.article_lang),
    )

    logger.info("Article generated with title: {}", article.title)
    return article


async def generate_article(html_content: str, settings: Settings) -> Article:
    chunker = RecursiveChunker(chunk_size=settings.chunk_size)
    chunks = chunker.chunk(html_content)
    logger.info("Number of chunks created: {}", len(chunks))

    if len(chunks) <= 1:
        return await _generate_article(html_content, settings)

    articles = []
    for chunk in chunks:
        article = await _generate_article(chunk.text, settings)
        articles.append(article)

    article = await generate_article(
        "\n\n".join([article.render_content_text() for article in articles]),
        settings,
    )

    logger.info("Article generated with title: {}", article.title)
    return article
