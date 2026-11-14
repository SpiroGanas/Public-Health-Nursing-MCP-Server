//! Phase 16 tools: program planning, evaluation, and evidence.
//!
//! One list per work package, so the packages can be built in parallel.

use super::ToolMeta;
#[allow(unused_imports)]
use phn_core::Toolset;

/// Logic models, objectives, evaluation plans: PLAN-01, PLAN-02, PLAN-05.
pub(super) const DESIGN: &[ToolMeta] = &[
    // Phase 16 design entries go below this line.
    ToolMeta {
        name: "build_logic_model",
        plan_id: "PLAN-01",
        toolset: Toolset::Planning,
        phase: 16,
    },
    ToolMeta {
        name: "write_smartie_objectives",
        plan_id: "PLAN-02",
        toolset: Toolset::Planning,
        phase: 16,
    },
    ToolMeta {
        name: "build_evaluation_plan",
        plan_id: "PLAN-05",
        toolset: Toolset::Planning,
        phase: 16,
    },
    // End of Phase 16 design entries.
];

/// Interventions, theory, performance tracking: PLAN-03, PLAN-04, PLAN-11.
pub(super) const INTERVENTIONS: &[ToolMeta] = &[
    // Phase 16 interventions entries go below this line.
    ToolMeta {
        name: "find_evidence_based_interventions",
        plan_id: "PLAN-03",
        toolset: Toolset::Planning,
        phase: 16,
    },
    ToolMeta {
        name: "apply_behavior_change_theory",
        plan_id: "PLAN-04",
        toolset: Toolset::Planning,
        phase: 16,
    },
    ToolMeta {
        name: "track_program_performance",
        plan_id: "PLAN-11",
        toolset: Toolset::Planning,
        phase: 16,
    },
    // End of Phase 16 interventions entries.
];

/// Evidence appraisal and synthesis: EBP-03, EBP-04.
pub(super) const APPRAISE: &[ToolMeta] = &[
    // Phase 16 appraise entries go below this line.
    ToolMeta {
        name: "appraise_evidence",
        plan_id: "EBP-03",
        toolset: Toolset::Ebp,
        phase: 16,
    },
    ToolMeta {
        name: "build_evidence_synthesis_table",
        plan_id: "EBP-04",
        toolset: Toolset::Ebp,
        phase: 16,
    },
    // End of Phase 16 appraise entries.
];

/// Quality improvement planning: EBP-05.
pub(super) const QI: &[ToolMeta] = &[
    // Phase 16 qi entries go below this line.
    ToolMeta {
        name: "plan_quality_improvement",
        plan_id: "EBP-05",
        toolset: Toolset::Ebp,
        phase: 16,
    },
    // End of Phase 16 qi entries.
];
