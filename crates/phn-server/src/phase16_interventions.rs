//! Phase 16 tools (Interventions, theory, performance tracking): PLAN-03, PLAN-04, PLAN-11.

use phn_core::Envelope;
use phn_tools::planning::{
    EvidenceArgs, EvidenceResult, PerformanceArgs, PerformanceResult, TheoryArgs, TheoryResult,
};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking, respond};

// Phase 16 interventions: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase16_interventions_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "find_evidence_based_interventions",
        description = "Finds Community Guide evidence for a health topic: systematic reviews of \
            effectiveness, systematic economic reviews, and Community Preventive Services Task \
            Force recommendation statements, from the journal papers PubMed attributes to the \
            Task Force (a dated snapshot, with an optional live PubMed search for newer papers). \
            For each paper: title, journal, publication date, kind of paper, the evidence wording \
            its abstract uses (strong evidence, sufficient evidence, insufficient evidence, the \
            Task Force recommends or recommends against; never inferred, and a plain note when the \
            record gives no rating), short quotations that carry that wording, a short excerpt of \
            the findings and conclusion, any implementation, barrier, or cost sentence in the \
            abstract, and links. Give a topic (words like smoking and tobacco are both \
            recognized), and optionally a population, a setting such as school or worksite, kinds \
            of paper, and an evidence filter. This is NOT the Community Guide's own recommendation \
            list, and What Works for Health (County Health Rankings) is not included because its \
            site could not be reached: the result says what to check on each site. Inputs are \
            topic words only; no local files are read.",
        annotations(
            title = "Find evidence-based interventions",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for_output::<Envelope<EvidenceResult>>()
    )]
    async fn find_evidence_based_interventions(
        &self,
        Parameters(args): Parameters<EvidenceArgs>,
    ) -> CallToolResult {
        respond(phn_tools::planning::find_evidence_based_interventions(&self.ctx, args).await)
    }

    #[tool(
        name = "apply_behavior_change_theory",
        description = "Maps a program's problem, population, setting, observed barriers, and \
            planned components onto the constructs of health behavior and behavior change \
            theories: the Health Belief Model, Social Cognitive Theory, Theory of Planned \
            Behavior, Transtheoretical Model (stages of change), Socio-Ecological Model, \
            Diffusion of Innovations, COM-B and the Behaviour Change Wheel, and Self-Determination \
            Theory. For each construct: what it means, intervention strategies that target it \
            (marked as taken from a published table or as this tool's own suggestion), and \
            measure ideas (not validated instruments). Your barriers are matched to constructs by \
            whole-word signals, and your planned components to strategies that share a word; \
            matches are leads, and unmatched items are flagged for your judgment. The tool picks \
            up to three theories whose constructs match your words (and adds the \
            Socio-Ecological Model when none looks beyond the individual), or use the theories \
            you name. Constructs were paraphrased from open-access reviews and a CDC page that \
            were read; the original theory papers were not. Describe a population, never a \
            person. Optional Word or PDF worksheet to /data/output.",
        annotations(
            title = "Apply behavior change theory",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<TheoryResult>>()
    )]
    async fn apply_behavior_change_theory(
        &self,
        Parameters(args): Parameters<TheoryArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::planning::apply_behavior_change_theory(ctx, args)
        })
        .await
    }

    #[tool(
        name = "track_program_performance",
        description = "Tracks a program's results against targets from a local CSV or XLSX in \
            /data with outputs and outcomes by period (one row per period, one row per period \
            and measure, or one row per record to count by period). You give the period column, \
            the measures, and each target (a count or a percentage with a denominator, higher or \
            lower is better, cumulative or each period, optional baseline). Returns, per measure, \
            the period values, the cumulative total or pooled percentage, progress to target, \
            pace against the program year (on track, behind, met), and run-chart or p-chart trend \
            flags (an unusually long run, a shift, a point outside the limits). Only aggregate \
            counts by period are returned: small counts (1 to the suppression threshold minus 1) \
            are hidden, and when one is hidden the totals, progress, and center line that would \
            reveal it are withheld. Writes a dashboard report (Word or PDF), a workbook with live \
            formulas (Summary and Data sheets, so you can add next quarter's row), and a chart \
            per measure to /data/output with the full values from your own file. Only the columns \
            you name are read; identifier columns are refused; other columns, such as contact \
            people, are never read or returned.",
        annotations(
            title = "Track program performance",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<PerformanceResult>>()
    )]
    async fn track_program_performance(
        &self,
        Parameters(args): Parameters<PerformanceArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::planning::track_program_performance(ctx, args)
        })
        .await
    }
}
