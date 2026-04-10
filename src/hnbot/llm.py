from openai import AsyncOpenAI

from hnbot.settings import get_settings


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
