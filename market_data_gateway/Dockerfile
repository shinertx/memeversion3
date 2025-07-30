FROM ruCOPY Cargo.toml .
COPY Cargo.lock .
COPY market_data_gateway/Cargo.toml market_data_gateway/Cargo.toml
COPY shared-models/Cargo.toml shared-models/Cargo.toml
COPY executor/Cargo.toml executor/Cargo.toml
COPY position_manager/Cargo.toml position_manager/Cargo.toml
COPY signer/Cargo.toml signer/Cargo.toml
COPY portfolio_manager/Cargo.toml portfolio_manager/Cargo.toml

RUN mkdir -p market_data_gateway/src && echo "fn main(){}" > market_data_gateway/src/main.rs
RUN mkdir -p shared-models/src && echo "pub fn lib(){}" > shared-models/src/lib.rs
RUN mkdir -p executor/src && echo "fn main(){}" > executor/src/main.rs
RUN mkdir -p position_manager/src && echo "fn main(){}" > position_manager/src/main.rs
RUN mkdir -p signer/src && echo "fn main(){}" > signer/src/main.rs
RUN mkdir -p portfolio_manager/src && echo "fn main(){}" > portfolio_manager/src/main.rs

RUN cargo build --release --workspace

RUN rm market_data_gateway/src/*.rs shared-models/src/*.rs executor/src/*.rs position_manager/src/*.rs signer/src/*.rs portfolio_manager/src/*.rsm AS builder

RUN apt-get update && \
    apt-get install -y --no-install-recommends build-essential pkg-config libssl-dev

WORKDIR /app

COPY Cargo.toml .
COPY Cargo.lock .
COPY market_data_gateway/Cargo.toml market_data_gateway/Cargo.toml
COPY shared-models/Cargo.toml shared-models/Cargo.toml

RUN mkdir -p market_data_gateway/src && echo "fn main() {}" > market_data_gateway/src/main.rs
RUN mkdir -p shared-models/src && echo "pub fn lib(){}" > shared-models/src/lib.rs

RUN cargo build --release --workspace

RUN rm market_data_gateway/src/*.rs shared-models/src/*.rs

COPY market_data_gateway/src ./market_data_gateway/src
COPY shared-models/src ./shared-models/src

RUN cargo build --release --bin market_data_gateway

FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

RUN groupadd --system app && useradd --system --gid app app

WORKDIR /app

COPY --from=builder /app/target/release/market_data_gateway /usr/local/bin/market_data_gateway

USER app

EXPOSE 9185

CMD ["market_data_gateway"]
