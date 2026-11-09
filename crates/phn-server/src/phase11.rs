//! Phase 11 tools: substance use, crisis resources, and health communication.

use phn_core::Envelope;
use phn_tools::bh::{
    HarmReductionArgs, HarmReductionResult, OverdoseArgs, OverdoseResult, PostventionArgs,
    PostventionResult, TreatmentArgs, TreatmentResult, YouthSurveyArgs, YouthSurveyResult,
};
use phn_tools::comms::{
    PressArgs, PressResult, ReadabilityArgs, ReadabilityResult, SocialCampaignArgs,
    SocialCampaignResult, TranslationArgs, TranslationResult,
};
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

    #[tool(
        name = "find_substance_use_treatment",
        description = "Find substance use treatment near a Massachusetts town from SAMHSA's \
            FindTreatment.gov (live, cached, or a bundled statewide copy): methadone at opioid \
            treatment programs, buprenorphine, naltrexone, withdrawal management, residential or \
            outpatient care, recovery supports, programs for adolescents, pregnant or postpartum \
            women, veterans, or co-occurring disorders, filtered by payment (MassHealth/Medicaid, \
            Medicare, private, sliding fee) and language, with the Massachusetts Substance Use \
            Helpline and 988.",
        annotations(title = "Find substance use treatment", read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for_output::<Envelope<TreatmentResult>>()
    )]
    async fn find_substance_use_treatment(
        &self,
        Parameters(args): Parameters<TreatmentArgs>,
    ) -> CallToolResult {
        respond(phn_tools::bh::find_substance_use_treatment(&self.ctx, args).await)
    }

    #[tool(
        name = "plan_school_postvention",
        description = "After a suicide affects a school: the AFSP/SPRC After a Suicide toolkit's \
            steps for the first 24 hours, first week, and after (with page numbers), talking \
            points, and message templates for the staff meeting, students, parents, the media, \
            and social media chosen by whether the cause of death is confirmed; a safe-messaging \
            check of a draft (method details, oversimplified causes, glamorizing, missing help \
            resources); and local fill of the templates from a file in /data into Word documents \
            in /data/output (names never returned). Never put names in arguments.",
        annotations(title = "Plan school postvention", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<PostventionResult>>()
    )]
    async fn plan_school_postvention(
        &self,
        Parameters(args): Parameters<PostventionArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::bh::plan_school_postvention(ctx, args)
        })
        .await
    }

    #[tool(
        name = "translate_health_material",
        description = "Translation workflow for a health material (no translations are bundled): \
            the town's language priorities (ACS limited English proficiency), agency translations \
            already in /data/templates/<language>/, a translator brief (numbered segments, the \
            agency glossary's approved terms, numbers, phones, and web addresses that must not \
            change), a check of a translated draft against the glossary, a back-translation \
            comparison (numbers, negations, glossary terms, missing sentences), CDC's translation \
            steps, the required 'draft for qualified translator review' label, and an optional \
            review worksheet (XLSX) in /data/output.",
        annotations(title = "Translate health material (workflow)", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = true),
        output_schema = schema_for_output::<Envelope<TranslationResult>>()
    )]
    async fn translate_health_material(
        &self,
        Parameters(args): Parameters<TranslationArgs>,
    ) -> CallToolResult {
        respond(phn_tools::comms::translate_health_material(&self.ctx, args).await)
    }

    #[tool(
        name = "create_social_media_campaign",
        description = "Plan a social media campaign on a health topic: a posting calendar for \
            the chosen platforms tied to related national health observances (bundled list or \
            your own calendar), and checks of draft posts written by the assistant or the nurse: \
            platform character limits (overridable), alt text, CamelCase hashtags, emoji, all \
            capitals, reading grade and jargon (as in check_readability), and safe messaging for \
            suicide topics; with an optional XLSX calendar in /data/output.",
        annotations(title = "Create a social media campaign", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<SocialCampaignResult>>()
    )]
    async fn create_social_media_campaign(
        &self,
        Parameters(args): Parameters<SocialCampaignArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::comms::create_social_media_campaign(ctx, args)
        })
        .await
    }

    #[tool(
        name = "draft_press_materials",
        description = "Draft and check a press release, media advisory, talking points, or \
            spokesperson Q&A from the facts given (who, what, when, where, why, how; quotes; \
            media contact; embargo), with missing parts marked, checks from CDC's CERC media \
            guidance (contact block, date, release time, 5 Ws, one page, short sentences, no \
            loaded words, privacy), reading grade, safe messaging for suicide topics, CERC \
            principles and questions to prepare for, and an optional Word document in \
            /data/output.",
        annotations(title = "Draft press materials", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<PressResult>>()
    )]
    async fn draft_press_materials(
        &self,
        Parameters(args): Parameters<PressArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::comms::draft_press_materials(ctx, args)
        })
        .await
    }
}
