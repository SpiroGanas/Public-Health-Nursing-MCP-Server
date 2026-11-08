//! Health equity and community assessment tools (toolset `equity`).

// Phase 4 modules (EQ-01 to EQ-04) go below this line.
pub(crate) mod acs;
pub mod disparities;
pub mod language;
pub mod profile;
pub mod svi;

pub use disparities::{DisparityArgs, DisparityResult, measure_health_disparities};
pub use language::{LanguageAccessArgs, LanguageAccessResult, language_access_profile};
pub use profile::{ProfileArgs, ProfileResult, build_community_health_profile};
pub use svi::{SviArgs, SviResult, get_social_vulnerability_index};
