//! Phase 7 tools: school health.

use super::ToolMeta;
use phn_core::Toolset;

/// Tools delivered in Phase 7.
pub(super) const TOOLS: &[ToolMeta] = &[
    ToolMeta {
        name: "get_ma_school_screening_requirements",
        plan_id: "SCH-01",
        toolset: Toolset::School,
        phase: 7,
    },
    ToolMeta {
        name: "track_school_screenings",
        plan_id: "SCH-02",
        toolset: Toolset::School,
        phase: 7,
    },
    ToolMeta {
        name: "school_medication_administration_guide",
        plan_id: "SCH-03",
        toolset: Toolset::School,
        phase: 7,
    },
    ToolMeta {
        name: "build_individualized_health_plan",
        plan_id: "SCH-04",
        toolset: Toolset::School,
        phase: 7,
    },
    ToolMeta {
        name: "concussion_return_to_learn_tracker",
        plan_id: "SCH-05",
        toolset: Toolset::School,
        phase: 7,
    },
    ToolMeta {
        name: "monitor_school_absenteeism",
        plan_id: "SCH-06",
        toolset: Toolset::School,
        phase: 7,
    },
    ToolMeta {
        name: "build_school_health_annual_report",
        plan_id: "SCH-07",
        toolset: Toolset::School,
        phase: 7,
    },
    ToolMeta {
        name: "child_care_health_consultation_checklist",
        plan_id: "SCH-08",
        toolset: Toolset::School,
        phase: 7,
    },
];
