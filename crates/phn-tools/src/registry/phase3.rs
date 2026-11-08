//! Phase 3 tools: communicable disease I.

use super::ToolMeta;
use phn_core::Toolset;

/// Tools delivered in Phase 3.
pub(super) const TOOLS: &[ToolMeta] = &[
    ToolMeta {
        name: "search_ma_disease_control_guide",
        plan_id: "CD-02",
        toolset: Toolset::Cd,
        phase: 3,
    },
    ToolMeta {
        name: "calculate_isolation_and_exclusion",
        plan_id: "CD-03",
        toolset: Toolset::Cd,
        phase: 3,
    },
    ToolMeta {
        name: "analyze_outbreak_line_list",
        plan_id: "CD-04",
        toolset: Toolset::Cd,
        phase: 3,
    },
    ToolMeta {
        name: "generate_epi_curve",
        plan_id: "CD-05",
        toolset: Toolset::Cd,
        phase: 3,
    },
    ToolMeta {
        name: "plan_contact_tracing",
        plan_id: "CD-06",
        toolset: Toolset::Cd,
        phase: 3,
    },
    ToolMeta {
        name: "assess_measles_exposure",
        plan_id: "CD-07",
        toolset: Toolset::Cd,
        phase: 3,
    },
    ToolMeta {
        name: "detect_aberrations",
        plan_id: "CD-10",
        toolset: Toolset::Cd,
        phase: 3,
    },
    ToolMeta {
        name: "infection_control_for_congregate_settings",
        plan_id: "CD-13",
        toolset: Toolset::Cd,
        phase: 3,
    },
];
