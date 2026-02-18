# --- Stage 1: Build ---
FROM rust:1.85-slim-bookworm AS builder

WORKDIR /usr/src/app

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create a dummy main.rs to cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -f target/release/deps/mcp_searxng_rs*

# Copy the real source code
COPY src ./src

# Build for release
RUN cargo build --release

# --- Stage 2: Runtime ---
FROM debian:bookworm-slim

# Install SSL certificates (required for https calls to SearXNG)
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from the builder stage
COPY --from=builder /usr/src/app/target/release/mcp-searxng-rs /app/mcp-bridge

# Default environment variables
ENV MCP_SX_PORT=3000
ENV MCP_SX_URL=http://localhost:8080
ENV MCP_SX_LOG=info

# Expose the SSE/HTTP port
EXPOSE 3000

# Run the bridge
CMD ["./mcp-bridge"]
