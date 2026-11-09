//! Tool registry: plan IDs, toolsets, and phases.
//!
//! Each phase keeps its tools in its own file (`phase2.rs`, ...) so work on
//! different phases does not collide. [`tools`] lists every implemented tool.

use phn_core::Toolset;

mod phase10;
mod phase11;
mod phase12;
mod phase13;
mod phase2;
mod phase3;
mod phase4;
mod phase5;
mod phase6;
mod phase7;
mod phase8;
mod phase9;

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

/// Tools delivered in Phases 0 and 1.
const PHASE_0_1: &[ToolMeta] = &[
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

/// Every phase's tool list, in phase order.
const PHASES: &[&[ToolMeta]] = &[
    PHASE_0_1,
    phase2::TOOLS,
    phase3::TOOLS,
    phase4::TOOLS,
    phase5::TOOLS,
    phase6::TOOLS,
    phase7::TOOLS,
    phase8::TOOLS,
    phase9::TOOLS,
    phase10::TOOLS,
    phase11::TOOLS,
    phase12::TOOLS,
    phase13::ICS,
    phase13::COOP,
    phase13::HSEEP,
    phase13::CERC,
];

/// Every implemented tool, in phase order.
pub fn tools() -> impl Iterator<Item = &'static ToolMeta> {
    PHASES.iter().flat_map(|p| p.iter())
}

/// Look up a tool by MCP name.
pub fn tool(name: &str) -> Option<&'static ToolMeta> {
    tools().find(|t| t.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_tool_is_documented() {
        let docs = include_str!("../../../../docs/tools.md");
        let readme = include_str!("../../../../README.md");
        for t in tools() {
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

    /// The user guide (docs/user-guide) describes exactly the implemented tools, each in
    /// its own toolset, so it never documents a planned tool or misses a new one.
    #[test]
    fn user_guide_lists_exactly_the_implemented_tools() {
        let guide = include_str!("../../../../docs/user-guide/guide-content.js");
        let mut described = std::collections::BTreeMap::new();
        for (pos, _) in guide.match_indices("\n        id: '") {
            let rest = &guide[pos + "\n        id: '".len()..];
            let name = &rest[..rest.find('\'').expect("closing quote after id")];
            let ts = rest
                .find("toolset: '")
                .map(|i| &rest[i + "toolset: '".len()..])
                .and_then(|s| s.find('\'').map(|j| &s[..j]))
                .unwrap_or_else(|| panic!("{name} has no toolset in the user guide"));
            assert!(
                described.insert(name, ts).is_none(),
                "{name} is described twice in the user guide"
            );
        }
        for t in tools() {
            let ts = described.remove(t.name).unwrap_or_else(|| {
                panic!(
                    "{} missing from docs/user-guide/guide-content.js (add it and rebuild the guide)",
                    t.name
                )
            });
            assert_eq!(
                ts,
                t.toolset.as_str(),
                "{} has the wrong toolset in the user guide",
                t.name
            );
        }
        assert!(
            described.is_empty(),
            "the user guide describes tools that are not implemented: {:?}",
            described.keys().collect::<Vec<_>>()
        );
        let count = tools().count();
        assert!(
            guide.contains(&format!("const TOOL_COUNT = {count};")),
            "TOOL_COUNT in the user guide should be {count}"
        );
    }

    /// Every user-guide entry uses an `internet` value the guide builder knows
    /// (an unknown value makes `build-user-guide.js` fail).
    #[test]
    fn user_guide_internet_values_are_known() {
        let guide = include_str!("../../../../docs/user-guide/guide-content.js");
        let mut seen = 0;
        for (pos, _) in guide.match_indices("\n        internet: '") {
            let rest = &guide[pos + "\n        internet: '".len()..];
            let value = &rest[..rest.find('\'').expect("closing quote after internet")];
            assert!(
                ["no", "optional", "yes"].contains(&value),
                "unknown internet value '{value}' in the user guide (use no, optional, or yes)"
            );
            seen += 1;
        }
        assert_eq!(
            seen,
            tools().count(),
            "every guide entry needs an internet value"
        );
    }

    #[test]
    fn names_and_plan_ids_are_unique() {
        let mut names = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for t in tools() {
            assert!(names.insert(t.name), "duplicate tool name {}", t.name);
            assert!(ids.insert(t.plan_id), "duplicate plan ID {}", t.plan_id);
        }
    }

    #[test]
    fn each_phase_file_lists_only_its_own_phase() {
        for (i, list) in PHASES.iter().enumerate().skip(1) {
            for t in *list {
                assert_eq!(
                    usize::from(t.phase),
                    i + 1,
                    "{} is in the wrong phase file",
                    t.name
                );
            }
        }
    }
}
