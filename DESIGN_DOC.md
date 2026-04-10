# hnbot Design Document

## Purpose and Non-Goals

This document describes the current production-oriented design of `hnbot` as implemented today.

Purpose:
- Explain the end-to-end runtime flow and module boundaries.
- Document failure handling, concurrency controls, and operational knobs.
- Provide maintainers with a reliable reference for debugging and on-call work.

Non-goals:
- Proposing future architecture not present in the codebase.
- Defining product roadmap or feature backlog.
- Replacing API-level docs for third-party services.

## System Context

`hnbot` is a batch-style Telegram bot pipeline that:
- Reads Hacker News entries from `hnrss.org`.
- Fetches the linked HN discussion page HTML.
- Converts HTML to Markdown and summarizes it via OpenAI.
- Publishes a long-form page to Telegraph.
- Sends a formatted Telegram message to a target chat.
- Deduplicates processed entries in Redis.

External dependencies:
- `hnrss.org` RSS feed (`/newest?points=...`)
- OpenAI Responses API
- Telegraph API
- Telegram Bot API
- Redis

## Runtime and Deployment

Local runtime:
- Entrypoint: `hnbot` console script -> `hnbot.cli:app`.
- Main command: `uv run hnbot` (or `uv run hnbot main`).
- `.env` is loaded by CLI via `python-dotenv`.

Scheduled runtime:
- GitHub Actions workflow `.github/workflows/cron.yml` runs every 4 hours (`0 */4 * * *`) on a self-hosted runner.
- The cron job executes `uv run hnbot` with secrets for OpenAI/Telegram/logfire.

Execution model:
- A single invocation processes one feed batch and exits.
- There is no built-in infinite polling loop in `App.run()`.

## End-to-End Flow

```mermaid
flowchart LR
    A[hnrss.org RSS feed] --> B[Parse feed entries]
    B --> C[Sleep batch_sleep_seconds]
    C --> D[Check Redis key hnbot:entry:{id}]
    D -->|not processed| E[Fetch HN comments HTML]
    E --> F[Convert HTML to Markdown]
    F --> G[Retry on transient HTTP errors]
    G --> H[Generate structured article via OpenAI]
    H --> I[Create Telegraph page]
    I --> J[Build escaped HTML Telegram message]
    J --> K[Send Telegram message]
    K --> L[Set Redis dedupe key]
    D -->|already processed| M[Skip entry]
```

## Component Responsibilities

- `src/hnbot/cli.py`
- Loads environment variables, builds settings, configures logfire, and launches `App`.

- `src/hnbot/settings.py`
- Defines runtime configuration with `pydantic-settings`.
- Provides cached singleton settings via `get_settings()`.

- `src/hnbot/app.py`
- Orchestrates feed batch execution.
- Handles dedupe checks, retries, bounded concurrency, and pipeline error boundaries.
- Sends final Telegram notifications.

- `src/hnbot/rss.py`
- Fetches and parses HN RSS feed into typed `HNFeed` / `HNEntry` objects.
- Extracts entry ID from HN comment URL query (`id`).

- `src/hnbot/article.py`
- Defines article output schema (`Article`, `Section`).
- Calls OpenAI parse endpoint with strict instructions.
- Renders article text and creates Telegraph page.

- `src/hnbot/llm.py`
- Thin async wrapper around the OpenAI Responses API parse method.
- Reads model from settings (`openai_model`).

- `src/hnbot/page.py`
- Creates Telegraph client/account and page.
- Sanitizes HTML into Telegraph-compatible subset.

- `src/hnbot/utils.py`
- HTML -> Markdown conversion and whitespace normalization.
- Optional logfire configuration.

## Data Contracts

Core models:
- `HNEntry`
  - `title: str`
  - `link: str`
  - `comment_url: str`
  - `id: str`
  - `published_at: datetime` (normalized to UTC)
- `HNFeed`
  - `title: str`
  - `entries: list[HNEntry]`

Generated content models:
- `Section`
  - `title: str`
  - `emoji: str`
  - `content: str`
- `Article`
  - `title: str`
  - `summary: str`
  - `sections: list[Section]`

Redis contract:
- Key: `hnbot:entry:{entry.id}`
- Value: `entry.comment_url`
- Semantics: presence means "already processed" for future runs.
- TTL: none (persistent unless externally evicted/deleted).

