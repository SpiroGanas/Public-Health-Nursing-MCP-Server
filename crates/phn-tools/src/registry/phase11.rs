//! Phase 11 tools: substance use, crisis resources, and health communication.

use super::ToolMeta;
use phn_core::Toolset;

/// Tools delivered in Phase 11.
pub(super) const TOOLS: &[ToolMeta] = &[
    ToolMeta {
        name: "check_readability",
        plan_id: "COMM-01",
        toolset: Toolset::Comms,
        phase: 11,
    },
    ToolMeta {
        name: "get_overdose_data",
        plan_id: "MH-03",
        toolset: Toolset::Bh,
        phase: 11,
    },
];
