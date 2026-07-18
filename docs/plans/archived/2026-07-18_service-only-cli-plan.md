## Goal

Make `hnbot serve` the only supported runtime command, while preserving continuous polling, batch isolation, cleanup on shutdown, and the existing `--poll-interval` override.

## Context

`compose.yaml` already starts the application with `command: ["serve"]`. The one-shot paths are confined to the CLI, `App.run()` / `App._run_async()`, tests, and documentation. `App.serve()` calls `_serve_async()` directly, so it does not depend on the one-shot wrappers.

## Architecture

Keep the Typer application as a command group with an empty callback so the public syntax remains `hnbot serve`; a single Typer command without a callback may be flattened into the root command. Keep `_run_feed_batch()` because the service loop uses it for every poll.

## Non-Goals

- Do not remove the internal feed-batch boundary used by service polling.
- Do not remove `serve --poll-interval` or `FEED_POLL_INTERVAL_SECONDS`.
- Do not alter polling, retry, deduplication, concurrency, or shutdown behavior.
- Do not rewrite archived plans, which document historical behavior.

## Assumptions

- Bare `hnbot` should show help and must not process a feed batch.
- `hnbot main` may become an invalid command without a compatibility alias.
- Direct Docker invocations must pass `serve`; Compose remains unchanged because it already does so.

## Plan

- [x] Update `tests/test_cli.py` first to specify the service-only interface: bare `hnbot` shows help without creating a runtime app, `hnbot main` is rejected, help lists `serve` but not `main`, and the existing serve interval tests continue to pass; red phase verified with `uv run pytest tests/test_cli.py -v` (3 expected failures, 3 existing serve tests passed).
- [x] Simplify `src/hnbot/cli.py`: removed the one-shot callback dispatch, `main()` command, unused stdlib logger, and related imports; retained an empty callback with no-argument help so `hnbot serve` remains a subcommand; verified by 6 passing CLI tests and successful `uv run hnbot serve --help` output.
- [x] Remove `App.run()` and `App._run_async()` from `src/hnbot/app.py`; converted the two one-shot-oriented tests in `tests/test_app.py` to await `_run_feed_batch()` directly while retaining their success/deduplication and concurrency assertions and closing clients in `finally`; verified by 12 passing app tests.
- [x] Update `README.md`, `DESIGN_DOC.md`, and `AGENTS.md` to describe service-only operation; removed bare/`main` examples and the nonexistent scheduled cron workflow, required `serve` in direct Docker examples, and left `docs/plans/archived/` unchanged; the specified `rg` check returned no matches.
- [x] Smoke-test the public command contract: `uv run hnbot serve --help` exited 0 and listed `--poll-interval`; `uv run hnbot main` and bare `uv run hnbot` each exited 2, while isolated CLI tests verified neither creates a runtime app.
- [x] Run the complete repository gate with `just all`, then inspect the focused diff; Ruff format/check, ty, and all 58 tests passed, `git diff --check` passed, and focused diff review found only the intended service-only changes.

## Risks

- Removing the callback entirely could change Typer from a command group to a flattened single command and break `hnbot serve`.
- Bare `docker run --env-file .env hnbot` will no longer start work; documentation must require the explicit `serve` argument.
- Removing one-shot wrappers must not weaken tests for feed processing or resource cleanup; those behaviors remain covered through `_run_feed_batch()` and `_serve_async()`.

## Rollback / Recovery

This is a command-interface change with no data migration. Revert the focused commit to restore bare/`main` compatibility; Redis deduplication state is unaffected.

## Completion Checklist

- [x] `hnbot serve` is the only command shown by `uv run hnbot --help`, verified by CLI smoke output and 6 passing CLI tests.
- [x] Bare `hnbot` and `hnbot main` cannot process a batch, verified by exit status 2 in smoke checks and isolated `CliRunner` tests asserting zero runtime app creations.
- [x] Service polling and shutdown cleanup still pass, verified by 12 focused app tests and the complete 58-test suite.
- [x] Current documentation contains no supported one-shot or cron instructions, verified by the repository search in the plan returning no matches.
- [x] Formatting, linting, typing, and all tests pass, verified by `just all`.
