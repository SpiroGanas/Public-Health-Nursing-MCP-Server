//! Document output.
//!
//! * [`model`]: a small document model, built from Markdown.
//! * [`docx`], [`pdf`], [`xlsx`], [`csv_out`], [`svg`]: writers. DOCX and PDF
//!   carry the agency letterhead and are built to pass the server's own
//!   accessibility checks (title, language, real headings, table header rows,
//!   alt text, tagged PDF).
//! * [`mailmerge`]: the local mail-merge pattern: identifiable letters are
//!   written to the output folder and never returned to the model.
//! * [`accessibility`]: Section 508 / WCAG 2.1 AA checks for DOCX, HTML, PDF.
//! * [`chart`]: accessible SVG charts (bar, stacked, line, dot, epidemic-curve
//!   histogram) with generated alt text and a data table.
//! * [`palette`]: validated, color-vision-safe colors for charts and maps;
//!   [`svg::svg_to_png`] rasterizes any SVG with the bundled fonts.

pub mod accessibility;
pub mod chart;
pub mod csv_out;
pub mod docx;
pub mod fonts;
pub mod image;
pub mod mailmerge;
pub mod model;
pub mod palette;
pub mod pdf;
pub mod svg;
pub mod xlsx;

pub use model::{Block, Document, Inline, TableBlock};
