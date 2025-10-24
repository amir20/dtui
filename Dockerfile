# Build stage - cross-compile from AMD64
FROM --platform=$BUILDPLATFORM rust:1.90-alpine AS builder

ARG TARGETPLATFORM
ARG BUILDPLATFORM

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Install dependencies and setup cross-compilation
RUN case "$TARGETPLATFORM" in \
    "linux/amd64") \
    apk add --no-cache musl-dev && \
    TARGET=x86_64-unknown-linux-musl && \
    RUSTFLAGS="" \
    ;; \
    "linux/arm64") \
    apk add --no-cache musl-dev gcc-aarch64-linux-gnu && \
    TARGET=aarch64-unknown-linux-musl && \
    ln -s /usr/bin/aarch64-linux-gnu-gcc /usr/bin/aarch64-linux-musl-gcc && \
    RUSTFLAGS="-C linker=aarch64-linux-musl-gcc" \
    ;; \
    *) echo "Unsupported platform: $TARGETPLATFORM" && exit 1 ;; \
    esac && \
    rustup target add $TARGET && \
    RUSTFLAGS="$RUSTFLAGS" cargo build --release --target $TARGET && \
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
