from functools import lru_cache

from pydantic import Field
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

    article_lang: str = "Traditional Chinese (台灣正體中文)"

    logfire_token: str | None = None

    redis_host: str = "localhost"
    redis_port: int = 6379
    redis_db: int = 0

    http_timeout_seconds: float = 10.0
    http_user_agent: str = "hnbot/0.0.0"
    comments_fetch_concurrency: int = Field(default=1, ge=1)
    article_pipeline_concurrency: int = Field(default=3, ge=1)
    chunk_size: int = Field(default=200_000, ge=1)

    feed_points: int = Field(default=200, ge=1)
    batch_sleep_seconds: float = Field(default=0.5, ge=0.0)


@lru_cache(maxsize=1)
def get_settings() -> Settings:
    return Settings()  # ty: ignore[missing-argument]
