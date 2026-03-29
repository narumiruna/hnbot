import os
from pathlib import Path

import charset_normalizer
import logfire
from loguru import logger
from markdownify import markdownify


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


def logfire_is_enabled() -> bool:
    return bool(os.getenv("LOGFIRE_TOKEN"))


def configure_logfire() -> None:
    if not logfire_is_enabled():
        return

    logfire.configure()
    # logfire.instrument_openai_agents()
    logfire.instrument_openai()
    logfire.instrument_redis()
    logger.configure(handlers=[logfire.loguru_handler()])
