import logfire
from openai import AsyncOpenAI
from openai import OpenAI

from hnbot.settings import get_settings


async def async_send(prompt: str, instructions: str | None = None) -> str:
    client = AsyncOpenAI()
    settings = get_settings()

    with logfire.span(
        "hnbot.llm.send",
        model=settings.openai_model,
        prompt_chars=len(prompt),
    ):
        response = await client.responses.create(
            input=prompt,
            instructions=instructions,
            model=settings.openai_model,
        )

        if response.output_text is None:
            raise RuntimeError("No text returned.")

        return response.output_text


def send(prompt: str, instructions: str | None = None) -> str:
    client = OpenAI()
    settings = get_settings()

    with logfire.span(
        "hnbot.llm.send",
        model=settings.openai_model,
        prompt_chars=len(prompt),
    ):
        response = client.responses.create(
            input=prompt,
            instructions=instructions,
            model=settings.openai_model,
        )

        if response.output_text is None:
            raise RuntimeError("No text returned.")

        return response.output_text


async def async_parse[T](prompt: str, text_format: type[T], instructions: str | None = None) -> T:
    client = AsyncOpenAI()
    settings = get_settings()

    with logfire.span(
        "hnbot.llm.parse",
        model=settings.openai_model,
        prompt_chars=len(prompt),
        text_format=text_format.__name__,
    ):
        response = await client.responses.parse(
            input=prompt,
            instructions=instructions,
            model=settings.openai_model,
            text_format=text_format,
        )

        if response.output_parsed is None:
            raise RuntimeError("No parsed message returned.")

        return response.output_parsed


def parse[T](prompt: str, text_format: type[T], instructions: str | None = None) -> T:
    client = OpenAI()
    settings = get_settings()

    with logfire.span(
        "hnbot.llm.parse",
        model=settings.openai_model,
        prompt_chars=len(prompt),
        text_format=text_format.__name__,
    ):
        response = client.responses.parse(
            input=prompt,
            instructions=instructions,
            model=settings.openai_model,
            text_format=text_format,
        )

        if response.output_parsed is None:
            raise RuntimeError("No parsed message returned.")

        return response.output_parsed
