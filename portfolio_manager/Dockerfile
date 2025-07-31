FROM rust:1.82-slim AS builder

RUN apt-get update && \
    apt-get install -y --no-install-recommends build-essential pkg-config libssl-dev clang

WORKDIR /app

COPY Cargo.toml Cargo.toml
COPY Cargo.lock Cargo.lock
COPY portfolio_manager/Cargo.toml portfolio_manager/Cargo.toml
COPY shared-models/Cargo.toml shared-models/Cargo.toml

RUN mkdir -p portfolio_manager/src && echo "fn main() {}" > portfolio_manager/src/main.rs
RUN mkdir -p shared-models/src && echo "pub fn lib(){}" > shared-models/src/lib.rs

RUN cargo build --release --workspace

RUN rm portfolio_manager/src/*.rs shared-models/src/*.rs

COPY portfolio_manager/src ./portfolio_manager/src
COPY shared-models/src ./shared-models/src

RUN cargo build --release --bin portfolio_manager

FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*

RUN groupadd --system app && useradd --system --gid app app
USER app

WORKDIR /app

COPY --from=builder /app/target/release/portfolio_manager .

CMD ["./portfolio_manager"]
