//! Policy, law, and ethics tools (toolset `policy`).

// Phase 18 ethics (POL-05, POL-06): add `pub mod` and `pub use` lines below.
pub mod ethics;
pub mod scope;
pub mod shared;
pub use ethics::{EthicsArgs, EthicsResult, public_health_ethics_analysis};
pub use scope::{ScopeArgs, ScopeResult, nursing_scope_of_practice_guide};
// End of Phase 18 ethics.

// Phase 19 tracking (POL-01, POL-03): add `pub mod` and `pub use` lines below.
pub mod legislation;
pub use legislation::{TrackArgs, TrackResult, track_ma_legislation, track_ma_legislation_at};
pub mod federal;
pub use federal::{FederalArgs, FederalResult, monitor_federal_rules, monitor_federal_rules_at};
// End of Phase 19 tracking.

// Phase 19 law (POL-02, POL-04, POL-07): add `pub mod` and `pub use` lines below.
pub mod brief;
pub mod law;
pub mod sharing;
pub use brief::{BriefArgs, BriefResult, draft_policy_brief};
pub use law::{LawResult, LawSearchArgs, search_public_health_law};
pub use sharing::{SharingArgs, SharingResult, health_information_sharing_guide};
// End of Phase 19 law.
