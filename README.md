# Hacker News Bot

A Telegram bot that monitors [Hacker News](https://news.ycombinator.com/) for trending articles, generates AI-powered summaries of the discussions, and delivers them straight to your Telegram chat.

## Features

- **RSS feed monitoring** — fetches top stories from [hnrss.org](https://hnrss.org/) filtered by minimum points
- **AI-powered summaries** — uses OpenAI to distill HN comment threads into concise, structured articles
- **Telegraph publishing** — creates readable long-form pages on [Telegraph](https://telegra.ph/)
- **Telegram notifications** — sends formatted messages with links to the original article, HN discussion, and the generated summary
- **Redis deduplication** — tracks processed entries to avoid sending duplicate notifications
- **Concurrent processing** — handles multiple articles in parallel with configurable concurrency limits
- **Retry with backoff** — automatically retries transient HTTP failures

## How It Works

```
hnrss.org RSS feed
        │
        ▼
  Filter by points
        │
        ▼
  Fetch HN comments ──► HTML → Markdown
        │
        ▼
  OpenAI summarisation
        │
        ▼
  Publish to Telegraph
        │
        ▼
  Notify via Telegram
```

## Quick Start

### Prerequisites

- [Python 3.12+](https://www.python.org/)
- [uv](https://docs.astral.sh/uv/)
- A [Telegram Bot Token](https://core.telegram.org/bots#how-do-i-create-a-bot) and a target chat ID
- An [OpenAI API key](https://platform.openai.com/api-keys)
- A running [Redis](https://redis.io/) instance (for deduplication)

### Install & Run

```sh
# Clone the repository
git clone https://github.com/narumiruna/hnbot.git
cd hnbot

# Install dependencies
uv sync

# Configure environment variables
cp .env.example .env
# Edit .env and fill in the required values

# Run the bot
uv run hnbot
```

### Install from PyPI

```sh
pip install hnbot
```

### Docker

```sh
docker build -t hnbot .
docker run --env-file .env hnbot
```

## Configuration

All settings are loaded from environment variables (or a `.env` file). See [`.env.example`](.env.example) for the full template.

### Required

| Variable | Description |
|---|---|
| `OPENAI_API_KEY` | OpenAI API key |
| `BOT_TOKEN` | Telegram bot token |
| `CHAT_ID` | Telegram chat ID to receive notifications |

### Optional

| Variable | Default | Description |
|---|---|---|
| `OPENAI_BASE_URL` | *(OpenAI default)* | Custom OpenAI-compatible API endpoint |
| `OPENAI_MODEL` | `gpt-5-mini` | LLM model to use for summarisation |
| `LOGFIRE_TOKEN` | — | [Logfire](https://logfire.pydantic.dev/) token for observability |
| `REDIS_HOST` | `localhost` | Redis host |
| `REDIS_PORT` | `6379` | Redis port |
| `REDIS_DB` | `0` | Redis database number |
| `FEED_POINTS` | `100` | Minimum HN points threshold for feed entries |
| `HTTP_TIMEOUT_SECONDS` | `10.0` | HTTP request timeout (seconds) |
| `HTTP_USER_AGENT` | `hnbot/0.0.0` | HTTP User-Agent header |
| `MAX_COMMENT_MARKDOWN_CHARS` | `20000` | Maximum characters to keep from comment markdown |
| `COMMENTS_FETCH_CONCURRENCY` | `1` | Max parallel comment fetches |
| `ARTICLE_PIPELINE_CONCURRENCY` | `3` | Max parallel article generation tasks |
| `BATCH_SLEEP_SECONDS` | `0.5` | Delay before processing a batch |

## Development

```sh
# Install all dependencies (including dev)
uv sync

# Run the full development gate (format → lint → type-check → test)
just all

# Or run individual steps
just format   # ruff format
just lint     # ruff check --fix
just type     # ty check
just test     # pytest with coverage
```

## License

[MIT](LICENSE)
