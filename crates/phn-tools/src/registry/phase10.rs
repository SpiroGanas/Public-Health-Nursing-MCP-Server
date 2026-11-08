//! Phase 10 tools: healthy aging and mental health screening.

use super::ToolMeta;
use phn_core::Toolset;

/// Tools delivered in Phase 10.
pub(super) const TOOLS: &[ToolMeta] = &[
    ToolMeta {
        name: "score_mental_health_screen",
        plan_id: "MH-02",
        toolset: Toolset::Bh,
        phase: 10,
    },
    ToolMeta {
        name: "find_crisis_resources",
        plan_id: "MH-01",
        toolset: Toolset::Bh,
        phase: 10,
    },
    ToolMeta {
        name: "steadi_fall_risk_assessment",
        plan_id: "AGE-01",
        toolset: Toolset::Aging,
        phase: 10,
    },
    ToolMeta {
        name: "elder_protective_services_guide",
        plan_id: "AGE-03",
        toolset: Toolset::Aging,
        phase: 10,
    },
];
