# Multi-stage build. The runtime image is a distroless-like base with just
# the binary — no shell, no package manager, no attack surface.

FROM rust:1.80-slim-bookworm AS builder

# protoc is required by tonic-build at compile time.
RUN apt-get update && apt-get install -y --no-install-recommends \
        protobuf-compiler \
        pkg-config \
        libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src

# Dependency layer — copy manifests first for cache efficiency.
COPY Cargo.toml Cargo.lock ./
COPY rust-toolchain.toml ./
COPY build.rs ./
COPY proto ./proto

# Empty src to warm the dep cache. Replaced below.
RUN mkdir -p src && echo "fn main() {}" > src/main.rs && \
    echo "" > src/lib.rs && \
    cargo build --release && \
    rm -rf src

COPY src ./src
COPY tests ./tests
COPY benches ./benches

RUN cargo build --release --bin order-book

# ------------------------------------------------------------------

FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

WORKDIR /app

COPY --from=builder /src/target/release/order-book /app/order-book

EXPOSE 50051 9100

USER nonroot:nonroot

ENTRYPOINT ["/app/order-book"]
