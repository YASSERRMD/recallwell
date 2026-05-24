# syntax=docker/dockerfile:1.7
#
# Multi-stage build for recallwell.
#
#   docker build -t recallwell .
#   docker run --rm -p 7676:7676 \
#     -e RECALLWELL_GROQ_API_KEY=gsk_... \
#     -v $(pwd)/recallwell-data:/data \
#     recallwell

# ----------------------------------------------------------------------------
# Stage 1: build
# ----------------------------------------------------------------------------
FROM rust:1.89-bookworm AS builder

WORKDIR /usr/src/recallwell

# Cache deps separately from sources.
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
RUN mkdir -p src tests \
 && echo "fn main() {}" > src/main.rs \
 && echo "" > src/lib.rs

# Pre-build dependencies (this layer is cached when only sources change).
RUN cargo build --release --bin recallwell || true

# Now copy the real sources and build.
COPY src ./src
COPY tests ./tests
# Touch top-level files so cargo notices changes.
RUN touch src/main.rs src/lib.rs \
 && cargo build --release --bin recallwell

# ----------------------------------------------------------------------------
# Stage 2: runtime
# ----------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# CA bundle for HTTPS calls to Groq, tini for clean PID 1 signal handling.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates tini \
 && rm -rf /var/lib/apt/lists/*

# Non-root user.
RUN groupadd --system --gid 1001 recallwell \
 && useradd --system --uid 1001 --gid recallwell --home /data --shell /usr/sbin/nologin recallwell

COPY --from=builder /usr/src/recallwell/target/release/recallwell /usr/local/bin/recallwell
RUN chmod 0755 /usr/local/bin/recallwell

# Data dir is the only writable surface.
RUN mkdir -p /data && chown -R recallwell:recallwell /data
VOLUME ["/data"]

USER recallwell
WORKDIR /data

# In-container defaults; override with -e on docker run / compose env.
ENV RECALLWELL_HOST=0.0.0.0 \
    RECALLWELL_PORT=7676 \
    RECALLWELL_DATA_DIR=/data \
    RECALLWELL_AUTO_OPEN=false \
    RUST_LOG=info

EXPOSE 7676

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/recallwell"]
CMD ["serve"]
