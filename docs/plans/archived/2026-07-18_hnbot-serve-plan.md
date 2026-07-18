## Goal

Add a long-running `hnbot serve` command that immediately processes Redis-unseen entries from the current HN RSS feed, then polls again at a configurable interval without overlapping batches. Preserve the existing one-shot `hnbot` behavior.

## Architecture

- Keep the Typer command layer thin: bare `hnbot` and `hnbot main` run one batch, while `hnbot serve` delegates continuous execution to `App`.
- Add `FEED_POLL_INTERVAL_SECONDS` with a 30-second default and a one-second minimum; `hnbot serve --poll-interval` overrides it.
- Run batches sequentially. A completed or failed batch is followed by the configured delay, and a normal batch exception is logged before polling continues.
- Await every entry task before leaving a batch. Unexpected entry exceptions fail the one-shot command after sibling tasks finish, while serve mode logs the batch failure and continues.
- Preserve Redis as the durable success marker. Failed entries remain unseen and are retried on a later poll.

## Non-Goals

- Do not replace the existing cron workflow or Docker entrypoint.
- Do not add dependencies or redesign Redis, Telegram, Telegraph, or OpenAI idempotency.
- Do not independently poll while an article batch is still processing.

## Assumptions

- The first service poll processes all current feed entries not already present in Redis, covering downtime between service runs.
- Delivery retains the existing at-least-once boundary between external side effects and the Redis success marker.

## Plan

- [x] Add failing tests for polling settings, CLI dispatch and interval precedence, sequential service polling, batch failure recovery, entry-task completion, and cancellation cleanup; verified three targeted tests failed for the missing setting, CLI command, and service loop.
- [x] Add the validated polling setting and Typer command group while preserving bare `hnbot`; verified 21 CLI and settings tests pass.
- [x] Implement the sequential service loop, batch exception boundary, entry-task exception aggregation, and graceful client cleanup; verified 11 app tests pass.
- [x] Update `.env.example`, README, and `DESIGN_DOC.md` for batch and service operation; verified all three documented CLI forms against live help output.
- [x] Run the repository quality gate and CLI smoke checks; `just all` passed Ruff format/lint, ty, and 51 tests with 79% coverage, and all three CLI help commands exited successfully.

## Risks

- Catching cancellation as a normal batch error would prevent shutdown; only `Exception` is caught so cancellation propagates to cleanup.
- Returning early from `asyncio.gather` could allow task overlap; gather all task results before propagating unexpected entry failures.
- Converting the single Typer command into a group could break existing cron and Docker use; an invoke-without-command callback preserves bare `hnbot`.

## Completion Checklist

- [x] Bare `hnbot` and explicit `hnbot main` each run exactly one batch, verified by CLI tests and successful help smoke checks.
- [x] `hnbot serve` immediately runs one batch and continues sequential polling with CLI-over-environment interval precedence, verified by deterministic CLI and service-loop tests.
- [x] Batch failures do not terminate serve mode, cancellation closes HTTP and Redis clients, and no entry tasks overlap the next batch, verified by 11 passing app tests.
- [x] Configuration and operating modes are documented in `.env.example`, README, and `DESIGN_DOC.md`, verified by diff review against the implementation.
- [x] Formatting, lint, type checking, and all 51 tests pass via `just all` with 79% coverage.
