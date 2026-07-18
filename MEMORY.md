## GOTCHA

- `tests/data/sample_rss.xml` is a real HN comments feed snapshot, so RSS parsing tests must not assume synthetic entries like `id=100` or fixed titles such as `First Post`.
- Pre-commit `ty-check` can catch test typing issues that `just all`/`uv run ty check` misses; before committing, make fakes structurally type-compatible (for example via a Protocol) or run the hook.
- Symptom: Docker cannot resolve `COPY --chown=app:app` in the final Python slim stage. Cause: the base image has no `app` user. Fix: create the group and user before `COPY`, then select it with `USER app`.
- Symptom: HN comment fetches repeatedly return HTTP 429 even with serialized, paced requests. Cause: `news.ycombinator.com` page scraping is intermittently rate-limited, and failed entries are retried on later batches. Fix: fetch each nested discussion through the one-request Algolia item API while retaining pacing/cooldown for API throttling.
- Symptom: full OpenAI article requests fail after about 10 seconds even though the endpoint is reachable. Cause: generation shared the short general HTTP timeout. Fix: use `OPENAI_TIMEOUT_SECONDS` for a dedicated OpenAI client and preserve the transport error source chain in logs.
- Symptom: `docker` is absent in this WSL shell even though Docker Desktop is installed. Cause: WSL integration does not expose the Linux CLI and the Desktop daemon may be stopped. Fix: launch `/mnt/c/Program Files/Docker/Docker/Docker Desktop.exe` and invoke `/mnt/c/Program Files/Docker/Docker/resources/bin/docker.exe`; enable WSL integration for a normal `docker` command.
- Symptom: pre-commit alternates between `cargo-clippy` and `tombi-format` modifying `Cargo.lock`. Cause: Cargo rewrites Tombi's formatting for its generated lockfile. Fix: exclude both `Cargo.lock` and `uv.lock` under `[tool.tombi.files]`.
- Symptom: real OpenAI Responses requests reject a Schemars-generated schema despite `strict: true`. Cause: strict structured outputs require `additionalProperties: false` on every root and nested object. Fix: recursively close generated object schemas and assert both `Article` and `Section` in the HTTP contract test.
- Symptom: the container exhibits behavior already fixed in the Rust source, such as the old short OpenAI timeout. Cause: `docker compose up -d` reuses the existing hnbot image. Fix: run `docker compose up -d --build` after source changes.

## TASTE

- Prefer `uv`-managed commands (for example `uv run python ...`, `uv run pytest`, `uv sync`) instead of invoking bare `python` directly in this repository.
- Prefer a service-only CLI (`hnbot serve`); avoid maintaining bare or `hnbot main` one-shot modes.
