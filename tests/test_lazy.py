from pydantic import BaseModel

import hnbot.lazy as lazy
from hnbot.settings import Settings


class OutputModel(BaseModel):
    value: str


class FakeCreateResponse:
    output_text = "ok"


class FakeParseResponse:
    output_parsed = OutputModel(value="parsed")


class FakeResponses:
    def __init__(self) -> None:
        self.create_calls: list[dict[str, object]] = []
        self.parse_calls: list[dict[str, object]] = []

    def create(self, **kwargs: object) -> FakeCreateResponse:
        self.create_calls.append(kwargs)
        return FakeCreateResponse()

    def parse(self, **kwargs: object) -> FakeParseResponse:
        self.parse_calls.append(kwargs)
        return FakeParseResponse()


class FakeOpenAI:
    def __init__(self) -> None:
        self.responses = FakeResponses()


def test_send_uses_openai_model_from_settings(monkeypatch) -> None:
    fake_client = FakeOpenAI()
    monkeypatch.setattr(lazy, "OpenAI", lambda: fake_client)
    monkeypatch.setattr(
        lazy,
        "get_settings",
        lambda: Settings.model_validate(
            {
                "bot_token": "bot-token",
                "chat_id": "chat-id",
                "openai_model": "gpt-5",
            }
        ),
    )

    result = lazy.send("prompt", instructions="instr")

    assert result == "ok"
    assert fake_client.responses.create_calls[0]["model"] == "gpt-5"


def test_parse_uses_openai_model_from_settings(monkeypatch) -> None:
    fake_client = FakeOpenAI()
    monkeypatch.setattr(lazy, "OpenAI", lambda: fake_client)
    monkeypatch.setattr(
        lazy,
        "get_settings",
        lambda: Settings.model_validate(
            {
                "bot_token": "bot-token",
                "chat_id": "chat-id",
                "openai_model": "gpt-5",
            }
        ),
    )

    result = lazy.parse("prompt", text_format=OutputModel, instructions="instr")

    assert result.value == "parsed"
    assert fake_client.responses.parse_calls[0]["model"] == "gpt-5"
