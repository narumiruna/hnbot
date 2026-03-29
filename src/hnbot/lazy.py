from typing import Final

from openai import OpenAI

_MODEL: Final[str] = "gpt-5-mini"


def send(prompt: str, instructions: str | None = None) -> str:
    client = OpenAI()

    response = client.responses.create(
        input=prompt,
        instructions=instructions,
        model=_MODEL,
    )

    if response.output_text is None:
        raise RuntimeError("No text returned.")

    return response.output_text


def parse[T](prompt: str, text_format: type[T], instructions: str | None = None) -> T:
    client = OpenAI()

    response = client.responses.parse(
        input=prompt,
        instructions=instructions,
        model=_MODEL,
        text_format=text_format,
    )

    if response.output_parsed is None:
        raise RuntimeError("No parsed message returned.")

    return response.output_parsed
