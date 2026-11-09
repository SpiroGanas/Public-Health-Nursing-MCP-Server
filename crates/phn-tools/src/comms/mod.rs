//! Health communication tools (toolset `comms`).
//!
//! * COMM-01 `check_readability` (Phase 11)
//! * COMM-02 `translate_health_material` (Phase 11)
//! * COMM-03 `create_social_media_campaign` (Phase 11)
//! * COMM-04 `draft_press_materials` (Phase 11)
//! * COMM-05 `check_document_accessibility` (Phase 1)
//!
//! [`textstats`] and [`readability::analyze`] are shared with VAX-09 and the
//! other communication tools; [`content`] loads the Phase 11 content files
//! (also used by the `bh` substance-use and postvention tools).

pub mod accessibility;
pub use accessibility::{AccessibilityArgs, AccessibilityResult, check_document_accessibility};

// Phase 11 modules.
pub mod content;
pub mod readability;
pub mod safe_messaging;
pub mod textstats;

pub use readability::{ReadabilityArgs, ReadabilityResult, check_readability};
