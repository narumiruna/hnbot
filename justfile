[default]
all: format lint test

format:
    cargo fmt --check

lint:
    cargo clippy --all-targets --all-features -- -D warnings

test:
    cargo test --all-targets

up:
    docker compose up -d --build --remove-orphans

down:
    docker compose down --remove-orphans
