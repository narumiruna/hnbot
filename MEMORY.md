## GOTCHA

- `tests/data/sample_rss.xml` is a real HN comments feed snapshot, so RSS parsing tests must not assume synthetic entries like `id=100` or fixed titles such as `First Post`.
- Pre-commit `ty-check` can catch test typing issues that `just all`/`uv run ty check` misses; before committing, make fakes structurally type-compatible (for example via a Protocol) or run the hook.
- Symptom: Docker cannot resolve `COPY --chown=app:app` in the final Python slim stage. Cause: the base image has no `app` user. Fix: create the group and user before `COPY`, then select it with `USER app`.
- Symptom: HN comment fetches repeatedly return HTTP 429 even with `comments_fetch_concurrency=1`. Cause: the semaphore limits concurrency but does not space sequential requests, and failed entries are retried on later batches. Fix: add per-host request pacing/global cooldown rather than only lowering concurrency.

## TASTE

- Prefer `uv`-managed commands (for example `uv run python ...`, `uv run pytest`, `uv sync`) instead of invoking bare `python` directly in this repository.
- Prefer a service-only CLI (`hnbot serve`); avoid maintaining bare or `hnbot main` one-shot modes.
