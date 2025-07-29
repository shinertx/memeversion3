FROM rust:1.82-slim AS builder

RUN apt-get update && \
    apt-get install -y --no-install-recommends build-essential pkg-config libssl-dev

WORKDIR /app

COPY ./shared-models ./shared-models
COPY ./market_data_gateway ./market_data_gateway

RUN cd market_data_gateway && cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

RUN groupadd --system app && useradd --system --gid app app

WORKDIR /app

COPY --from=builder /app/market_data_gateway/target/release/market_data_gateway /usr/local/bin/market_data_gateway

USER app

EXPOSE 9185

CMD ["market_data_gateway"]
