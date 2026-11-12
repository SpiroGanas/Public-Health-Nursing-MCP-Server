//! Program planning, evaluation, and grants tools (toolset `planning`).

// Phase 16 design (PLAN-01, PLAN-02, PLAN-05): add `pub mod` and `pub use` lines below.
// End of Phase 16 design.

// Phase 16 interventions (PLAN-03, PLAN-04, PLAN-11): add `pub mod` and `pub use` lines below.
// End of Phase 16 interventions.

// Phase 17 grants (PLAN-06, PLAN-07, PLAN-08): add `pub mod` and `pub use` lines below.
pub mod grant_search;
pub use grant_search::{
    GrantEligibility, GrantSearchArgs, GrantSearchResult, GrantSource, search_grant_opportunities,
};
pub mod funding_announcement;
pub use funding_announcement::{
    AnalyzeAnnouncementArgs, AnnouncementAnalysis, AnnouncementKind, analyze_funding_announcement,
};
pub mod grant_narrative;
pub use grant_narrative::{
    CriterionInput, DataPoint, NarrativeArgs, NarrativeResult, NarrativeSection, ProgramFacts,
    draft_grant_narrative,
};
// End of Phase 17 grants.

// Phase 17 budget (PLAN-09, PLAN-10): add `pub mod` and `pub use` lines below.
// End of Phase 17 budget.
