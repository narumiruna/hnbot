## Goal

Eliminate the observed Compose runtime failures: HN comment HTTP 429s, OpenAI generation timeouts, and unauthenticated Redis startup.

## Context

Live checks confirmed that HNRSS and the configured OpenAI-compatible endpoint are reachable. HN page scraping intermittently returns 429, while the shared 10-second timeout is too short for full article generation. Redis is isolated from host ports but still runs without authentication.

## Architecture

Use the one-request Algolia HN item API for discussion content instead of scraping `news.ycombinator.com`, give OpenAI a dedicated longer timeout while retaining short timeouts for other adapters, and pass an environment-provided Redis password through typed settings to Redis and its health check.

## Plan

- [x] Add failing Rust regression tests for Algolia discussion parsing/request routing, dedicated OpenAI timeout configuration, detailed timeout errors, and Redis password propagation; `cargo test --locked --no-run` failed on the new missing symbols/fields before implementation, and all narrow regressions now pass.
- [x] Implement the comment API adapter and OpenAI timeout split, update runtime wiring and documentation, then verify the narrow tests pass; focused config, adapter, timeout, store, and mocked-service tests all pass.
- [ ] Add authenticated Redis Compose wiring and current local secret configuration without exposing credentials; verify rendered Compose config, authenticated health, and unauthenticated command rejection.
- [ ] Run the repository Rust gates and rebuild the Compose services; verify the service processes live entries without the previously observed 429, 10-second generation failure, or Redis warning.

## Risks

- Algolia is another external dependency; keep its base URL configurable for deterministic tests and operational overrides.
- A longer OpenAI timeout can leave requests in flight longer; only the OpenAI client receives it, and service cancellation still drops the in-flight future.
- Enabling Redis authentication requires coordinated app and health-check configuration; recreate both services together while preserving `redis-data`.

## Rollback / Recovery

Revert the code/config changes and run `docker compose up -d --build --force-recreate` without `docker compose down -v`; the persistent Redis volume and key/value schema remain unchanged.

## Completion Checklist

- [ ] HN discussions no longer depend on `news.ycombinator.com` page scraping, verified by Wiremock request-path assertions and live Compose logs.
- [ ] OpenAI requests use the dedicated timeout and expose timeout causes, verified by config/unit tests and live processing beyond the old 10-second boundary.
- [ ] Redis requires authentication and remains healthy with existing data, verified by `docker compose ps`, authenticated `DBSIZE`, and rejected unauthenticated `PING`.
- [ ] Formatting, Clippy, all-target tests, and Compose log review complete without known required work remaining.
