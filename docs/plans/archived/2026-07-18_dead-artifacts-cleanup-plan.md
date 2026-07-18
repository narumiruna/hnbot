## Goal

Remove confirmed unreferenced snapshots, one-off diagnostics, and placeholder tests without changing runtime behavior or deleting active test fixtures.

## Context

Repository searches show no references to `hnrss.json`. `scripts/validate_rss_datetime.py` is a historical manual diagnostic, and `tests/test_hello.py` only prints text without asserting application behavior. The active RSS parser tests use `tests/data/sample_rss.xml`, which must remain.

## Non-Goals

- Do not remove `scripts/example.py`; its manual end-to-end role requires a separate usage decision.
- Do not remove `HNEntry.published_at` or `parse_datetime()` in this cleanup; that belongs to the later RSS model simplification.
- Do not remove `tests/data/sample_rss.xml`.
- Do not edit archived plans.

## Assumptions

- No external workflow consumes the unreferenced root `hnrss.json` file or invokes `scripts/validate_rss_datetime.py` outside the repository.
- Diagnostic history remains available through Git history after deletion.

## Plan

- [x] Reconfirm that the removal targets are not referenced by tracked code, tests, documentation, workflows, or configuration using the specified `git grep`; the only match was the `test_hello` function's self-reference.
- [x] Delete `hnrss.json`, `scripts/validate_rss_datetime.py`, and `tests/test_hello.py`; the focused diff summary verified exactly those three deletions.
- [x] Confirm the active RSS fixture remains tracked and used; `tests/test_rss.py` references it, `git ls-files` and `test -f` confirmed it is tracked/present, and its focused status was clean.
- [x] Run focused RSS tests with `uv run pytest tests/test_rss.py -v`, then run the complete repository gate with `just all`; all 9 RSS tests and all 58 repository tests passed, along with Ruff format/check and ty.
- [x] Review the final change with `git diff --check` and `git status --short`; the focused diff contains exactly the three intended deletions, while production source, `tests/data/sample_rss.xml`, and `scripts/example.py` remain unmodified.

## Risks

- The root JSON snapshot could be used by an undocumented external process. Git history provides recovery, but external usage cannot be proven from repository evidence alone.
- The similarly named `tests/data/sample_rss.xml` is an active fixture and must not be confused with the unreferenced root snapshot.

## Rollback / Recovery

Restore any deleted artifact from the cleanup commit with `git restore --source=<commit>^ -- <path>`. No runtime data or Redis state is affected.

## Completion Checklist

- [x] The three confirmed artifacts are absent, verified by the specified `test ! -e` assertions.
- [x] `tests/data/sample_rss.xml` remains present and exercised, verified by its tracked reference and 9 passing RSS tests.
- [x] No unrelated files are included in the cleanup diff, verified by `git diff --name-status` showing exactly the three intended deletions.
- [x] Formatting, linting, typing, and all 58 tests pass, verified by `just all`.
