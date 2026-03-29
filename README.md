# Hacker News Bot

## Usage
```sh
# Edit .env with your own values
cp .env.example .env

uv run hnbot
```

## Required settings
- `BOT_TOKEN`
- `CHAT_ID`

## Optional settings
- `OPENAI_MODEL` (default: `gpt-5-mini`)
- `LOGFIRE_TOKEN`
- `REDIS_HOST`, `REDIS_PORT`, `REDIS_DB`
- `HTTP_TIMEOUT_SECONDS`, `HTTP_USER_AGENT`
- `MAX_COMMENT_MARKDOWN_CHARS`
