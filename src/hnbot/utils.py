from pathlib import Path

import charset_normalizer
import logfire
from loguru import logger
from markdownify import markdownify

from hnbot.settings import Settings


def normalize_whitespace(text: str) -> str:
    lines = []
    for line in text.splitlines():
        stripped = line.strip()
        if stripped:
            lines += [stripped]
    return "\n".join(lines)


def html_to_markdown(content: str | bytes) -> str:
    """Convert HTML content to markdown format.

    Args:
        content: HTML content as string or bytes

    Returns:
        Converted markdown text with normalized whitespace
    """
    if isinstance(content, bytes):
        content = str(charset_normalizer.from_bytes(content).best())

    md = markdownify(content, strip=["a", "img"])
    return normalize_whitespace(md)


def read_html_content(f: str | Path) -> str:
    content = str(charset_normalizer.from_path(f).best())

    md = markdownify(content, strip=["a", "img"])
    return normalize_whitespace(md)


def logfire_is_enabled(settings: Settings) -> bool:
    return bool(settings.logfire_token)


def configure_logfire(settings: Settings) -> None:
    token = settings.logfire_token
    if not token:
        return

    logfire.configure(token=token)
    logfire.instrument_openai()
    logfire.instrument_redis()
    logger.configure(handlers=[logfire.loguru_handler()])
