//! Phase 10 tools: healthy aging and mental health screening.

use phn_core::Envelope;
use phn_tools::aging::{FallRiskArgs, FallRiskResult, ProtectiveArgs, ProtectiveResult};
use phn_tools::bh::{CrisisArgs, CrisisResult, ScreenArgs, ScreenResult};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking, respond};

#[tool_router(router = phase10_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "score_mental_health_screen",
        description = "Score a free mental health screening instrument from its item responses: \
            PHQ-2, PHQ-9, GAD-2, GAD-7, Edinburgh Postnatal Depression Scale (EPDS), NIMH Ask \
            Suicide-Screening Questions (ASQ), or the C-SSRS screener. Validates ranges, gives the \
            total and the published cutoff or risk level, and raises safety alerts (PHQ-9 item 9, \
            EPDS item 10, ASQ positive or acute positive, C-SSRS risk) with the required steps \
            (NIMH ASQ pathway and brief suicide safety assessment, 988, agency protocol). Never \
            diagnoses. Omit the instrument to list instruments, cutoffs, and license terms. With \
            file, scores a CSV/XLSX of screens in /data, writes per-row results to /data/output, \
            and returns only suppressed counts.",
        annotations(title = "Score a mental health screen", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<ScreenResult>>()
    )]
    async fn score_mental_health_screen(
        &self,
        Parameters(args): Parameters<ScreenArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::bh::score_mental_health_screen(ctx, args)
        })
        .await
    }

    #[tool(
        name = "find_crisis_resources",
        description = "Crisis resources for a Massachusetts town: 911 for immediate danger, the \
            988 Suicide and Crisis Lifeline (call, text, chat; Spanish, interpretation in 240+ \
            languages, ASL videophone, TTY, veterans), the Massachusetts Behavioral Health Help \
            Line (call or text, 200+ languages), Mobile Crisis Intervention for youth and adults \
            through Community Behavioral Health Centers, the Crisis Text Line, and the town's \
            local CBHC with address and phone. Says which line to use when and how to reach each \
            in a requested language.",
        annotations(title = "Find crisis resources", read_only_hint = true, idempotent_hint = true, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<CrisisResult>>()
    )]
    async fn find_crisis_resources(
        &self,
        Parameters(args): Parameters<CrisisArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::bh::find_crisis_resources(ctx, args)
        })
        .await
    }

    #[tool(
        name = "steadi_fall_risk_assessment",
        description = "CDC STEADI fall risk assessment for an older adult: scores the Stay \
            Independent checklist (12 items, at risk at 4 or more or any fall in the past year) or \
            the three key questions, interprets the Timed Up and Go (12 seconds or more), \
            30-Second Chair Stand (CDC below-average scores by age and sex), 4-Stage Balance Test \
            (tandem stand under 10 seconds), and orthostatic blood pressure, and builds an \
            intervention plan by risk factor plus home-safety fixes from CDC's Check for Safety \
            checklist. Can write the plan as DOCX or PDF to /data/output. No names needed.",
        annotations(title = "STEADI fall risk assessment", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<FallRiskResult>>()
    )]
    async fn steadi_fall_risk_assessment(
        &self,
        Parameters(args): Parameters<FallRiskArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::aging::steadi_fall_risk_assessment(ctx, args)
        })
        .await
    }

    #[tool(
        name = "elder_protective_services_guide",
        description = "Massachusetts reporting rules for abuse, neglect, financial exploitation, and \
            self-neglect of adults 60 and older (Adult Protective Services, M.G.L. c.19A; hotline \
            800-922-2275) and adults 18-59 with disabilities (DPPC, c.19C), and abuse by nursing \
            home or hospital staff (DPH): who must report, how and how fast (verbal now, written \
            within 48 hours), what a report contains, timelines, signs, self-neglect, and \
            protections. Returns a report-narrative template; with details_file (a CSV/XLSX in \
            /data) it fills the narratives and writes them to /data/output without returning the \
            details. Never pass names or addresses as arguments.",
        annotations(title = "Elder and disabled adult protective services guide", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<ProtectiveResult>>()
    )]
    async fn elder_protective_services_guide(
        &self,
        Parameters(args): Parameters<ProtectiveArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::aging::elder_protective_services_guide(ctx, args)
        })
        .await
    }
}
