//! Phase 8 tools: maternal, child, and family health.

use super::ToolMeta;
use phn_core::Toolset;

/// Tools delivered in Phase 8.
pub(super) const TOOLS: &[ToolMeta] = &[
    ToolMeta {
        name: "calculate_growth_percentiles",
        plan_id: "MCH-03",
        toolset: Toolset::Mch,
        phase: 8,
    },
    ToolMeta {
        name: "childhood_lead_screening_guide",
        plan_id: "MCH-04",
        toolset: Toolset::Mch,
        phase: 8,
    },
    ToolMeta {
        name: "mandated_reporter_51a_guide",
        plan_id: "MCH-07",
        toolset: Toolset::Mch,
        phase: 8,
    },
    ToolMeta {
        name: "check_developmental_milestones",
        plan_id: "MCH-02",
        toolset: Toolset::Mch,
        phase: 8,
    },
    ToolMeta {
        name: "plan_newborn_postpartum_visit",
        plan_id: "MCH-01",
        toolset: Toolset::Mch,
        phase: 8,
    },
    ToolMeta {
        name: "estimate_benefit_eligibility",
        plan_id: "EQ-06",
        toolset: Toolset::Equity,
        phase: 8,
    },
    ToolMeta {
        name: "map_lead_risk",
        plan_id: "MCH-05",
        toolset: Toolset::Mch,
        phase: 8,
    },
];
