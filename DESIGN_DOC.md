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

`hnbot` is a long-running Telegram bot pipeline that:
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
- Service command: `uv run hnbot serve`.
- `.env` is loaded by CLI via `python-dotenv`.

Execution model:
- The service immediately processes one feed batch, waits `feed_poll_interval_seconds`, and repeats.
- Batches never overlap; the polling delay starts only after every entry task from the current batch has finished.

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
- Handles dedupe checks, bounded concurrency, and pipeline error boundaries.
- Sends final Telegram notifications.

- `src/hnbot/http_retry.py`
- Defines the shared transient HTTP retry policy for idempotent external GET requests.
- Retries `httpx` request failures, HTTP 429, and HTTP 5xx with bounded backoff and retry logging.

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

Shared transient HTTP retry policy (`hnbot.http_retry`):
- Applied to HNRSS feed fetch (`get_hn_feed`) and HN comment fetch (`CommentFetcher._fetch_with_retry`).
- Maximum attempts: 3.
- Retry condition:
  - `httpx.RequestError` (for example `ConnectTimeout`).
  - `httpx.HTTPStatusError` with 429 or 5xx.
- Wait strategy:
  - If `Retry-After` header is present on HTTP status error and can be parsed, use it.
  - Otherwise exponential jitter (`initial=1`, `max=8`).
- HN comment requests additionally share a process-wide pacer:
  - Request starts are separated by `comments_fetch_min_interval_seconds` (default `2.0`).
  - A 429 defers all later comment requests for `Retry-After`, or `comments_fetch_429_cooldown_seconds`
    (default `30.0`) when that header is absent.

Batch/feed failure boundary:
- If HNRSS feed fetch fails after retries, the batch raises the final HTTP error after retry evidence is logged.
- The service logs the batch exception and starts the next poll after the configured interval.

Per-entry failure boundaries:
- If comment fetch fails after retries: entry is skipped (returns `False`).
- If OpenAI rejects input with `BadRequestError` (for example `invalid_prompt`): entry is skipped.
- If article generation/page creation fails with `RuntimeError` or `ValueError`: entry is skipped.
- Failed entries are not marked in Redis.
- Processing of other entries continues. All entry tasks are awaited before unexpected entry exceptions are propagated.

## Concurrency and Throughput

Two independent semaphores are used per batch:
- Fetch semaphore: `comments_fetch_concurrency` (default `1`).
- Pipeline semaphore: `article_pipeline_concurrency` (default `3`).

Behavioral implications:
- Comment fetching can be serialized while generation runs in parallel.
- Comment request starts remain paced even if fetch concurrency is increased.
- Final send order is not guaranteed to match feed order when pipeline tasks have different durations.

Batch pacing:
- `batch_sleep_seconds` delay is applied before processing feed entries.
- Default is `0.5` seconds.

Service polling:
- `feed_poll_interval_seconds` controls the delay after a completed or failed batch.
- Default is `30.0` seconds and the minimum is `1.0` second.
- `hnbot serve --poll-interval` overrides the configured value for one process.

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
- `COMMENTS_FETCH_MIN_INTERVAL_SECONDS = 2.0` (must be finite and >= 0.0)
- `COMMENTS_FETCH_429_COOLDOWN_SECONDS = 30.0` (must be finite and >= 0.0)
- `ARTICLE_PIPELINE_CONCURRENCY = 3` (must be >= 1)
- `CHUNK_SIZE = 200000` (must be >= 1)
- `FEED_POINTS = 100` (must be >= 1)
- `BATCH_SLEEP_SECONDS = 0.5` (must be >= 0.0)
- `FEED_POLL_INTERVAL_SECONDS = 30.0` (must be >= 1.0)

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
- Retry behavior for transient feed fetch and comment fetch errors.
- Invalid-prompt article generation failures are skipped without marking Redis state.
- Markdown truncation behavior by configured max length.
- Parallel pipeline behavior (including non-deterministic send order).
- Message HTML escaping for title/summary/links.
- OpenAI model propagation from settings in the LLM wrapper.
- Service CLI dispatch, polling interval precedence, sequential polling, failure recovery, and cancellation cleanup.

Known test gaps:
- No full integration test against real external services.
- Limited direct tests for `page.py` sanitizer edge cases.

## Known Limitations

- Service mode does not fetch another feed while the current batch is still processing.
- Dedupe key has no TTL and can grow without a retention policy.
- Heavy dependence on external service availability and latency after bounded HTTP retries are exhausted.
- OpenAI generation, Telegraph page creation, and Telegram sending are not retried because they can have non-idempotent side effects or extra cost.
- Telegraph account creation is performed at page creation time.
