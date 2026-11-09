//! Phase 12 tools: emergency preparedness I: situational awareness and planning.

use super::ToolMeta;
use phn_core::Toolset;

/// Tools delivered in Phase 12.
pub(super) const TOOLS: &[ToolMeta] = &[
    ToolMeta {
        name: "get_active_hazard_alerts",
        plan_id: "EPR-05",
        toolset: Toolset::Epr,
        phase: 12,
    },
    ToolMeta {
        name: "model_dispensing_site_throughput",
        plan_id: "EPR-02",
        toolset: Toolset::Epr,
        phase: 12,
    },
    ToolMeta {
        name: "air_quality_protective_guidance",
        plan_id: "ENV-02",
        toolset: Toolset::Env,
        phase: 12,
    },
    ToolMeta {
        name: "heat_health_action_planner",
        plan_id: "ENV-01",
        toolset: Toolset::Env,
        phase: 12,
    },
    ToolMeta {
        name: "hazard_vulnerability_assessment",
        plan_id: "EPR-06",
        toolset: Toolset::Epr,
        phase: 12,
    },
    ToolMeta {
        name: "functional_needs_shelter_intake",
        plan_id: "EPR-03",
        toolset: Toolset::Epr,
        phase: 12,
    },
    ToolMeta {
        name: "get_electricity_dependent_population",
        plan_id: "EPR-04",
        toolset: Toolset::Epr,
        phase: 12,
    },
    ToolMeta {
        name: "plan_casper_assessment",
        plan_id: "EPR-11",
        toolset: Toolset::Epr,
        phase: 12,
    },
];
