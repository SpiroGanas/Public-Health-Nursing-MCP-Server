# ADR 0003: Own DOCX and tagged-PDF writers

- Status: accepted (Phase 1). Resolves the plan's open choice between `printpdf` and `typst`.

## Context

Exported documents must pass the server's own accessibility checks (Phase 1 exit criterion):
document title and language, real headings, table header rows, alt text, and, for PDF, a
structure tree. Nurses write in English, Spanish, Portuguese, Haitian Creole, Vietnamese, and
other languages.

## Decision

- **DOCX:** write WordprocessingML directly (about 500 lines) instead of using `docx-rs`, so every
  accessibility property is controlled: core-property title and language, built-in heading
  styles with outline levels, `tblHeader` rows and `tblCaption`, `docPr/@descr` alt text, real
  list numbering, letterhead header and page-numbered footer.
- **PDF:** a tagged-PDF writer on `lopdf`, embedding DejaVu Sans (regular and bold) subset with
  `subsetter` as Type0/CIDFontType2 fonts with ToUnicode maps. It emits a structure tree
  (H1–H6, P, L/LI/Lbl/LBody, Table/TR/TH/TD with column scope, Caption, Figure with `/Alt`), a
  parent tree, `/Lang`, `/MarkInfo`, and `DisplayDocTitle`; letterhead, rules, repeated header
  rows, and page numbers are artifacts. `printpdf` does not produce structure trees and `typst`
  would add a very large dependency for a small layout need.
- **XLSX** uses `rust_xlsxwriter` with Excel tables (announced with headers by screen readers);
  **SVG/PNG** are rendered with `resvg` using the same bundled fonts.

## Consequences

PDF layout is deliberately simple (single column, Letter size, no kerning or ligatures, no
italic face). Scripts outside DejaVu's coverage (Chinese, Khmer, Arabic) are not rendered in PDF
or images; the tool warns and recommends DOCX for those languages.
