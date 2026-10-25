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

# Stub sources to warm the dep cache. The [[bench]] declared in Cargo.toml
# must exist for the manifest to load, so it gets a stub too. Replaced below.
RUN mkdir -p src benches && echo "fn main() {}" > src/main.rs && \
    echo "" > src/lib.rs && \
    echo "fn main() {}" > benches/matching.rs && \
    cargo build --release && \
    rm -rf src benches

COPY src ./src
COPY benches ./benches

# COPY keeps the files' original mtimes, which can be older than the stub build
# above; touch them so cargo rebuilds the crate instead of reusing the stubs.
RUN touch src/main.rs src/lib.rs && cargo build --release --bin order-book

# ------------------------------------------------------------------

FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

WORKDIR /app

COPY --from=builder /src/target/release/order-book /app/order-book

EXPOSE 50051 9100

USER nonroot:nonroot

ENTRYPOINT ["/app/order-book"]
