//! Phase 15 tools: chronic disease prevention and community assessment.
//!
//! One list per work package, so the packages can be built in parallel.

use super::ToolMeta;
#[allow(unused_imports)]
use phn_core::Toolset;

/// BP screening and diabetes prevention: CHR-02, CHR-03.
pub(super) const SCREEN: &[ToolMeta] = &[
    // Phase 15 screen entries go below this line.
    // End of Phase 15 screen entries.
];

/// Preventive services and asthma: CHR-04, CHR-05.
pub(super) const PREVENT: &[ToolMeta] = &[
    // Phase 15 prevent entries go below this line.
    // End of Phase 15 prevent entries.
];

/// Community health assessment: EQ-08, EQ-09, EQ-10.
pub(super) const CHA: &[ToolMeta] = &[
    // Phase 15 cha entries go below this line.
    ToolMeta {
        name: "community_health_assessment_builder",
        plan_id: "EQ-08",
        toolset: Toolset::Equity,
        phase: 15,
    },
    ToolMeta {
        name: "prioritize_health_issues",
        plan_id: "EQ-09",
        toolset: Toolset::Equity,
        phase: 15,
    },
    ToolMeta {
        name: "analyze_qualitative_data",
        plan_id: "EQ-10",
        toolset: Toolset::Equity,
        phase: 15,
    },
    // End of Phase 15 cha entries.
];

/// Offline geocoder: EPI-13.
pub(super) const GEO: &[ToolMeta] = &[
    // Phase 15 geo entries go below this line.
    // End of Phase 15 geo entries.
];
