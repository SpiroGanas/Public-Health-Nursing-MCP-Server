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
# /cache is world-writable with the sticky bit (like /tmp). A named volume
# copies this mode the first time it is mounted, so the server can save its
# settings and cache when it runs as the default user (65532) and also when a
# Linux user runs it with `--user "$(id -u):$(id -g)"` to own the output files.
# The folders are made here, under /out/root, and copied as entries of their
# parent: COPY of a folder itself copies only its contents and creates the
# destination with mode 755, which would lose the 1777.
RUN cargo build --release --locked --package phn-server --bin phn-mcp-server \
    && mkdir -p /out/root/data /out/root/cache \
    && chown 65532:65532 /out/root/data /out/root/cache \
    && chmod 1777 /out/root/cache \
    && cp target/release/phn-mcp-server /out/phn-mcp-server

FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
ARG VERSION=dev
LABEL org.opencontainers.image.title="Public Health Nursing MCP Server" \
      org.opencontainers.image.description="MCP server that makes AI assistants more useful to Massachusetts public health nurses." \
      org.opencontainers.image.source="https://github.com/SpiroGanas/Public-Health-Nursing-MCP-Server" \
      org.opencontainers.image.licenses="MIT" \
      org.opencontainers.image.version="${VERSION}"
COPY --from=builder /out/phn-mcp-server /usr/local/bin/phn-mcp-server
COPY --from=builder /out/root/ /
# TZ: "today" and deadline calculations use the local clock (chrono reads TZ
# and the zone file in /usr/share/zoneinfo, which the distroless base image
# ships with its tzdata). Massachusetts is Eastern time; without this the
# container would run on UTC and dates would roll over at 7 or 8 p.m. local.
# `phn-mcp-server check` prints the local time and offset to confirm it.
ENV PHN_DATA_DIR=/data \
    PHN_CACHE_DIR=/cache \
    PHN_LOG=info \
    TZ=America/New_York
VOLUME ["/cache"]
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/phn-mcp-server"]
