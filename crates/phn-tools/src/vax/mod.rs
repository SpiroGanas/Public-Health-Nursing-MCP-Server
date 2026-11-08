//! Immunization tools (toolset `vax`).

// Phase 5 modules (VAX-01, VAX-02, VAX-04 to VAX-09) go below this line.
pub mod clinic;
pub mod compare;
pub mod confidence;
pub mod content;
pub mod coverage;
pub mod dates;
pub mod excursion;
pub mod products;
pub mod readability;
pub mod schedule;
pub mod school;

pub use clinic::{ClinicArgs, ClinicPlan, plan_vaccination_clinic};
pub use compare::{CompareArgs, CompareResult, compare_vaccine_recommendations};
pub use confidence::{
    ConfidenceArgs, ConfidenceGuideResult, vaccine_confidence_conversation_guide,
};
pub use coverage::{CoverageArgs, CoverageResult, analyze_vaccination_coverage};
pub use excursion::{ExcursionArgs, ExcursionResult, vaccine_temperature_excursion_helper};
pub use products::{ProductArgs, ProductLookup, lookup_vaccine_product};
pub use schedule::{ScheduleArgs, ScheduleResult, get_immunization_schedule};
pub use school::{SchoolCheckArgs, SchoolCheckResult, check_ma_school_immunization_requirements};

// Phase 6 modules (VAX-03, the CDSi forecaster) go below this line.
pub mod forecast;
pub use forecast::{
    ForecastImmunizationsArgs, ForecastImmunizationsResult, forecast_immunizations_due,
};
