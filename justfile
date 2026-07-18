[default]
all: format lint test

format:
    cargo fmt --check

lint:
    cargo clippy --all-targets --all-features -- -D warnings

test:
    cargo test --all-targets

# Temporary parity/rollback gate until the controlled Rust cutover is accepted.
python-all:
    uv run ruff format --check
    uv run ruff check
    uv run ty check
    uv run pytest -v -s --cov=src/hnbot tests --ignore=tests/cli.rs --ignore=tests/contracts.rs --ignore=tests/service.rs

up:
    docker compose up -d --build --remove-orphans

down:
    docker compose down --remove-orphans
