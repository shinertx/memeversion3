FROM rustlang/rust:nightly-slim AS builder

RUN apt-get update && \
    apt-get install -y --no-install-recommends build-essential pkg-config libssl-dev clang

WORKDIR /app

# Copy workspace files
COPY Cargo.toml ./

# Copy all crate manifests  
COPY executor/Cargo.toml ./executor/
COPY shared-models/Cargo.toml ./shared-models/
COPY signer/Cargo.toml ./signer/
COPY portfolio_manager/Cargo.toml ./portfolio_manager/
COPY market_data_gateway/Cargo.toml ./market_data_gateway/
COPY position_manager/Cargo.toml ./position_manager/

# Create stub files for dependency build
RUN mkdir -p shared-models/src && echo "fn main() {}" > shared-models/src/lib.rs
RUN mkdir -p signer/src && echo "fn main() {}" > signer/src/main.rs  
RUN mkdir -p portfolio_manager/src && echo "fn main() {}" > portfolio_manager/src/main.rs
RUN mkdir -p market_data_gateway/src && echo "fn main() {}" > market_data_gateway/src/main.rs
RUN mkdir -p position_manager/src && echo "fn main() {}" > position_manager/src/main.rs

# Create stub for executor
RUN mkdir -p executor/src && echo "fn main() {}" > executor/src/main.rs

# Build dependencies
RUN cargo build --release --workspace

# Remove stub files and copy real source
RUN rm -rf executor/src shared-models/src
COPY executor/src ./executor/src  
COPY shared-models/src ./shared-models/src

# Build final application
RUN cargo build --release --bin executor

# Runtime stage
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
RUN groupadd --system app && useradd --system --gid app app
WORKDIR /app
COPY --from=builder /app/target/release/executor ./executor
RUN chown -R app:app /app
USER app
CMD ["./executor"]
