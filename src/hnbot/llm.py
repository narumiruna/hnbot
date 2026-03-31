from openai import AsyncOpenAI
from openai import OpenAI

from hnbot.settings import get_settings


async def async_send(prompt: str, instructions: str | None = None) -> str:
    client = AsyncOpenAI()
    model = get_settings().openai_model

    response = await client.responses.create(
        input=prompt,
        instructions=instructions,
        model=model,
    )

    if response.output_text is None:
        raise RuntimeError("No text returned.")

    return response.output_text


def send(prompt: str, instructions: str | None = None) -> str:
    client = OpenAI()
    model = get_settings().openai_model

    response = client.responses.create(
        input=prompt,
        instructions=instructions,
        model=model,
    )

    if response.output_text is None:
        raise RuntimeError("No text returned.")

    return response.output_text


async def async_parse[T](prompt: str, text_format: type[T], instructions: str | None = None) -> T:
    client = AsyncOpenAI()
    model = get_settings().openai_model

    response = await client.responses.parse(
        input=prompt,
        instructions=instructions,
        model=model,
        text_format=text_format,
    )

    if response.output_parsed is None:
        raise RuntimeError("No parsed message returned.")

    return response.output_parsed


def parse[T](prompt: str, text_format: type[T], instructions: str | None = None) -> T:
    client = OpenAI()
    model = get_settings().openai_model

    response = client.responses.parse(
        input=prompt,
        instructions=instructions,
        model=model,
        text_format=text_format,
    )

    if response.output_parsed is None:
        raise RuntimeError("No parsed message returned.")

    return response.output_parsed
