//! Phase 2 tools: epidemiology and biostatistics engine.

use super::ToolMeta;
use phn_core::Toolset;

/// Tools delivered in Phase 2.
pub(super) const TOOLS: &[ToolMeta] = &[
    ToolMeta {
        name: "age_adjust_rates",
        plan_id: "EPI-02",
        toolset: Toolset::Epi,
        phase: 2,
    },
    ToolMeta {
        name: "analyze_two_by_two",
        plan_id: "EPI-03",
        toolset: Toolset::Epi,
        phase: 2,
    },
    ToolMeta {
        name: "calculate_sample_size",
        plan_id: "EPI-04",
        toolset: Toolset::Epi,
        phase: 2,
    },
    ToolMeta {
        name: "compute_standardized_ratio",
        plan_id: "EPI-05",
        toolset: Toolset::Epi,
        phase: 2,
    },
    ToolMeta {
        name: "analyze_trends",
        plan_id: "EPI-09",
        toolset: Toolset::Epi,
        phase: 2,
    },
    ToolMeta {
        name: "run_statistical_test",
        plan_id: "EPI-10",
        toolset: Toolset::Epi,
        phase: 2,
    },
    ToolMeta {
        name: "create_chart",
        plan_id: "EPI-11",
        toolset: Toolset::Epi,
        phase: 2,
    },
    ToolMeta {
        name: "create_spc_chart",
        plan_id: "EBP-06",
        toolset: Toolset::Ebp,
        phase: 2,
    },
];
