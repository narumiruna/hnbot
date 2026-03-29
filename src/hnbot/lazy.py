from typing import Final

from openai import OpenAI

_MODEL: Final[str] = "gpt-5-mini"
_TEMPERATURE: Final[float] = 0.0


def send(prompt: str, instructions: str | None = None) -> str:
    client = OpenAI()

    response = client.responses.parse(
        input=prompt,
        instructions=instructions,
        model=_MODEL,
        temperature=_TEMPERATURE,
    )

    if response.output_text is None:
        raise Exception("No text returned.")

    return response.output_text


def parse[T](prompt: str, text_format: type[T], instructions: str | None = None) -> T:
    client = OpenAI()

    response = client.responses.parse(
        input=prompt,
        instructions=instructions,
        model=_MODEL,
        temperature=_TEMPERATURE,
        text_format=text_format,
    )

    if response.output_parsed is None:
        raise Exception("No parsed message returned.")

    return response.output_parsed
