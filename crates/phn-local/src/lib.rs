//! Local data access with privacy guardrails.
//!
//! * [`sandbox`]: path resolution confined to the mounted `/data` folder.
//! * [`table`], [`docx`], [`pdf`], [`html`], [`extract`]: parsers.
//! * [`phi`]: detection of possible identifiers in columns and free text.
//! * [`profile`]: dataset profiling that never shows identifier values.
//! * [`deid`]: HIPAA Safe Harbor and limited-data-set de-identification.
//! * [`policy_index`]: full-text index of agency policies.

pub mod deid;
pub mod docx;
pub mod extract;
pub mod html;
pub mod pdf;
pub mod phi;
pub mod policy_index;
pub mod profile;
pub mod sandbox;
pub mod table;

pub use sandbox::Sandbox;
