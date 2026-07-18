## Goal

Make the repository Rust-only by removing the legacy Python implementation, tests, packaging, automation, and development-tool references while preserving Rust behavior and shared test fixtures.

## Context

The production Dockerfile and Compose service already run the Rust binary. The user explicitly requested Python removal; the separate production cutover acceptance in `docs/plans/2026-07-18_rust-rewrite-plan.md` remains an operational check and will not be marked complete without deployment evidence.

## Non-Goals

- Do not change runtime behavior, external API contracts, or the Redis schema.
- Do not delete language-neutral fixtures under `tests/contracts/` or `tests/data/`.
- Do not rewrite archived historical plans merely to erase accurate history.
- Do not deploy services or claim the controlled production cutover passed.

## Plan

- [x] Remove tracked Python source, tests, scripts, package metadata, lockfile, and Python-only release workflows; working-tree scan found no Python artifacts, and `git status --short` records every formerly tracked path as deleted.
- [x] Convert active automation and repository configuration to Rust-only operation, including CI, Dependabot, pre-commit, Just, ignore rules, and standalone Tombi configuration; focused active-tree search returned no tooling references, `tombi lint --offline` passed, and Tombi left the `Cargo.lock` hash unchanged (`656b8cf89203137978dd960dd23b78dd862b4e49`).
- [x] Update active documentation, repository instructions, migration tracking, stale memory, and Rust test names to describe the Rust-only repository without changing historical archived plans; focused active-tree search found no implementation/tooling references outside the two intentional removal/migration plans.
- [x] Run all removal gates; `just all` passed formatting, strict Clippy, and 52 tests; `prek run --all-files` passed all hooks; Compose config, Docker build, and container CLI smoke passed; active-reference/artifact audits, a credential-pattern scan, Tombi lint/format checks, and `git diff --check` passed.

## Risks

- Removing rollback source increases recovery time; the pre-removal Git history remains the recovery source.
- Shared fixtures may be deleted accidentally with Python tests; explicit file-presence and Rust contract-test checks mitigate this.
- Moving Tombi settings out of `pyproject.toml` could change Cargo lockfile formatting; run Tombi/pre-commit and confirm `Cargo.lock` remains unchanged.

## Rollback / Recovery

Restore the removed paths from the pre-removal Git commit. Runtime and Redis data require no migration or rollback because this change does not alter the Rust service or Compose volume.

## Completion Checklist

- [x] The repository is Rust-only; working-tree artifact scan returned no files, active-reference search returned no findings outside intentional historical plans, and tracked deletion/status review accounted for every removed source/tooling path.
- [x] Shared contracts and RSS fixtures remain present and pass their Rust tests; `tests/contracts/parity.json` and `tests/data/sample_rss.xml` exist, and `cargo test --all-targets` passed all 52 tests (including the contract integration test).
- [x] Rust quality gates and repository hooks pass; `just all`, `prek run --all-files`, and `git diff --check` exited successfully after the final source/configuration changes.
- [x] Container configuration remains valid; `docker compose config --quiet`, `docker build --tag hnbot:python-removal-test .`, and `docker run --rm hnbot:python-removal-test serve --help` all passed.
- [x] The final diff contains only Python-removal and directly required Rust-only configuration/documentation changes; `git status --short`, `git diff --name-status`, focused non-deletion diff review, and deleted-path absence checks all passed.
