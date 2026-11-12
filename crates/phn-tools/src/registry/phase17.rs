//! Phase 17 tools: grants and finance.
//!
//! One list per work package, so the packages can be built in parallel.

use super::ToolMeta;
#[allow(unused_imports)]
use phn_core::Toolset;

/// Grant search, NOFO analysis, narratives: PLAN-06, PLAN-07, PLAN-08.
pub(super) const GRANTS: &[ToolMeta] = &[
    // Phase 17 grants entries go below this line.
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
    // End of Phase 17 finance entries.
];
