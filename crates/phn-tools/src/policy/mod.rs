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
pub use legislation::{TrackArgs, TrackResult, track_ma_legislation};
pub mod federal;
pub use federal::{FederalArgs, FederalResult, monitor_federal_rules};
// End of Phase 19 tracking.

// Phase 19 law (POL-02, POL-04, POL-07): add `pub mod` and `pub use` lines below.
// End of Phase 19 law.
