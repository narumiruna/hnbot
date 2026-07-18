ARG RUST_VERSION=1.88
ARG DEBIAN_VERSION=bookworm
FROM rust:${RUST_VERSION}-${DEBIAN_VERSION} AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:${DEBIAN_VERSION}-slim

WORKDIR /app

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system app \
    && useradd --system --gid app --no-create-home app

COPY --from=builder --chown=app:app /app/target/release/hnbot /usr/local/bin/hnbot

USER app

ENTRYPOINT ["hnbot"]
