FROM rust:1.82-slim AS builder

RUN apt-get update && \
    apt-get install -y --no-install-recommends build-essential pkg-config libssl-dev

WORKDIR /app

COPY ./shared-models ./shared-models
COPY ./position_manager ./position_manager

RUN cd position_manager && cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

RUN groupadd --system app && useradd --system --gid app app

WORKDIR /app

COPY --from=builder /app/position_manager/target/release/position_manager /usr/local/bin/position_manager

USER app

CMD ["position_manager"]
