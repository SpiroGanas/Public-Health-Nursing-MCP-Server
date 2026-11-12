//! Evidence-based practice and quality improvement tools (toolset `ebp`).

// Phase 2 modules (EBP-06) go below this line.
pub mod spc;
pub use spc::{SpcArgs, SpcResult, create_spc_chart};

// Phase 4 modules (EBP-01, EBP-02) go below this line.
pub mod cdc_guidance;
pub mod pubmed;

pub use cdc_guidance::{CdcGuidanceArgs, CdcGuidanceResult, search_cdc_guidance};
pub use pubmed::{PubmedResult, SearchPubmedArgs, search_pubmed};

// Phase 16 appraise (EBP-03, EBP-04): add `pub mod` and `pub use` lines below.
pub mod appraisal;
pub mod appraise;
pub mod prisma;
pub mod synthesis;

pub use appraise::{
    AppraiseEvidenceArgs, AppraiseEvidenceResult, DoiOutcome, appraise_evidence, lookup_doi,
};
pub use synthesis::{SynthesisArgs, SynthesisResult, build_evidence_synthesis_table};
// End of Phase 16 appraise.

// Phase 16 qi (EBP-05): add `pub mod` and `pub use` lines below.
pub mod qi;
pub use qi::{QiPlanArgs, QiPlanResult, plan_quality_improvement};
// End of Phase 16 qi.

// Phase 18 ethics (EBP-07): add `pub mod` and `pub use` lines below.
pub mod qi_research;
pub use qi_research::{QiResearchArgs, QiResearchResult, qi_vs_research_determination};
// End of Phase 18 ethics.
