//! Phase 6 tools: immunization forecasting and surveillance feeds.

use super::ToolMeta;
#[allow(unused_imports)]
use phn_core::Toolset;

/// Tools delivered in Phase 6.
pub(super) const TOOLS: &[ToolMeta] = &[
    ToolMeta {
        name: "forecast_immunizations_due",
        plan_id: "VAX-03",
        toolset: Toolset::Vax,
        phase: 6,
    },
];
