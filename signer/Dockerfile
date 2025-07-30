# Stage 1: Builder
FROM rust:1.78 as builder

# Set up a new empty shell project
WORKDIR /app
RUN cargo init --bin signer
WORKDIR /app/signer

# Copy over all Cargo.toml and Cargo.lock files to cache dependencies
# This layer is cached, so dependencies are not re-downloaded on every build
COPY ../Cargo.toml .
COPY ../Cargo.lock .
COPY ../signer/Cargo.toml ./signer/
COPY ../shared-models/Cargo.toml ./shared-models/

# Create dummy source files to build dependencies
RUN mkdir -p src && echo "fn main(){}" > src/main.rs
RUN mkdir -p shared-models/src && echo "pub fn lib(){}" > shared-models/src/lib.rs

# Build only the dependencies to leverage Docker layer caching
RUN cargo build --release --bin signer
# Clean up dummy files and artifacts
RUN rm -rf src target/release/deps/signer*

# Now, copy the actual source code
COPY ../signer/src ./src
COPY ../shared-models/src ./shared-models/src

# Build the application with the actual source code
RUN cargo build --release --bin signer

# Stage 2: Final, minimal image
FROM debian:bullseye-slim

# Install required certificates and curl for the healthcheck
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && \
    rm -rf /var/lib/apt/lists/*

# Create a non-root user for security
RUN groupadd --system app && useradd --system --gid app app

# Copy the built binary from the builder stage
COPY --from=builder /app/signer/target/release/signer /usr/local/bin/signer

# Set the user
USER app

WORKDIR /app

# Expose the port the app runs on
EXPOSE 8989

# Run the binary
CMD ["signer"]
