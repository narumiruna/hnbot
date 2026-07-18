## Goal

Make runtime dependency declarations match actual imports: depend directly on Redis, remove unused direct dependencies, and remove unused byte-decoding support while preserving string HTML-to-Markdown conversion.

## Context

`src/hnbot/app.py` imports `redis.asyncio`, but Redis is currently installed only through the unused `aiocache[redis]` dependency. `rich` is declared directly but not imported by hnbot. `html_to_markdown()` supports `bytes` through `charset_normalizer`, although every production caller passes `httpx.Response.text` as `str`; `charset_normalizer` is also available only transitively. Deptry confirmed the unused `aiocache` and `rich` declarations and the transitive `redis` / `charset_normalizer` imports.

## Tech Stack

- Dependency and lockfile management: `uv`
- Runtime packages affected: `redis`, `aiocache`, `rich`, and the transitive `charset-normalizer`
- Verification: pytest, Ruff, ty, deptry, and `uv tree`

## Non-Goals

- Do not remove Redis, Redis deduplication, Typer rich-formatted help, or Logfire.
- Do not require `rich` or `charset-normalizer` to disappear transitively from `uv.lock`; Typer, Logfire, Telegraph, or their dependencies may still require them.
- Do not redesign HTML normalization beyond narrowing the unused bytes input contract.
- Do not upgrade unrelated packages.

## Assumptions

- No supported caller passes `bytes` to `html_to_markdown()`; repository production callers pass `str`.
- The currently resolved Redis major version is compatible with `redis.asyncio` usage in `App`.

## Plan

- [x] Add a focused string-input test for `html_to_markdown()` in `tests/test_utils.py` before changing its contract, covering HTML conversion, link/image stripping, and whitespace normalization; verified against the current implementation with `uv run pytest tests/test_utils.py -v` (5 passed).
- [x] Narrow `html_to_markdown()` in `src/hnbot/utils.py` from `str | bytes` to `str` and remove the `charset_normalizer` import and byte-decoding branch; verified with 17 focused tests and a passing `uv run ty check src tests scripts`.
- [x] Add Redis as a direct project dependency with `uv add redis`, then remove the unused cache abstraction with `uv remove aiocache`; verified `redis>=7.4.0` is direct, `redis.asyncio` imports successfully at 7.4.0, and `aiocache` is absent from the manifest, lockfile, and codebase.
- [x] Remove the unused direct Rich declaration with `uv remove rich`; verified it is absent from `pyproject.toml` and `uv tree --depth 1`, while `uv tree --invert --package rich` confirms it remains transitively required by Typer and Logfire.
- [x] Audit dependency correctness with `uv run --with deptry deptry . --per-rule-ignores DEP003=hnbot`; the focused audit reported `Success! No dependency issues found` while ignoring only hnbot's internal absolute imports.
- [x] Run `uv lock --check`, the complete repository gate with `just all`, and `git diff --check`; lock validation, Ruff format/check, ty, and all 59 tests passed, and focused diff review confirmed only aiocache/direct dependency metadata changed in `uv.lock` with no unrelated upgrades.

## Risks

- Running separate `uv add` / `uv remove` commands may re-resolve unrelated packages. Review the lockfile diff and restore unrelated version movement before completion.
- Removing byte input support is an intentional contract narrowing; an undocumented external Python caller could rely on it even though repository callers do not.
- Removing direct `rich` will not necessarily reduce installation size because Typer and Logfire currently depend on it transitively; the main benefit is manifest accuracy.

## Rollback / Recovery

Revert the dependency-cleanup commit to restore the previous manifest, lockfile, and bytes input support. This change has no data migration and does not alter Redis keys or values.

## Completion Checklist

- [x] `redis` is a direct dependency and `aiocache` is not declared, verified by `pyproject.toml`, package metadata, `uv tree --depth 1`, and a successful Redis 7.4.0 import.
- [x] `rich` is not a direct dependency, verified by `pyproject.toml` and `uv tree --depth 1`; `uv tree --invert --package rich` documents its expected transitive use by Typer and Logfire.
- [x] `src/hnbot/utils.py` has no `charset_normalizer` import or bytes branch, verified by the specified `rg` check returning no matches.
- [x] String HTML-to-Markdown behavior remains covered and passing, verified by 17 focused utility/app tests.
- [x] Dependency metadata and the lockfile are consistent, verified by `uv lock --check` and a zero-finding focused deptry audit.
- [x] Formatting, linting, typing, and all 59 tests pass, verified by `just all`.
