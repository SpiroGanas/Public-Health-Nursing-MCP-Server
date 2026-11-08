//! Evidence-based practice and quality improvement tools (toolset `ebp`).

// Phase 2 modules (EBP-06) go below this line.

// Phase 4 modules (EBP-01, EBP-02) go below this line.
pub mod cdc_guidance;
pub mod pubmed;

pub use cdc_guidance::{CdcGuidanceArgs, CdcGuidanceResult, search_cdc_guidance};
pub use pubmed::{PubmedResult, SearchPubmedArgs, search_pubmed};
