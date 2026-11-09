//! Phase 9 tools: PHN practice toolkit and case management.

use phn_core::Envelope;
use phn_tools::cd::{LtbiCalendar, LtbiCalendarArgs, TbContactArgs, TbContactPlan};
use phn_tools::core::{
    DocTemplateArgs, DocTemplateResult, OmahaArgs, OmahaResult, WheelArgs, WheelResult,
};
use phn_tools::equity::{
    HrsnArgs, HrsnResult, NewcomerArgs, NewcomerResult, ResourceSearchArgs, ResourceSearchResult,
};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking, respond};

#[tool_router(router = phase9_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "plan_tb_contact_investigation",
        description = "Plan a tuberculosis contact investigation with the 2005 NTCA/CDC guidelines: \
            whether to investigate, the infectious period (3 months before symptoms or first \
            finding, or 4 weeks before diagnosis), contact priority (high, medium, low) from the \
            published decision trees, initial-encounter and evaluation deadlines in business days, \
            and the retest 8-10 weeks after last exposure, plus DPH TB program contacts, reporting \
            rules, and state TB clinics. Give contact groups as counts and risk facts (never \
            names). Writes an XLSX tracking workbook to /data/output; with contact_file it fills \
            the workbook from a local list and returns only suppressed counts and expansion signals.",
        annotations(title = "Plan a TB contact investigation", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<TbContactPlan>>()
    )]
    async fn plan_tb_contact_investigation(
        &self,
        Parameters(args): Parameters<TbContactArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::cd::plan_tb_contact_investigation(ctx, args)
        })
        .await
    }

    #[tool(
        name = "ltbi_treatment_calendar",
        description = "Build a dose calendar and adherence tracker for a latent TB infection \
            regimen (3HP weekly x12, 4R daily x120, 3HR daily x90, 6H/9H daily, or twice-weekly \
            6H/9H by DOT): every dose date, monitoring visits (DPH's week 1/5/9 schedule for 3HP, \
            monthly otherwise), the completion window (e.g., 11 of 12 3HP doses within 16 weeks, \
            120 4R doses within 6 months), and, from dose dates taken, doses counted, gaps over 2 \
            months, and whether completion is still possible. Shows CDC/NTCA 2020 and DPH \
            positions. Writes an XLSX tracker and a DOCX patient calendar (agency translations from \
            /data/templates/<language>/). Does not calculate doses.",
        annotations(title = "LTBI treatment calendar", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<LtbiCalendar>>()
    )]
    async fn ltbi_treatment_calendar(
        &self,
        Parameters(args): Parameters<LtbiCalendarArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::cd::ltbi_treatment_calendar(ctx, args)
        })
        .await
    }

    #[tool(
        name = "screen_health_related_social_needs",
        description = "Score the CMS Accountable Health Communities health-related social needs \
            (HRSN) screening tool: living situation, food, transportation, utilities, and \
            interpersonal safety (HITS score 11 or more), plus supplemental domains (financial \
            strain, employment, family support, education, physical activity, substance use, \
            PHQ-2 and stress, disabilities). Positive domains map to Massachusetts referral \
            categories (EA shelter, Mass 211, SNAP, WIC, HEAP, MassHealth ACO HRSN services). Give \
            answer codes only (no names). With path, scores a local file of many screenings, \
            writes per-person results to /data/output, and returns suppressed counts by domain. \
            Call with no answers to list item ids and answer keys.",
        annotations(title = "Screen health-related social needs", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<HrsnResult>>()
    )]
    async fn screen_health_related_social_needs(
        &self,
        Parameters(args): Parameters<HrsnArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::equity::screen_health_related_social_needs(ctx, args)
        })
        .await
    }

    #[tool(
        name = "omaha_system_coder",
        description = "Code a home visit narrative or care plan with the Omaha System (public \
            domain): suggested problems (42 problems in 4 domains) with codes, modifiers, and \
            signs/symptoms; interventions as category (Teaching, Guidance, and Counseling; \
            Treatments and Procedures; Case Management; Surveillance) plus target; and the \
            Knowledge-Behavior-Status rating scale. The narrative (text, or a note file in /data \
            via path) is analyzed locally and never returned: only codes and matched vocabulary \
            terms. Also looks up a problem or target, labels KBS ratings with change from \
            baseline, and can write a blank care plan (DOCX) to /data/output.",
        annotations(title = "Omaha System coder", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<OmahaResult>>()
    )]
    async fn omaha_system_coder(&self, Parameters(args): Parameters<OmahaArgs>) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::core::omaha_system_coder(ctx, args)
        })
        .await
    }

    #[tool(
        name = "classify_intervention_wheel",
        description = "Classify public health nursing activities by the Public Health \
            Intervention Wheel (Minnesota Department of Health): 17 interventions in 5 wedges \
            (surveillance, disease investigation, outreach, screening, case-finding; referral and \
            follow-up, case management, delegated functions; health teaching, counseling, \
            consultation; collaboration, coalition building, community organizing; advocacy, \
            social marketing, policy development and enforcement) and 3 levels of practice \
            (individual/family, community, systems), with definitions. Classify a short list of \
            activities (returned by number only) or an activity log in /data (results written to \
            /data/output; counts and hours returned for workload reports). With no input, lists \
            the Wheel.",
        annotations(title = "Classify by the Intervention Wheel", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<WheelResult>>()
    )]
    async fn classify_intervention_wheel(
        &self,
        Parameters(args): Parameters<WheelArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::core::classify_intervention_wheel(ctx, args)
        })
        .await
    }

    #[tool(
        name = "nursing_documentation_template",
        description = "Documentation templates for public health nursing: SOAP, DAR (focus \
            charting), or narrative, for a home visit, clinic visit, communicable disease case \
            investigation, or DOT visit, with section prompts and the required elements (date and \
            time, signature with credentials per 244 CMR 9.03(44), consent, language and \
            interpreter, assessment, interventions, teaching with teach-back, response, \
            follow-up, plus visit-specific items such as control measures or dose observed). \
            With check_path, checks a note file in /data for those elements and returns only \
            found/missing results, never the note's text. Can write a blank template (DOCX, MD).",
        annotations(title = "Nursing documentation template", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<DocTemplateResult>>()
    )]
    async fn nursing_documentation_template(
        &self,
        Parameters(args): Parameters<DocTemplateArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::core::nursing_documentation_template(ctx, args)
        })
        .await
    }

    #[tool(
        name = "find_community_resources",
        description = "Find community resources near a Massachusetts town: searches the nurse's own \
            resource directory (CSV/XLSX in /data, default resources/community_resources.csv) by \
            service (food, housing, utilities, transportation, primary care, dental, behavioral \
            health, legal/immigration, ...), language, and opening day and time, plus HRSA-funded \
            health center sites from data.hrsa.gov (live, cached, or bundled snapshot). Results \
            are ranked same town first, then same county, with stale directory entries flagged. \
            Files that look like client lists are refused.",
        annotations(title = "Find community resources", read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for_output::<Envelope<ResourceSearchResult>>()
    )]
    async fn find_community_resources(
        &self,
        Parameters(args): Parameters<ResourceSearchArgs>,
    ) -> CallToolResult {
        respond(phn_tools::equity::find_community_resources(&self.ctx, args).await)
    }

    #[tool(
        name = "newcomer_health_navigator",
        description = "Refugee and immigrant (newcomer) health in Massachusetts: CDC domestic \
            refugee screening components (history and physical, TB, immunizations, CBC, lead, \
            hepatitis, HIV, parasites, malaria, mental health, nutrition, reproductive health, \
            cancer) with age rules and the 90-day window from arrival; DPH and Office for Refugees \
            and Immigrants programs (RHAP, RHPP, Refugee Medical Assistance); MassHealth coverage \
            by immigration status, including the October 1, 2026 federal changes for refugees, \
            asylees, and others; and Emergency Assistance family shelter and basic-needs \
            resources. Uses status and age only (no identifiers); can write a blank screening \
            checklist (agency translations from /data/templates/<language>/).",
        annotations(title = "Newcomer health navigator", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<NewcomerResult>>()
    )]
    async fn newcomer_health_navigator(
        &self,
        Parameters(args): Parameters<NewcomerArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::equity::newcomer_health_navigator(ctx, args)
        })
        .await
    }
}
