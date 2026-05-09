## GOTCHA

- `tests/data/sample_rss.xml` is a real HN comments feed snapshot, so RSS parsing tests must not assume synthetic entries like `id=100` or fixed titles such as `First Post`.
- Pre-commit `ty-check` can catch test typing issues that `just all`/`uv run ty check` misses; before committing, make fakes structurally type-compatible (for example via a Protocol) or run the hook.

## TASTE

- Prefer `uv`-managed commands (for example `uv run python ...`, `uv run pytest`, `uv sync`) instead of invoking bare `python` directly in this repository.
