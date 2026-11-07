# ADR 0002: One response envelope for every tool

- Status: accepted (Phase 0)

## Decision

Every tool returns `Envelope<T>` (`phn_core::envelope`): `summary` (markdown), `data` (typed),
`sources[]`, `caveats[]`, `privacy`, and an optional decision-support `notice`. The MCP result
carries:

1. a text block with the markdown summary plus caveats, sources, and the privacy line, so
   clients that only show text to the model still convey everything important;
2. a text block with the full envelope as JSON (the MCP spec's recommendation for backward
   compatibility with clients that ignore structured content);
3. `structuredContent` with the same JSON, validated against the tool's `outputSchema`.

Problems the nurse can fix are tool results with `isError: true`, not protocol errors, so the
model sees the message.

## Consequences

Responses are larger than a single block, which matters little next to the benefit of uniform
citations and privacy facts. Integration tests validate every tool's structured content against
its published output schema.
