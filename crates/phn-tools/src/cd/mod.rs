//! Communicable disease tools (toolset `cd`).
//!
//! * CD-01 `lookup_reportable_condition` (Phase 0)

pub mod reportable;

pub use reportable::*;

// Phase 3 modules (CD-02 to CD-07, CD-10, CD-13) go below this line.

// Phase 6 modules (CD-08, CD-09) go below this line.
pub mod nndss;
pub mod respiratory;
pub mod surveillance;

pub use nndss::{NotifiableTrendsArgs, NotifiableTrendsResult, get_notifiable_disease_trends};
pub use respiratory::{
    RespiratoryBriefing, RespiratorySurveillanceArgs, get_respiratory_and_wastewater_surveillance,
};
