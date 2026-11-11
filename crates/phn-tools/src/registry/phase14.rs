//! Phase 14 tools: environmental health and climate.
//!
//! One list per work package, so the packages can be built in parallel.

#[allow(unused_imports)]
use super::ToolMeta;
#[allow(unused_imports)]
use phn_core::Toolset;

/// Water: ENV-03 `drinking_water_quality_lookup`, CHR-07 `community_water_fluoridation_status`.
pub(super) const WATER: &[ToolMeta] = &[
    ToolMeta {
        name: "drinking_water_quality_lookup",
        plan_id: "ENV-03",
        toolset: Toolset::Env,
        phase: 14,
    },
    ToolMeta {
        name: "community_water_fluoridation_status",
        plan_id: "CHR-07",
        toolset: Toolset::Chronic,
        phase: 14,
    },
];

/// Environmental justice and climate: ENV-04 `environmental_justice_screen`, ENV-05
/// `climate_health_vulnerability_profile`.
pub(super) const EJ_CLIMATE: &[ToolMeta] = &[];

/// Vectors and housing: ENV-06 `vector_borne_disease_risk`, ENV-07
/// `housing_conditions_health_guide`.
pub(super) const VECTOR_HOUSING: &[ToolMeta] = &[];

/// Chemicals and food: ENV-08 `chemical_exposure_reference`, CHR-06 `food_access_mapper`.
pub(super) const CHEM_FOOD: &[ToolMeta] = &[];
