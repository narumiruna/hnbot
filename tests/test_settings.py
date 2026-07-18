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


def test_settings_async_concurrency_defaults() -> None:
    settings = Settings.model_validate({"bot_token": "bot-token", "chat_id": "chat-id"})

    assert settings.comments_fetch_concurrency == 1
    assert settings.article_pipeline_concurrency == 3


def test_settings_comment_fetch_pacing_defaults() -> None:
    settings = Settings.model_validate({"bot_token": "bot-token", "chat_id": "chat-id"})

    assert settings.comments_fetch_min_interval_seconds == 2.0
    assert settings.comments_fetch_429_cooldown_seconds == 30.0


def test_settings_comment_fetch_pacing_must_be_finite_and_non_negative() -> None:
    for invalid_duration in (-1.0, float("nan"), float("inf")):
        with pytest.raises(ValidationError):
            Settings.model_validate(
                {
                    "bot_token": "bot-token",
                    "chat_id": "chat-id",
                    "comments_fetch_min_interval_seconds": invalid_duration,
                }
            )

        with pytest.raises(ValidationError):
            Settings.model_validate(
                {
                    "bot_token": "bot-token",
                    "chat_id": "chat-id",
                    "comments_fetch_429_cooldown_seconds": invalid_duration,
                }
            )


def test_settings_async_concurrency_must_be_positive() -> None:
    with pytest.raises(ValidationError):
        Settings.model_validate(
            {
                "bot_token": "bot-token",
                "chat_id": "chat-id",
                "comments_fetch_concurrency": 0,
            }
        )


def test_settings_feed_points_default() -> None:
    settings = Settings.model_validate({"bot_token": "bot-token", "chat_id": "chat-id"})
    assert settings.feed_points == 200


def test_settings_feed_points_override() -> None:
    settings = Settings.model_validate({"bot_token": "bot-token", "chat_id": "chat-id", "feed_points": 200})
    assert settings.feed_points == 200


def test_settings_feed_points_must_be_positive() -> None:
    with pytest.raises(ValidationError):
        Settings.model_validate({"bot_token": "bot-token", "chat_id": "chat-id", "feed_points": 0})


def test_settings_batch_sleep_seconds_default() -> None:
    settings = Settings.model_validate({"bot_token": "bot-token", "chat_id": "chat-id"})
    assert settings.batch_sleep_seconds == 0.5


def test_settings_batch_sleep_seconds_override() -> None:
    settings = Settings.model_validate({"bot_token": "bot-token", "chat_id": "chat-id", "batch_sleep_seconds": 1.0})
    assert settings.batch_sleep_seconds == 1.0


def test_settings_batch_sleep_seconds_zero_is_valid() -> None:
    settings = Settings.model_validate({"bot_token": "bot-token", "chat_id": "chat-id", "batch_sleep_seconds": 0.0})
    assert settings.batch_sleep_seconds == 0.0


def test_settings_feed_poll_interval_seconds_default() -> None:
    settings = Settings.model_validate({"bot_token": "bot-token", "chat_id": "chat-id"})

    assert settings.feed_poll_interval_seconds == 30.0


def test_settings_feed_poll_interval_seconds_override() -> None:
    settings = Settings.model_validate(
        {
            "bot_token": "bot-token",
            "chat_id": "chat-id",
            "feed_poll_interval_seconds": 5.0,
        }
    )

    assert settings.feed_poll_interval_seconds == 5.0


def test_settings_feed_poll_interval_seconds_must_be_at_least_one() -> None:
    with pytest.raises(ValidationError):
        Settings.model_validate(
            {
                "bot_token": "bot-token",
                "chat_id": "chat-id",
                "feed_poll_interval_seconds": 0.5,
            }
        )
