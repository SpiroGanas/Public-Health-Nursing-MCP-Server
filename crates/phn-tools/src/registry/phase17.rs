//! Phase 17 tools: grants and finance.
//!
//! One list per work package, so the packages can be built in parallel.

use super::ToolMeta;
#[allow(unused_imports)]
use phn_core::Toolset;

/// Grant search, NOFO analysis, narratives: PLAN-06, PLAN-07, PLAN-08.
pub(super) const GRANTS: &[ToolMeta] = &[
    // Phase 17 grants entries go below this line.
    ToolMeta {
        name: "search_grant_opportunities",
        plan_id: "PLAN-06",
        toolset: Toolset::Planning,
        phase: 17,
    },
    ToolMeta {
        name: "analyze_funding_announcement",
        plan_id: "PLAN-07",
        toolset: Toolset::Planning,
        phase: 17,
    },
    ToolMeta {
        name: "draft_grant_narrative",
        plan_id: "PLAN-08",
        toolset: Toolset::Planning,
        phase: 17,
    },
    // End of Phase 17 grants entries.
];

/// Budgets and economic evaluation: PLAN-09, PLAN-10.
pub(super) const BUDGET: &[ToolMeta] = &[
    // Phase 17 budget entries go below this line.
    ToolMeta {
        name: "build_program_budget",
        plan_id: "PLAN-09",
        toolset: Toolset::Planning,
        phase: 17,
    },
    ToolMeta {
        name: "economic_evaluation_calculator",
        plan_id: "PLAN-10",
        toolset: Toolset::Planning,
        phase: 17,
    },
    // End of Phase 17 budget entries.
];

/// Department budget and staffing: LEAD-01, LEAD-02.
pub(super) const FINANCE: &[ToolMeta] = &[
    // Phase 17 finance entries go below this line.
    ToolMeta {
        name: "analyze_health_department_budget",
        plan_id: "LEAD-01",
        toolset: Toolset::Leadership,
        phase: 17,
    },
    ToolMeta {
        name: "model_staffing_needs",
        plan_id: "LEAD-02",
        toolset: Toolset::Leadership,
        phase: 17,
    },
    // End of Phase 17 finance entries.
];
