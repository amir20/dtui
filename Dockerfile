# Build stage - use target platform for native compilation
FROM --platform=$TARGETPLATFORM rust:1.90-alpine AS builder

ARG TARGETPLATFORM

# Install musl-dev for static linking
RUN apk add --no-cache musl-dev

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Determine target architecture and build natively
RUN case "$TARGETPLATFORM" in \
    "linux/amd64") TARGET=x86_64-unknown-linux-musl ;; \
    "linux/arm64") TARGET=aarch64-unknown-linux-musl ;; \
    *) echo "Unsupported platform: $TARGETPLATFORM" && exit 1 ;; \
    esac && \
    rustup target add $TARGET && \
    cargo build --release --target $TARGET && \
    mv target/$TARGET/release/docker-monitor /docker-monitor

# Runtime stage - use scratch for minimal size
FROM scratch

# Copy CA certificates for HTTPS
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Copy the statically linked binary
COPY --from=builder /docker-monitor /docker-monitor

# Set terminal environment
ENV TERM=xterm-256color

ENTRYPOINT ["/docker-monitor"]
CMD []
