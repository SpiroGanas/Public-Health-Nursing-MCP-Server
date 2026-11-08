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
];
