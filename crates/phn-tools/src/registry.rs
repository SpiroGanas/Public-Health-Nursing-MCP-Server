//! Tool registry: plan IDs, toolsets, and phases.

use phn_core::Toolset;

/// Metadata about one tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolMeta {
    /// MCP tool name.
    pub name: &'static str,
    /// Plan identifier (see `brainstorming.md`).
    pub plan_id: &'static str,
    /// Toolset that enables the tool.
    pub toolset: Toolset,
    /// Plan phase that delivered the tool.
    pub phase: u8,
}

/// Every implemented tool.
pub const TOOLS: &[ToolMeta] = &[
    // Phase 0
    ToolMeta {
        name: "get_data_source_status",
        plan_id: "CORE-08",
        toolset: Toolset::Core,
        phase: 0,
    },
    ToolMeta {
        name: "configure_jurisdiction",
        plan_id: "CORE-09",
        toolset: Toolset::Core,
        phase: 0,
    },
    ToolMeta {
        name: "calculate_rates",
        plan_id: "EPI-01",
        toolset: Toolset::Epi,
        phase: 0,
    },
    ToolMeta {
        name: "lookup_reportable_condition",
        plan_id: "CD-01",
        toolset: Toolset::Cd,
        phase: 0,
    },
    // Phase 1
    ToolMeta {
        name: "read_local_file",
        plan_id: "CORE-04",
        toolset: Toolset::Core,
        phase: 1,
    },
    ToolMeta {
        name: "search_agency_policies",
        plan_id: "CORE-05",
        toolset: Toolset::Core,
        phase: 1,
    },
    ToolMeta {
        name: "export_document",
        plan_id: "CORE-06",
        toolset: Toolset::Core,
        phase: 1,
    },
    ToolMeta {
        name: "apply_small_cell_suppression",
        plan_id: "EPI-06",
        toolset: Toolset::Epi,
        phase: 1,
    },
    ToolMeta {
        name: "profile_local_dataset",
        plan_id: "EPI-07",
        toolset: Toolset::Epi,
        phase: 1,
    },
    ToolMeta {
        name: "deidentify_dataset",
        plan_id: "EPI-08",
        toolset: Toolset::Epi,
        phase: 1,
    },
    ToolMeta {
        name: "check_document_accessibility",
        plan_id: "COMM-05",
        toolset: Toolset::Comms,
        phase: 1,
    },
];

/// Look up a tool by MCP name.
pub fn tool(name: &str) -> Option<&'static ToolMeta> {
    TOOLS.iter().find(|t| t.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_is_documented() {
        let docs = include_str!("../../../docs/tools.md");
        let readme = include_str!("../../../README.md");
        for t in TOOLS {
            assert!(
                docs.contains(&format!("### `{}` ({}", t.name, t.plan_id)),
                "{} missing from docs/tools.md",
                t.name
            );
            assert!(
                readme.contains(&format!("`{}`", t.name)),
                "{} missing from README",
                t.name
            );
        }
    }
}
