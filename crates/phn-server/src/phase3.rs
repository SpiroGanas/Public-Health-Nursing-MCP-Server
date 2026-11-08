//! Phase 3 tools: communicable disease I.

use phn_core::Envelope;
use phn_tools::cd::{
    AberrationArgs, AberrationResult, ContactTracingArgs, ContactTracingResult, EpiCurveArgs,
    EpiCurveResult, GuideSearchArgs, GuideSearchResult, InfectionControlArgs,
    InfectionControlResult, IsolationArgs, IsolationResult, MeaslesExposureArgs,
    MeaslesExposureResult, OutbreakArgs, OutbreakResult,
};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking, respond};

#[tool_router(router = phase3_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "search_ma_disease_control_guide",
        description = "Search Massachusetts DPH's Guide to Surveillance, Reporting and Control \
            (disease chapters) and DPH disease fact sheets offline. Returns quoted passages with \
            the chapter section, document date, and mass.gov URL. Filter by disease or document \
            kind; with only a disease, returns the chapter outline; list_documents shows the corpus.",
        annotations(title = "Search the DPH disease control guide", read_only_hint = true, idempotent_hint = true, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<GuideSearchResult>>()
    )]
    async fn search_ma_disease_control_guide(
        &self,
        Parameters(args): Parameters<GuideSearchArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::cd::search_ma_disease_control_guide(ctx, args)
        })
        .await
    }

    #[tool(
        name = "calculate_isolation_and_exclusion",
        description = "Calculate isolation, quarantine, and exclusion dates for a case or a \
            contact under 105 CMR 300.200 and DPH guidance, by setting (child care, school, food \
            handler, health care worker, other), from onset, specimen, exposure, and treatment \
            dates. Shows each rule's wording, citation, confidence, day counting, and rules that \
            depend on unanswered facts (susceptible, outbreak, diarrhea).",
        annotations(title = "Isolation and exclusion dates", read_only_hint = true, idempotent_hint = true, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<IsolationResult>>()
    )]
    async fn calculate_isolation_and_exclusion(
        &self,
        Parameters(args): Parameters<IsolationArgs>,
    ) -> CallToolResult {
        respond(phn_tools::cd::calculate_isolation_and_exclusion(
            &self.ctx, args,
        ))
    }

    #[tool(
        name = "analyze_outbreak_line_list",
        description = "Analyze a local outbreak line list (CSV/XLSX in /data): finds onset, \
            case status, age, sex, town, and yes/no exposure columns (overridable), applies the \
            case definition, and returns person/place/time tables with small-cell suppression, \
            food- or exposure-specific attack rates with risk ratios, odds ratios, CIs and Fisher \
            p-values, likely vehicles, and the probable exposure window. Aggregates only.",
        annotations(title = "Analyze an outbreak line list", read_only_hint = true, idempotent_hint = true, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<OutbreakResult>>()
    )]
    async fn analyze_outbreak_line_list(
        &self,
        Parameters(args): Parameters<OutbreakArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::cd::analyze_outbreak_line_list(ctx, args)
        })
        .await
    }

    #[tool(
        name = "generate_epi_curve",
        description = "Draw an epidemic curve (PNG/SVG in /data/output/charts) from a local line \
            list or inline onset counts, with an interval of about 1/4 of the incubation period, \
            optional strata (up to 8), and exposure and incubation markers. Describes the likely \
            pattern (point source, continuous or intermittent common source, propagated) with \
            reasoning and uncertainty. Returned counts are suppressed; the saved chart is exact.",
        annotations(title = "Generate an epidemic curve", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<EpiCurveResult>>()
    )]
    async fn generate_epi_curve(
        &self,
        Parameters(args): Parameters<EpiCurveArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::cd::generate_epi_curve(ctx, args)
        })
        .await
    }

    #[tool(
        name = "plan_contact_tracing",
        description = "Plan contact tracing for an index case: infectious period, the window to \
            ask about contacts, per-contact symptom-watch, monitoring, PEP, and exclusion dates, \
            and a blank XLSX contact-tracking template (with date formulas) written to \
            /data/output. Never put contact names in the chat; fill the template locally.",
        annotations(title = "Plan contact tracing", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<ContactTracingResult>>()
    )]
    async fn plan_contact_tracing(
        &self,
        Parameters(args): Parameters<ContactTracingArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::cd::plan_contact_tracing(ctx, args)
        })
        .await
    }

    #[tool(
        name = "assess_measles_exposure",
        description = "Assess one person's measles exposure (no identifiers needed): evidence of \
            immunity for their group (ACIP and DPH), MMR (72 hours) and immune globulin (6 days) \
            deadlines with IG dose by weight, exclusion from day 5 through day 21 under 105 CMR \
            300.200 (including health care and continuous exposure), priority groups, and the \
            symptom-monitoring end date.",
        annotations(title = "Assess a measles exposure", read_only_hint = true, idempotent_hint = true, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<MeaslesExposureResult>>()
    )]
    async fn assess_measles_exposure(
        &self,
        Parameters(args): Parameters<MeaslesExposureArgs>,
    ) -> CallToolResult {
        respond(phn_tools::cd::assess_measles_exposure(&self.ctx, args))
    }

    #[tool(
        name = "detect_aberrations",
        description = "Detect unusual increases in a case or syndrome count series with CDC EARS \
            C1, C2, C3, a Poisson CUSUM, and the Farrington-type (Noufaily 2013) seasonal \
            algorithm. Give daily, weekly, or monthly counts; returns signals by period, \
            thresholds, an interpretation, and the trade-offs between methods; optional chart.",
        annotations(title = "Detect aberrations", read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<AberrationResult>>()
    )]
    async fn detect_aberrations(
        &self,
        Parameters(args): Parameters<AberrationArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::cd::detect_aberrations(ctx, args)
        })
        .await
    }

    #[tool(
        name = "infection_control_for_congregate_settings",
        description = "Infection control for a pathogen in shelters, schools, child care, \
            long-term care, and day programs: transmission-based precautions and their duration \
            (CDC/HICPAC Appendix A), PPE, cohorting, hand hygiene, DPH setting notes, and which \
            EPA list of registered disinfectants (List G, K, N, …) to use, with links.",
        annotations(title = "Infection control for congregate settings", read_only_hint = true, idempotent_hint = true, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<InfectionControlResult>>()
    )]
    async fn infection_control_for_congregate_settings(
        &self,
        Parameters(args): Parameters<InfectionControlArgs>,
    ) -> CallToolResult {
        respond(phn_tools::cd::infection_control_for_congregate_settings(
            &self.ctx, args,
        ))
    }
}
