//! Maternal, child, and family health tools (toolset `mch`).
//!
//! Phase 8: MCH-01 to MCH-07.

// Phase 8 modules go below this line.
pub(crate) mod content;
pub mod growth;
pub mod lead;
pub(crate) mod lead_data;

pub use growth::{GrowthArgs, GrowthResult, calculate_growth_percentiles};
pub use lead::{LeadGuideArgs, LeadGuideResult, childhood_lead_screening_guide};
// End of Phase 8 modules.
