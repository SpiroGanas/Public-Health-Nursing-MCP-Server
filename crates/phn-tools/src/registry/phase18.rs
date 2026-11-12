//! Phase 18 tools: leadership, accreditation, and professional practice.
//!
//! One list per work package, so the packages can be built in parallel.

use super::ToolMeta;
#[allow(unused_imports)]
use phn_core::Toolset;

/// FPHS, PHAB, strategic planning: LEAD-03, LEAD-04, LEAD-05.
pub(super) const STRATEGY: &[ToolMeta] = &[
    // Phase 18 strategy entries go below this line.
    ToolMeta {
        name: "assess_foundational_public_health_services",
        plan_id: "LEAD-03",
        toolset: Toolset::Leadership,
        phase: 18,
    },
    ToolMeta {
        name: "phab_accreditation_crosswalk",
        plan_id: "LEAD-04",
        toolset: Toolset::Leadership,
        phase: 18,
    },
    ToolMeta {
        name: "build_strategic_plan",
        plan_id: "LEAD-05",
        toolset: Toolset::Leadership,
        phase: 18,
    },
    // End of Phase 18 strategy entries.
];

/// Workforce, board of health, standing orders: LEAD-06, LEAD-07, LEAD-08.
pub(super) const WORKFORCE: &[ToolMeta] = &[
    // Phase 18 workforce entries go below this line.
    // End of Phase 18 workforce entries.
];

/// QI vs research, ethics, scope of practice: EBP-07, POL-05, POL-06.
pub(super) const ETHICS: &[ToolMeta] = &[
    // Phase 18 ethics entries go below this line.
    // End of Phase 18 ethics entries.
];
