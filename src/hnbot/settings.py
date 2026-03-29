from functools import lru_cache

from pydantic_settings import BaseSettings
from pydantic_settings import SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(
        env_file=".env",
        env_file_encoding="utf-8",
        case_sensitive=False,
        extra="ignore",
    )

    openai_model: str = "gpt-5-mini"

    bot_token: str
    chat_id: str
    logfire_token: str | None = None

    redis_host: str = "localhost"
    redis_port: int = 6379
    redis_db: int = 0

    http_timeout_seconds: float = 10.0
    http_user_agent: str = "hnbot/0.0.0"
    max_comment_markdown_chars: int = 20_000


@lru_cache(maxsize=1)
def get_settings() -> Settings:
    return Settings()  # ty: ignore[missing-argument]
