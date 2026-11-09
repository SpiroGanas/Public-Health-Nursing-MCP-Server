//! Phase 13 tools: emergency preparedness II: ICS, plans, and exercises.
//!
//! Each work package keeps its own list so parallel work does not collide;
//! `registry/mod.rs` includes all four.

use super::ToolMeta;
#[allow(unused_imports)]
use phn_core::Toolset;

/// EPR-01 (ICS forms and Incident Action Plan).
pub(super) const ICS: &[ToolMeta] = &[ToolMeta {
    name: "generate_ics_forms",
    plan_id: "EPR-01",
    toolset: Toolset::Epr,
    phase: 13,
}];

/// EPR-07 (continuity of operations).
pub(super) const COOP: &[ToolMeta] = &[];

/// EPR-08 and EPR-09 (HSEEP exercise design and after-action report).
pub(super) const HSEEP: &[ToolMeta] = &[];

/// EPR-10 (crisis and emergency risk communication).
pub(super) const CERC: &[ToolMeta] = &[];
