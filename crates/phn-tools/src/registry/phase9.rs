//! Phase 9 tools: PHN practice toolkit and case management.

use super::ToolMeta;
use phn_core::Toolset;

/// Tools delivered in Phase 9.
pub(super) const TOOLS: &[ToolMeta] = &[
    ToolMeta {
        name: "plan_tb_contact_investigation",
        plan_id: "CD-11",
        toolset: Toolset::Cd,
        phase: 9,
    },
    ToolMeta {
        name: "ltbi_treatment_calendar",
        plan_id: "CD-12",
        toolset: Toolset::Cd,
        phase: 9,
    },
    ToolMeta {
        name: "screen_health_related_social_needs",
        plan_id: "EQ-05",
        toolset: Toolset::Equity,
        phase: 9,
    },
    ToolMeta {
        name: "omaha_system_coder",
        plan_id: "CORE-01",
        toolset: Toolset::Core,
        phase: 9,
    },
    ToolMeta {
        name: "classify_intervention_wheel",
        plan_id: "CORE-02",
        toolset: Toolset::Core,
        phase: 9,
    },
];
