//! Phase 19 tools: policy and law.
//!
//! One list per work package, so the packages can be built in parallel.

use super::ToolMeta;
#[allow(unused_imports)]
use phn_core::Toolset;

/// Legislation and federal rules: POL-01, POL-03.
pub(super) const TRACKING: &[ToolMeta] = &[
    // Phase 19 tracking entries go below this line.
    // End of Phase 19 tracking entries.
];

/// Law search, policy briefs, information sharing: POL-02, POL-04, POL-07.
pub(super) const LAW: &[ToolMeta] = &[
    // Phase 19 law entries go below this line.
    ToolMeta {
        name: "search_public_health_law",
        plan_id: "POL-02",
        toolset: Toolset::Policy,
        phase: 19,
    },
    ToolMeta {
        name: "draft_policy_brief",
        plan_id: "POL-04",
        toolset: Toolset::Policy,
        phase: 19,
    },
    ToolMeta {
        name: "health_information_sharing_guide",
        plan_id: "POL-07",
        toolset: Toolset::Policy,
        phase: 19,
    },
    // End of Phase 19 law entries.
];
