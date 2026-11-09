//! Phase 11 tools: substance use, crisis resources, and health communication.

use super::ToolMeta;
use phn_core::Toolset;

/// Tools delivered in Phase 11.
pub(super) const TOOLS: &[ToolMeta] = &[
    ToolMeta {
        name: "check_readability",
        plan_id: "COMM-01",
        toolset: Toolset::Comms,
        phase: 11,
    },
    ToolMeta {
        name: "get_overdose_data",
        plan_id: "MH-03",
        toolset: Toolset::Bh,
        phase: 11,
    },
    ToolMeta {
        name: "get_youth_risk_behavior_data",
        plan_id: "MH-07",
        toolset: Toolset::Bh,
        phase: 11,
    },
    ToolMeta {
        name: "find_naloxone_and_harm_reduction",
        plan_id: "MH-04",
        toolset: Toolset::Bh,
        phase: 11,
    },
    ToolMeta {
        name: "find_substance_use_treatment",
        plan_id: "MH-05",
        toolset: Toolset::Bh,
        phase: 11,
    },
    ToolMeta {
        name: "plan_school_postvention",
        plan_id: "MH-06",
        toolset: Toolset::Bh,
        phase: 11,
    },
    ToolMeta {
        name: "translate_health_material",
        plan_id: "COMM-02",
        toolset: Toolset::Comms,
        phase: 11,
    },
    ToolMeta {
        name: "create_social_media_campaign",
        plan_id: "COMM-03",
        toolset: Toolset::Comms,
        phase: 11,
    },
    ToolMeta {
        name: "draft_press_materials",
        plan_id: "COMM-04",
        toolset: Toolset::Comms,
        phase: 11,
    },
];
