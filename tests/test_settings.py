import pytest
from pydantic import ValidationError

from hnbot.settings import Settings
from hnbot.settings import get_settings


def test_settings_require_bot_token_and_chat_id(monkeypatch, tmp_path) -> None:
    get_settings.cache_clear()
    monkeypatch.chdir(tmp_path)
    monkeypatch.delenv("BOT_TOKEN", raising=False)
    monkeypatch.delenv("CHAT_ID", raising=False)

    with pytest.raises(ValidationError):
        get_settings()

    get_settings.cache_clear()


def test_settings_openai_model_default() -> None:
    settings = Settings.model_validate({"bot_token": "bot-token", "chat_id": "chat-id"})
    assert settings.openai_model == "gpt-5-mini"


def test_settings_openai_model_override() -> None:
    settings = Settings.model_validate({"bot_token": "bot-token", "chat_id": "chat-id", "openai_model": "gpt-5"})
    assert settings.openai_model == "gpt-5"


def test_settings_openai_model_from_env(monkeypatch) -> None:
    get_settings.cache_clear()
    monkeypatch.setenv("BOT_TOKEN", "bot-token")
    monkeypatch.setenv("CHAT_ID", "chat-id")
    monkeypatch.setenv("OPENAI_MODEL", "gpt-5")

    settings = get_settings()

    assert settings.openai_model == "gpt-5"
    get_settings.cache_clear()
