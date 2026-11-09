//! Phase 11 tools: substance use, crisis resources, and health communication.

use phn_core::Envelope;
use phn_tools::bh::{
    HarmReductionArgs, HarmReductionResult, OverdoseArgs, OverdoseResult, YouthSurveyArgs,
    YouthSurveyResult,
};
use phn_tools::comms::{ReadabilityArgs, ReadabilityResult};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking, respond};

#[tool_router(router = phase11_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "check_readability",
        description = "Check the reading level and plain language of a draft (pasted text or a \
            DOCX, HTML, PDF, TXT, or Markdown file in /data): Flesch Reading Ease, \
            Flesch-Kincaid, SMOG, and Gunning Fog grades against a target; long sentences, \
            jargon with CDC Everyday Words alternatives, possible passive voice, acronyms not \
            spelled out, and long paragraphs or lists; and the CDC Clear Communication Index \
            (scored from the nurse's yes/no answers, with the tool's suggestions). Use it before \
            publishing any material for the public.",
        annotations(title = "Check readability", read_only_hint = true, idempotent_hint = true, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<ReadabilityResult>>()
    )]
    async fn check_readability(
        &self,
        Parameters(args): Parameters<ReadabilityArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::comms::check_readability(ctx, args)
        })
        .await
    }

    #[tool(
        name = "get_overdose_data",
        description = "Overdose deaths for a Massachusetts city or town, county, or the state: \
            DPH's opioid-related overdose deaths by town of residence or occurrence for \
            2015-2022 with a 5-year rate compared with the state (no rate under 5 deaths); \
            CDC's provisional 12 month-ending drug overdose deaths for counties (counts of 1-9 \
            suppressed), Massachusetts, and the U.S. with the change from a year earlier and by \
            drug class (fentanyl, heroin, cocaine, stimulants); and DPH's statewide 2024 \
            findings. Says where newer town data are.",
        annotations(title = "Overdose death data", read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for_output::<Envelope<OverdoseResult>>()
    )]
    async fn get_overdose_data(
        &self,
        Parameters(args): Parameters<OverdoseArgs>,
    ) -> CallToolResult {
        respond(phn_tools::bh::get_overdose_data(&self.ctx, args).await)
    }

    #[tool(
        name = "get_youth_risk_behavior_data",
        description = "Massachusetts youth survey indicators for high or middle school students: \
            persistent sadness, suicidal ideation, self-injury, bullying, vaping, alcohol, \
            marijuana, protective factors, and other topics from DPH's Youth Health Survey \
            (2023, 2025) and the Massachusetts YRBS (2023), by sex, grade, race/ethnicity, and \
            LGBTQ identity with 95% confidence intervals and differences flagged; plus CDC YRBS \
            mental health estimates for Massachusetts compared with the U.S. (2019-2023).",
        annotations(title = "Youth risk behavior data", read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for_output::<Envelope<YouthSurveyResult>>()
    )]
    async fn get_youth_risk_behavior_data(
        &self,
        Parameters(args): Parameters<YouthSurveyArgs>,
    ) -> CallToolResult {
        respond(phn_tools::bh::get_youth_risk_behavior_data(&self.ctx, args).await)
    }

    #[tool(
        name = "find_naloxone_and_harm_reduction",
        description = "How to get naloxone in Massachusetts (pharmacy counter under the statewide \
            standing order with insurance including MassHealth, distribution programs, home \
            delivery, agency purchasing), the standing order's formulations and directions, \
            pharmacy duties and legal protections, whether a town's board of health approved a \
            syringe services program, fentanyl test strip facts, help lines, links to DPH's site \
            maps, and a cited naloxone teaching script, optionally as a Word handout in \
            /data/output.",
        annotations(title = "Naloxone and harm reduction", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<HarmReductionResult>>()
    )]
    async fn find_naloxone_and_harm_reduction(
        &self,
        Parameters(args): Parameters<HarmReductionArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::bh::find_naloxone_and_harm_reduction(ctx, args)
        })
        .await
    }
}
