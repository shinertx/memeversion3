FROM rust:1.82-slim AS builder

RUN apt-get update && \
    apt-get install -y --no-install-recommends build-essential pkg-config libssl-dev

WORKDIR /app

COPY ./shared-models ./shared-models
COPY ./signer ./signer

RUN cd signer && cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

RUN groupadd --system app && useradd --system --gid app app

WORKDIR /app

COPY --from=builder /app/signer/target/release/signer /usr/local/bin/signer

USER app

EXPOSE 8989

CMD ["signer"]
