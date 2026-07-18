# Hacker News Bot

A Rust service that monitors [Hacker News](https://news.ycombinator.com/), summarizes discussions with OpenAI, publishes readable notes to Telegraph, and sends them to Telegram.

## Features

- **RSS feed monitoring** — fetches stories from [hnrss.org](https://hnrss.org/) with a configurable points threshold
- **Discussion retrieval** — fetches each complete comment tree in one request from the HN Algolia API instead of rate-limited HN page scraping
- **AI summaries** — requests strict structured articles from the OpenAI Responses API
- **Telegraph publishing** — creates long-form public notes
- **Telegram notifications** — sends escaped HTML messages with source, discussion, and note links
- **Redis deduplication** — preserves processed entry keys across restarts
- **Continuous service execution** — processes the current feed, then polls sequentially
- **Bounded concurrency and pacing** — separates comment-fetch and article-pipeline limits and globally spaces HN requests
- **Retry and cooldown handling** — retries transient feed/comment errors and honors `Retry-After`
- **Structured stdout logs** — emits JSON through Rust `tracing` without logging credentials

## Runtime flow

```text
hnrss.org RSS feed
        │
        ▼
  Parse and filter entries
        │
        ▼
  Check Redis dedupe state
        │
        ▼
  Fetch Algolia item ──► comment HTML → Markdown
        │
        ▼
  OpenAI structured article
        │
        ▼
  Publish Telegraph page
        │
        ▼
  Send Telegram message
        │
        ▼
  Mark Redis key
```

## Quick start

### Docker Compose (recommended)

Prerequisites:

- Docker with Compose support
- Telegram bot token and target chat ID
- OpenAI API key

```sh
cp .env.example .env
# Fill in OPENAI_API_KEY, BOT_TOKEN, and CHAT_ID.

docker compose up --build -d
docker compose logs -f hnbot
```

Compose builds the Rust binary, starts Redis with persistent storage, and runs `hnbot serve`. Stop the services without deleting Redis state:

```sh
docker compose down
```

### Local development runtime

Install Rust 1.88+ and run Redis locally, then:

```sh
cp .env.example .env
# Fill in required values.

cargo run --release -- serve
```

The service processes one feed batch immediately and waits `FEED_POLL_INTERVAL_SECONDS` after each completed or failed batch. Override the interval for one invocation:

```sh
cargo run --release -- serve --poll-interval 60
```

The interval must be finite and at least one second. Bare `hnbot` displays help; `hnbot main` is not supported.

## Configuration

Settings are read from environment variables and an optional `.env` file. Unknown variables are ignored.

### Required

| Variable | Description |
|---|---|
| `OPENAI_API_KEY` | OpenAI API key |
| `BOT_TOKEN` | Telegram bot token |
| `CHAT_ID` | Telegram destination chat ID |

### Optional

| Variable | Default | Description |
|---|---:|---|
| `OPENAI_BASE_URL` | `https://api.openai.com/v1` | OpenAI-compatible API base URL |
| `OPENAI_MODEL` | `gpt-5-mini` | Responses API model |
| `OPENAI_TIMEOUT_SECONDS` | `120.0` | OpenAI generation request timeout |
| `ARTICLE_LANG` | `Traditional Chinese (台灣正體中文)` | Generated article language |
| `REDIS_HOST` | `localhost` | Redis host |
| `REDIS_PORT` | `6379` | Redis port |
| `REDIS_DB` | `0` | Redis database |
| `REDIS_PASSWORD` | unset | Password for an authenticated external Redis instance; Compose overrides this for its private Redis service |
| `HTTP_TIMEOUT_SECONDS` | `10.0` | General timeout for HNRSS, Telegraph, and Telegram requests |
| `HTTP_USER_AGENT` | `hnbot/0.0.0` | HTTP User-Agent |
| `COMMENTS_FETCH_CONCURRENCY` | `1` | Maximum concurrent HN comment fetches |
| `COMMENTS_FETCH_TIMEOUT_SECONDS` | `60.0` | HN comment API request timeout |
| `COMMENTS_FETCH_MIN_INTERVAL_SECONDS` | `2.0` | Minimum delay between HN request starts |
| `COMMENTS_FETCH_429_COOLDOWN_SECONDS` | `30.0` | Cooldown after comment API 429 without `Retry-After` |
| `HNBOT_COMMENTS_API_BASE_URL` | `https://hn.algolia.com/api/v1/items` | HN Algolia item API base URL |
| `ARTICLE_PIPELINE_CONCURRENCY` | `3` | Maximum concurrent generation/publishing pipelines |
| `CHUNK_SIZE` | `200000` | Unicode characters per generation chunk |
| `FEED_POINTS` | `200` | Minimum HN points threshold |
| `BATCH_SLEEP_SECONDS` | `0.5` | Delay before entry processing |
| `FEED_POLL_INTERVAL_SECONDS` | `30.0` | Delay between completed batches |

## Development

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
```

The standard aggregate gate is:

```sh
just all
```

Tests use local fixtures and mock HTTP adapters; they do not contact OpenAI, Telegram, Telegraph, HNRSS, or Redis.

## License

[MIT](LICENSE)
