FROM rust:1.99-slim-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --locked

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 10001 appuser

WORKDIR /app
COPY --from=builder /app/target/release/avatar-generator /usr/local/bin/avatar-generator
COPY --from=builder /app/static /app/static

ENV PORT=8080 \
    DATA_DIR=/data \
    STATIC_DIR=/app/static \
    RUST_LOG=info

RUN mkdir -p /data && chown -R appuser:appuser /data /app
USER appuser

EXPOSE 8080
ENTRYPOINT ["avatar-generator"]
