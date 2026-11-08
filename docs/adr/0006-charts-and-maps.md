# ADR 0006: Charts and maps are drawn by the server as accessible SVG

- Status: accepted
- Date: 2026-11-08
- Phase: 2 (charts), 4 (maps)

## Context

The project plan listed `plotters` for charts. Charts must work in a distroless container without
system fonts, embed in DOCX/PDF through `export_document`, and meet the same accessibility bar as
documents (WCAG 1.1.1 text alternatives, 1.4.1 use of color, 1.4.11 non-text contrast).
`plotters` needs a font backend for PNG and does not produce an accessible name, description, or
data table.

## Decision

`phn_docs::chart` renders bar, stacked bar, line, dot (with intervals and log scale), and
histogram (epidemic curve) charts, plus control-chart features (stepped limits, flagged points,
markers, shaded spans), directly as SVG with `<title>` and a generated `<desc>`. PNG comes from
the existing `resvg` path with the bundled DejaVu fonts (`phn_docs::svg::svg_to_png`). Maps
(Phase 4) use the same palette and rasterizer.

Colors come from `phn_docs::palette`: a categorical order validated for color-vision deficiency on
a white background (adjacent ΔE ≥ 9 under simulation), a one-hue sequential ramp for maps, and a
diverging ramp. Because three categorical colors are below 3:1 contrast on white, every chart also
has direct labels or marker shapes, a legend for two or more series, hatched suppressed values,
and a data table returned with the image. Bars always start at zero.

## Consequences

- No new rendering dependency; output is identical on every machine.
- Chart types are limited to what public health reports need; new forms are added to one module.
- Every chart tool returns alt text and the data, so the model can describe the image and the
  nurse can include a table in reports.
