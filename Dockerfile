# Build stage
FROM rust:1-slim-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests first for layer caching
COPY Cargo.toml Cargo.lock ./

# Create dummy src to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release && rm -rf src

# Copy actual source
COPY src ./src

# Touch main.rs to invalidate the dummy build
RUN touch src/main.rs

# Build release binary
RUN cargo build --release --locked

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies (CA certificates for HTTPS, curl for healthcheck)
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd --create-home --user-group app

# Create data and logs directories
RUN mkdir -p /app/data /app/logs && chown -R app:app /app

# Copy binary from builder
COPY --from=builder /app/target/release/vzdc-discord-bot /app/vzdc-discord-bot

USER app

# Health check endpoint
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3010/health || exit 1

EXPOSE 3010

CMD ["/app/vzdc-discord-bot"]
