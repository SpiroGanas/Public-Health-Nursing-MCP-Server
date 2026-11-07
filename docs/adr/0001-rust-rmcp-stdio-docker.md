# ADR 0001: Rust, rmcp over stdio, distributed as a container

- Status: accepted (Phase 0)

## Context

The project plan calls for a server written in Rust and distributed to nurses as a Docker
container launched by an MCP client over stdio. Nurses run it on agency laptops, often behind
proxies that inspect TLS, sometimes offline.

## Decision

- Use the official Rust SDK, `rmcp` 3.9, with the `#[tool]` macros for schemas and annotations.
  `list_tools` and `call_tool` are implemented by hand so tools can be filtered by toolset at
  runtime and a `tools/list_changed` notification sent when the nurse changes toolsets.
- Logs go to stderr only; stdout carries the protocol.
- A Cargo workspace mirrors the plan's crate layout so later phases add tools without moving
  code.
- `reqwest` with rustls and the platform verifier, plus an optional PEM bundle
  (`PHN_CA_BUNDLE`); proxies come from the standard environment variables.
- The image is `gcr.io/distroless/cc-debian12:nonroot`, built with cargo-chef, running as UID
  65532 with a read-only root filesystem; only `/data` and `/cache` are writable.
- Reference content and snapshots are embedded in the binary at build time, so the image works
  offline and ships exactly what was tested.

## Consequences

The binary is large (fonts, name lists, snapshots, and a search engine are embedded) but needs no
runtime files. Multi-architecture images are built with QEMU on release only.
