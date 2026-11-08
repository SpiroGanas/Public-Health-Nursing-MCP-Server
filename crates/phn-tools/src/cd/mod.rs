//! Communicable disease tools (toolset `cd`).
//!
//! * CD-01 `lookup_reportable_condition` (Phase 0)
//! * Phase 3: CD-02 `search_ma_disease_control_guide`, CD-03
//!   `calculate_isolation_and_exclusion`, CD-04 `analyze_outbreak_line_list`,
//!   CD-05 `generate_epi_curve`, CD-06 `plan_contact_tracing`, CD-07
//!   `assess_measles_exposure`, CD-10 `detect_aberrations`, CD-13
//!   `infection_control_for_congregate_settings`

pub mod reportable;

pub use reportable::*;

// Phase 3 modules (CD-02 to CD-07, CD-10, CD-13) go below this line.
pub mod aberrations;
pub mod contact_tracing;
pub mod engine;
pub mod epicurve;
pub mod guide;
pub mod infection_control;
pub mod isolation;
pub mod line_list;
pub mod measles;
pub mod outbreak;

pub use aberrations::{AberrationArgs, AberrationResult, detect_aberrations};
pub use epicurve::{EpiCurveArgs, EpiCurveResult, generate_epi_curve};
pub use guide::{GuideSearchArgs, GuideSearchResult, search_ma_disease_control_guide};
pub use contact_tracing::{ContactTracingArgs, ContactTracingResult, plan_contact_tracing};
pub use infection_control::{
    InfectionControlArgs, InfectionControlResult, infection_control_for_congregate_settings,
};
pub use measles::{MeaslesExposureArgs, MeaslesExposureResult, assess_measles_exposure};
pub use outbreak::{OutbreakArgs, OutbreakResult, analyze_outbreak_line_list};
pub use isolation::{IsolationArgs, IsolationResult, calculate_isolation_and_exclusion};

// Phase 6 modules (CD-08, CD-09) go below this line.
