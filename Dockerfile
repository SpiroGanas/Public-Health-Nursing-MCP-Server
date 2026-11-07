# syntax=docker/dockerfile:1.7
#
# Public Health Nursing MCP server.
#
# Multi-stage build: cargo-chef caches dependencies; the runtime image is
# distroless (glibc + CA certificates only), runs as a non-root user, and works
# with a read-only root filesystem. The nurse mounts their folder at /data;
# settings, the HTTP cache, and search indexes live in the /cache volume.

ARG RUST_VERSION=1.97

FROM rust:${RUST_VERSION}-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /src

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json --package phn-server
COPY . .
RUN cargo build --release --locked --package phn-server --bin phn-mcp-server \
    && mkdir -p /out/data /out/cache \
    && cp target/release/phn-mcp-server /out/phn-mcp-server

FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
ARG VERSION=dev
LABEL org.opencontainers.image.title="Public Health Nursing MCP Server" \
      org.opencontainers.image.description="MCP server that makes AI assistants more useful to Massachusetts public health nurses." \
      org.opencontainers.image.source="https://github.com/SpiroGanas/Public-Health-Nursing-MCP-Server" \
      org.opencontainers.image.licenses="MIT" \
      org.opencontainers.image.version="${VERSION}"
COPY --from=builder /out/phn-mcp-server /usr/local/bin/phn-mcp-server
COPY --from=builder --chown=65532:65532 /out/data /data
COPY --from=builder --chown=65532:65532 /out/cache /cache
ENV PHN_DATA_DIR=/data \
    PHN_CACHE_DIR=/cache \
    PHN_LOG=info
VOLUME ["/cache"]
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/phn-mcp-server"]
