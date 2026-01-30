# Multi-stage Dockerfile for 0rlhf
# Optimized for Railway deployment with PostgreSQL and Redis

# ====================
# Build stage - Rust
# ====================
FROM rust:1.88-slim-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy manifests first for better caching
COPY Cargo.toml Cargo.lock ./

# Create dummy src to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Build dependencies only (this layer will be cached)
RUN cargo build --release && rm -rf src

# Copy actual source code
COPY src ./src
COPY migrations ./migrations

# Touch main.rs to invalidate the dummy build
RUN touch src/main.rs

# Build the actual application
RUN cargo build --release

# ====================
# Frontend build stage
# ====================
FROM node:20-slim AS frontend

WORKDIR /app

# Copy package files
COPY package.json package-lock.json* ./

# Install dependencies (need devDependencies for esbuild)
RUN npm ci 2>/dev/null || npm install

# Copy TypeScript source
COPY static ./static
COPY tsconfig.json ./

# Build frontend
RUN npm run build

# ====================
# Runtime stage
# ====================
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user for security
RUN useradd -m -u 1000 -s /bin/bash orlhf

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/orlhf ./

# Copy migrations (run at startup)
COPY --from=builder /app/migrations ./migrations

# Copy static assets
COPY --from=frontend /app/static ./static

# Create upload directory
RUN mkdir -p uploads/src uploads/thumb && chown -R orlhf:orlhf /app

# Switch to non-root user
USER orlhf

# Railway provides PORT environment variable
ENV HOST=0.0.0.0
ENV PORT=8080
ENV RUST_LOG=orlhf=info,tower_http=info

# Expose the port
EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:${PORT}/health || exit 1

# Run the binary
CMD ["./orlhf"]
