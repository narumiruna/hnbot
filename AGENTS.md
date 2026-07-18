# Repository Guidelines

## Project Structure & Module Organization

- The production Rust binary is defined by root `Cargo.toml`; Rust modules live in `src/*.rs`.
- `src/main.rs` wires configuration, adapters, tracing, and the `hnbot` CLI.
- Rust integration tests live in `tests/*.rs`; shared fixtures are under `tests/contracts/` and `tests/data/`.
- CI automation lives in `.github/workflows/`.
- Configuration placeholders belong in `.env.example`; secrets stay in `.env` only.

## Build, Test, and Development Commands

- `cargo build --locked` builds the Rust service.
- `cargo run --release -- serve` starts the service locally.
- `cargo fmt --check` verifies formatting.
- `cargo clippy --all-targets --all-features -- -D warnings` runs the strict lint gate.
- `cargo test --all-targets` runs unit, contract, CLI, and mocked integration tests.
- `just all` runs the standard aggregate Rust gate.
- `docker compose up -d --build --remove-orphans` builds and starts hnbot and Redis.

## Coding Style & Design

- Target Rust edition 2024 with MSRV 1.88 and keep `Cargo.lock` committed.
- Keep modules single-purpose and use explicit error types at adapter boundaries.
- Keep external I/O behind traits so tests remain deterministic and offline.
- Preserve the existing Redis key/value contract and write only after Telegram succeeds.
- Source files exceeding 1,000 lines must be decomposed or carry a documented justification.

## Testing Guidelines

- Add unit tests next to modules and end-to-end/mock tests under `tests/`.
- Use Wiremock or fakes for HNRSS, OpenAI, Telegraph, Telegram, and Redis behavior; CI must not need network services or secrets.
- Update shared contract fixtures when intentionally changing externally visible behavior.
- Cover success, retry exhaustion, validation boundaries, dedupe ordering, cancellation, and partial failures.

## Commit & Pull Request Guidelines

- Use imperative, focused commit subjects consistent with repository history.
- Do not mix unrelated refactors with behavior changes.
- PRs should include purpose, key changes, test evidence, and linked issues when applicable.
- Run Cargo gates and applicable pre-commit hooks before pushing.

## Security & Operations

- Never commit API keys, bot tokens, chat IDs, or Redis credentials.
- Do not log credentials; settings debug output must remain redacted.
- Preserve the existing Redis volume during cutover and rollback.
- Use the non-root Docker runtime and keep CA certificates available for HTTPS.

## MEMORY.md

- `MEMORY.md` is not auto-loaded. Check it before non-trivial debugging or design work.
- Keep entries concise under `## GOTCHA` and `## TASTE`.
