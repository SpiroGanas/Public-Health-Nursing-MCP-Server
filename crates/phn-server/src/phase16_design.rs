//! Phase 16 tools (Logic models, objectives, evaluation plans): PLAN-01, PLAN-02, PLAN-05.

use crate::server::{PhnServer, blocking, respond};
use phn_core::Envelope;
use phn_tools::planning::{
    EvaluationPlanArgs, EvaluationPlanResult, LogicModelArgs, LogicModelResult, SmartieArgs,
    SmartieResult,
};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

// Phase 16 design: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase16_design_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "build_logic_model",
        description = "Builds a program logic model from the program description you and the nurse \
            worked out: inputs, activities, outputs, short-term, intermediate, and long-term \
            outcomes, assumptions, and external (contextual) factors, following CDC's Program \
            Evaluation Framework (Step 2). Pass the components as short phrases (draft them from \
            the nurse's description; the tool does not invent program content). It checks the \
            structure and wording (missing activities or outcomes, outcomes written as \
            activities, outcomes that do not say what changes, too many or too long boxes, \
            missing assumptions or context), reads the model as if-then statements, drafts the \
            narrative CDC pairs with it, and draws it as Mermaid text and a self-contained \
            accessible SVG (alt text and a full text description included). Writes a Word (or PDF) \
            one-page table, the SVG, a PNG, and the Mermaid text to /data/output.",
        annotations(
            title = "Build a logic model",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<LogicModelResult>>()
    )]
    async fn build_logic_model(
        &self,
        Parameters(args): Parameters<LogicModelArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::planning::build_logic_model(ctx, args)
        })
        .await
    }

    #[tool(
        name = "write_smartie_objectives",
        description = "Drafts and checks SMARTIE objectives (specific, measurable, achievable, \
            relevant, time-bound, inclusive, equitable; CDC's SMARTIE questions). Give each \
            objective in parts (measure, direction, population, baseline, target, unit, target \
            date, data source, why it is achievable and relevant, how the population gave input, \
            the equity dimension and the groups results are broken down by). The tool assembles \
            a statement with bracketed placeholders for what is missing, flags missing baseline, \
            target, date, population, data source, and equity dimension, catches a target that \
            moves the wrong way or a date before the start, and computes the change from \
            baseline to target; whether a target is achievable, relevant, inclusive, or \
            equitable stays a judgment for the nurse and the community. Also looks up Healthy \
            People 2030 objectives by topic, keyword, or ID with their national baseline, \
            target, desired direction, status, and data source (live from ODPHP's objective page \
            when reachable, else a bundled dated snapshot of about 500 objectives) and compares \
            your target with the national one only when the units match. Optional Word worksheet \
            to /data/output.",
        annotations(
            title = "Write SMARTIE objectives",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        output_schema = schema_for_output::<Envelope<SmartieResult>>()
    )]
    async fn write_smartie_objectives(
        &self,
        Parameters(args): Parameters<SmartieArgs>,
    ) -> CallToolResult {
        respond(phn_tools::planning::write_smartie_objectives(&self.ctx, args).await)
    }

    #[tool(
        name = "build_evaluation_plan",
        description = "Drafts a program evaluation plan that follows the six steps of CDC's \
            Program Evaluation Framework (2024): purpose, intended users and uses, evaluation \
            types (formative, process, outcome, impact, economic) chosen from the program's stage \
            or from the types you ask for, an evaluation matrix (focus area, question, \
            indicator, measure, baseline, data source, method, timing, responsible role), an \
            analysis plan, a use and dissemination plan, a dated or weekly timeline, and CDC's \
            cross-cutting actions and standards. Give the logic model (from build_logic_model) \
            to draft questions from its activities, outputs, and outcomes (CDC's own example \
            questions are used word for word and labeled), or give your own questions; anything \
            you leave out is filled with a suggestion marked as one. Optional RE-AIM lens (reach, \
            effectiveness, adoption, implementation, maintenance). Checks the plan for gaps \
            (no indicator, no data source, yes/no questions, no breakdown by group, impact \
            evaluation without a comparison). Writes a Word (or PDF) plan with blanks to /data/output; \
            an optional staff roster in /data adds staff names beside matching roles in that file \
            only.",
        annotations(
            title = "Build an evaluation plan",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<EvaluationPlanResult>>()
    )]
    async fn build_evaluation_plan(
        &self,
        Parameters(args): Parameters<EvaluationPlanArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::planning::build_evaluation_plan(ctx, args)
        })
        .await
    }
}
