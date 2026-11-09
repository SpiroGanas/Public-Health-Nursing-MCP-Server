//! Mental health and substance use tools (toolset `bh`).
//!
//! Phase 10: MH-01, MH-02. Phase 11: MH-03 to MH-07.

// Phase 10 modules go below this line.
pub mod crisis;
pub mod screening;
pub use crisis::{CrisisArgs, CrisisResult, find_crisis_resources};
pub use screening::{ScreenArgs, ScreenResult, score_mental_health_screen};
// End of Phase 10 modules.

// Phase 11 modules go below this line.
// End of Phase 11 modules.
