//! School health tools (toolset `school`).
//!
//! Phase 7: SCH-01 to SCH-08.

// Phase 7 modules go below this line.
pub mod bmi;
pub mod calendar;
pub mod content;
pub mod screening_requirements;
pub mod screenings;
pub mod templates;

pub use screening_requirements::{
    ScreeningRequirementsArgs, ScreeningRequirementsResult, get_ma_school_screening_requirements,
};
pub use screenings::{TrackScreeningsArgs, TrackScreeningsResult, track_school_screenings};
// End of Phase 7 modules.
