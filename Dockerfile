# Multi-stage Dockerfile for docker-monitor
# Supports both amd64 (x86_64) and arm64 (aarch64) architectures

# Build stage
FROM --platform=$BUILDPLATFORM rust:1.90-slim AS builder

# Install required dependencies for building
ARG TARGETPLATFORM
ARG BUILDPLATFORM
RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev musl-tools && \
    rm -rf /var/lib/apt/lists/*

# Install cross-compilation tools if needed
RUN if [ "$BUILDPLATFORM" != "$TARGETPLATFORM" ]; then \
    if [ "$TARGETPLATFORM" = "linux/amd64" ]; then \
    apt-get update && \
    apt-get install -y gcc-x86-64-linux-gnu && \
    rm -rf /var/lib/apt/lists/*; \
    elif [ "$TARGETPLATFORM" = "linux/arm64" ]; then \
    apt-get update && \
    apt-get install -y gcc-aarch64-linux-gnu && \
    rm -rf /var/lib/apt/lists/*; \
    fi; \
    fi

# Add Rust target for the platform
RUN case "$TARGETPLATFORM" in \
    "linux/amd64") \
    rustup target add x86_64-unknown-linux-musl \
    ;; \
    "linux/arm64") \
    rustup target add aarch64-unknown-linux-musl \
    ;; \
    esac

WORKDIR /usr/src/docker-monitor

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Build the application with musl for static linking
ARG TARGETPLATFORM
ARG BUILDPLATFORM
RUN case "$TARGETPLATFORM" in \
    "linux/amd64") \
    if [ "$BUILDPLATFORM" != "$TARGETPLATFORM" ]; then \
    export CC=x86_64-linux-gnu-gcc; \
    export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=x86_64-linux-gnu-gcc; \
    fi; \
    cargo build --release --target x86_64-unknown-linux-musl && \
    cp target/x86_64-unknown-linux-musl/release/docker-monitor /usr/local/bin/docker-monitor \
    ;; \
    "linux/arm64") \
    if [ "$BUILDPLATFORM" != "$TARGETPLATFORM" ]; then \
    export CC=aarch64-linux-gnu-gcc; \
    export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=aarch64-linux-gnu-gcc; \
    fi; \
    cargo build --release --target aarch64-unknown-linux-musl && \
    cp target/aarch64-unknown-linux-musl/release/docker-monitor /usr/local/bin/docker-monitor \
    ;; \
    esac

# Strip the binary to reduce size further (use arch-specific strip for cross-compilation)
ARG TARGETPLATFORM
ARG BUILDPLATFORM
RUN if [ "$BUILDPLATFORM" != "$TARGETPLATFORM" ]; then \
    if [ "$TARGETPLATFORM" = "linux/amd64" ]; then \
    x86_64-linux-gnu-strip /usr/local/bin/docker-monitor; \
    elif [ "$TARGETPLATFORM" = "linux/arm64" ]; then \
    aarch64-linux-gnu-strip /usr/local/bin/docker-monitor; \
    fi; \
    else \
    strip /usr/local/bin/docker-monitor; \
    fi

# Runtime stage - using scratch for minimal image size
FROM scratch

# Copy the statically-linked binary from builder
COPY --from=builder /usr/local/bin/docker-monitor /docker-monitor

# Copy CA certificates for HTTPS connections (if needed)
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Set the binary as entrypoint
ENTRYPOINT ["/docker-monitor"]

# Default to local Docker socket
CMD ["--host", "local"]