Telegram message contract:
- HTML parse mode.
- Includes escaped title and summary.
- Includes links to source article, HN discussion, and Telegraph note.

## Reliability and Failure Handling

Comment fetch retry policy (`CommentFetcher._fetch_with_retry`):
- Maximum attempts: 3.
- Retry condition:
  - `httpx.RequestError`
  - `httpx.HTTPStatusError` with 429 or 5xx.
- Wait strategy:
  - If `Retry-After` header is present on HTTP status error, use it.
  - Otherwise exponential jitter (`initial=1`, `max=8`).

Per-entry failure boundaries:
- If comment fetch fails after retries: entry is skipped (returns `False`).
- If OpenAI rejects input with `BadRequestError` (for example `invalid_prompt`): entry is skipped.
- If article generation/page creation fails with `RuntimeError` or `ValueError`: entry is skipped.
- Failed entries are not marked in Redis.
- Processing of other entries continues (`asyncio.gather` over tasks).

## Concurrency and Throughput

Two independent semaphores are used per batch:
- Fetch semaphore: `comments_fetch_concurrency` (default `1`).
- Pipeline semaphore: `article_pipeline_concurrency` (default `3`).

Behavioral implications:
- Comment fetching can be serialized while generation runs in parallel.
- Final send order is not guaranteed to match feed order when pipeline tasks have different durations.

Batch pacing:
- `batch_sleep_seconds` delay is applied before processing feed entries.
- Default is `0.5` seconds.

## Configuration Reference

Required:
- `BOT_TOKEN`: Telegram bot token.
- `CHAT_ID`: target Telegram chat ID.

Common optional settings and defaults:
- `OPENAI_MODEL = gpt-5-mini`
- `OPENAI_BASE_URL` (OpenAI client-compatible endpoint override)
- `ARTICLE_LANG = Traditional Chinese (台灣正體中文)`
- `LOGFIRE_TOKEN` (enables instrumentation)
- `REDIS_HOST = localhost`
- `REDIS_PORT = 6379`
- `REDIS_DB = 0`
- `HTTP_TIMEOUT_SECONDS = 10.0`
- `HTTP_USER_AGENT = hnbot/0.0.0`
- `COMMENTS_FETCH_CONCURRENCY = 1` (must be >= 1)
- `ARTICLE_PIPELINE_CONCURRENCY = 3` (must be >= 1)
- `CHUNK_SIZE = 200000` (must be >= 1)
- `FEED_POINTS = 100` (must be >= 1)
- `BATCH_SLEEP_SECONDS = 0.5` (must be >= 0.0)

## Observability

Logging:
- Runtime logs are emitted via `loguru`.

Optional logfire integration:
- Enabled only when `LOGFIRE_TOKEN` is set.
- Instruments OpenAI and Redis via `logfire.instrument_openai()` and `logfire.instrument_redis()`.

## Security and Privacy

Secrets:
- API keys/tokens are expected from environment variables (or local `.env`).
- `.env` must remain local and uncommitted.

Data flow considerations:
- HN discussion content is sent to OpenAI for summarization.
- Generated content is published to Telegraph (public URL by design).
- Final message content is delivered to Telegram chat.

## Testing and Coverage Snapshot

Current tests validate:
- Settings requirements/defaults and validation constraints.
- RSS parsing behavior and entry reversal.
- Retry behavior for transient fetch errors.
- Invalid-prompt article generation failures are skipped without marking Redis state.
- Markdown truncation behavior by configured max length.
- Parallel pipeline behavior (including non-deterministic send order).
- Message HTML escaping for title/summary/links.
- OpenAI model propagation from settings in the LLM wrapper.

Known test gaps:
- No full integration test against real external services.
- Limited direct tests for `page.py` sanitizer edge cases.
- Feed fetch (`get_hn_feed`) retry is not explicitly covered.

## Known Limitations

- Single-run batch execution; no continuous daemon loop.
- Dedupe key has no TTL and can grow without a retention policy.
- Heavy dependence on external service availability and latency.
- Feed-level fetch is not wrapped in retry logic inside `App._run_feed_batch()`.
- Telegraph account creation is performed at page creation time.
