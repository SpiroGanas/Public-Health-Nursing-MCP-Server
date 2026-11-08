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
    ToolMeta {
        name: "get_notifiable_disease_trends",
        plan_id: "CD-08",
        toolset: Toolset::Cd,
        phase: 6,
    },
    ToolMeta {
        name: "get_respiratory_and_wastewater_surveillance",
        plan_id: "CD-09",
        toolset: Toolset::Cd,
        phase: 6,
    },
];
