# Build stage
FROM rust:1.87-alpine AS builder

# Install build dependencies
RUN apk add --no-cache \
    musl-dev \
    pkgconfig \
    openssl-dev \
    nodejs \
    npm

# Set working directory
WORKDIR /app

# Copy dependency files first for better layer caching
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Build the web assets
COPY web ./web
RUN cd web && \
    npm install tailwindcss @tailwindcss/cli && \
    npx @tailwindcss/cli -i ./style.css -o ./output.css --minify

# Build the application
RUN cargo build --release

# Runtime stage
FROM alpine:latest

# Install runtime dependencies
RUN apk add --no-cache \
    ca-certificates \
    && addgroup -g 1000 ypb \
    && adduser -D -s /bin/sh -u 1000 -G ypb ypb

# Create necessary directories
RUN mkdir -p /var/lib/ypb/files /var/lib/ypb/web \
    && chown -R ypb:ypb /var/lib/ypb

# Copy the binary from builder stage
COPY --from=builder /app/target/release/ypb /usr/local/bin/ypb

# Copy web assets
COPY --from=builder /app/web/index.html /var/lib/ypb/web/
COPY --from=builder /app/web/output.css /var/lib/ypb/web/

# Set proper permissions
RUN chmod +x /usr/local/bin/ypb

# Switch to non-root user
USER ypb

# Set working directory
WORKDIR /var/lib/ypb

# Expose port
EXPOSE 3000

# Set environment variables
ENV RUST_LOG=info

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:3000/ || exit 1

# Run the application
CMD ["ypb", "--port", "3000", "--file-path", "/var/lib/ypb/files", "--web-path", "/var/lib/ypb/web"]
