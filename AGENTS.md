# Repository Guidelines

## Project Structure & Module Organization
- Core package code lives in `src/hnbot/`.
- CLI entrypoint is `src/hnbot/cli.py` and is exposed as the `hnbot` console script.
- Bot/runtime logic is primarily in `app.py`, with feed/article/page helpers in sibling modules.
- Tests live in `tests/` (currently a minimal baseline; expand coverage as features grow).
- CI and release automation are in `.github/workflows/`.
- Environment templates are in `.env.example`; local secrets should stay in `.env` only.

## Build, Test, and Development Commands
- `uv sync` installs project and dev dependencies from `uv.lock`.
- `just all` runs the standard local gate: format, lint, type-check, and tests.
- `just format` runs `uv run ruff format`.
- `just lint` runs `uv run ruff check --fix`.
- `just type` runs `uv run ty check`.
- `just test` runs `uv run pytest -v -s --cov=src tests`.
- `uv run hnbot main` starts the CLI command locally.

## Coding Style & Naming Conventions
- Target Python 3.12+ and keep code under Ruff’s `line-length = 120`.
- Use 4-space indentation and explicit type annotations on production code.
- Follow module naming like existing files (`article.py`, `rss.py`, `utils.py`): short, lowercase, single-purpose.
- Keep imports sorted and single-line grouped per Ruff isort settings.
- Run pre-commit hooks before pushing (`ruff`, `ruff-format`, `ty-check`, `uv-lock`, TOML formatting).

## Testing Guidelines
- Framework: `pytest` with `pytest-cov`.
- Place tests under `tests/` and name files `test_*.py`; prefer function names like `test_<behavior>()`.
- For new features, add or update tests in the same PR.
- Ensure coverage is collected for `src/` and no failing tests remain before opening a PR.

## Commit & Pull Request Guidelines
- Follow the repo’s history style: imperative, concise subject lines (e.g., `Refactor get_hn_feed to improve entry parsing`).
- Keep commits focused; avoid mixing refactors and behavior changes when possible.
- PRs should include: purpose, key changes, test evidence (command output), and linked issue(s) when applicable.
- Ensure GitHub Actions `python.yml` checks pass (`ruff`, `ty`, `pytest --cov`) before requesting review.

## Security & Configuration Tips
- Do not commit secrets or real tokens; use `.env.example` placeholders.
- If dependencies change, update lockfile (`uv lock`) so CI and local runs stay reproducible.

## Gotcha

- `GOTCHA.md` MUST NOT be assumed to be auto-loaded.
- The agent MUST first look for `GOTCHA.md` (case-sensitive) in the project root.
- If a GOTCHA file exists, the agent MUST read relevant entries and explicitly apply them in diagnosis and proposed fixes.
- If the agent makes a mistake during the task, the agent MUST create `GOTCHA.md` first when it does not exist, then add or update a `GOTCHA.md` entry in the same session; the entry MUST describe only **non-obvious, experience-derived pitfalls** that required debugging to understand.

## Taste

- `TASTE.md` MUST NOT be assumed to be auto-loaded.
- The agent MUST first look for `TASTE.md` (case-sensitive) in the project root.
- If a TASTE file exists, the agent MUST read relevant entries and explicitly apply them in recommendations and implementations.
- If the user requests a preference that the agent did not anticipate, the agent MUST update `TASTE.md` in the same turn.
- If an update to `TASTE.md` is required and the file does not exist, the agent MUST create `TASTE.md` first in the project root.
- Each `TASTE.md` entry MUST capture exactly one reusable preference signal.
