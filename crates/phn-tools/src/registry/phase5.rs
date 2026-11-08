//! Phase 5 tools: immunizations I.

use super::ToolMeta;
use phn_core::Toolset;

/// Tools delivered in Phase 5.
pub(super) const TOOLS: &[ToolMeta] = &[
    ToolMeta {
        name: "get_immunization_schedule",
        plan_id: "VAX-01",
        toolset: Toolset::Vax,
        phase: 5,
    },
    ToolMeta {
        name: "compare_vaccine_recommendations",
        plan_id: "VAX-02",
        toolset: Toolset::Vax,
        phase: 5,
    },
    ToolMeta {
        name: "check_ma_school_immunization_requirements",
        plan_id: "VAX-04",
        toolset: Toolset::Vax,
        phase: 5,
    },
    ToolMeta {
        name: "analyze_vaccination_coverage",
        plan_id: "VAX-05",
        toolset: Toolset::Vax,
        phase: 5,
    },
    ToolMeta {
        name: "vaccine_temperature_excursion_helper",
        plan_id: "VAX-06",
        toolset: Toolset::Vax,
        phase: 5,
    },
    ToolMeta {
        name: "lookup_vaccine_product",
        plan_id: "VAX-07",
        toolset: Toolset::Vax,
        phase: 5,
    },
    ToolMeta {
        name: "plan_vaccination_clinic",
        plan_id: "VAX-08",
        toolset: Toolset::Vax,
        phase: 5,
    },
    ToolMeta {
        name: "vaccine_confidence_conversation_guide",
        plan_id: "VAX-09",
        toolset: Toolset::Vax,
        phase: 5,
    },
    ToolMeta {
        name: "lookup_medication_information",
        plan_id: "CORE-07",
        toolset: Toolset::Core,
        phase: 5,
    },
];
