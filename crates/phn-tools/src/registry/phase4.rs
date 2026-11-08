//! Phase 4 tools: public data and evidence platform.

use super::ToolMeta;
use phn_core::Toolset;

/// Tools delivered in Phase 4.
pub(super) const TOOLS: &[ToolMeta] = &[
    ToolMeta {
        name: "build_community_health_profile",
        plan_id: "EQ-01",
        toolset: Toolset::Equity,
        phase: 4,
    },
    ToolMeta {
        name: "get_social_vulnerability_index",
        plan_id: "EQ-02",
        toolset: Toolset::Equity,
        phase: 4,
    },
    ToolMeta {
        name: "language_access_profile",
        plan_id: "EQ-03",
        toolset: Toolset::Equity,
        phase: 4,
    },
    ToolMeta {
        name: "measure_health_disparities",
        plan_id: "EQ-04",
        toolset: Toolset::Equity,
        phase: 4,
    },
    ToolMeta {
        name: "get_places_estimates",
        plan_id: "CHR-01",
        toolset: Toolset::Chronic,
        phase: 4,
    },
    ToolMeta {
        name: "create_choropleth_map",
        plan_id: "EPI-12",
        toolset: Toolset::Epi,
        phase: 4,
    },
    ToolMeta {
        name: "search_pubmed",
        plan_id: "EBP-01",
        toolset: Toolset::Ebp,
        phase: 4,
    },
    ToolMeta {
        name: "search_cdc_guidance",
        plan_id: "EBP-02",
        toolset: Toolset::Ebp,
        phase: 4,
    },
];
