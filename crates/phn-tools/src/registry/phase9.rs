//! Phase 9 tools: PHN practice toolkit and case management.

use super::ToolMeta;
use phn_core::Toolset;

/// Tools delivered in Phase 9.
pub(super) const TOOLS: &[ToolMeta] = &[
    ToolMeta {
        name: "plan_tb_contact_investigation",
        plan_id: "CD-11",
        toolset: Toolset::Cd,
        phase: 9,
    },
];
