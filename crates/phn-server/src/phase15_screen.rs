//! Phase 15 tools (BP screening and diabetes prevention): CHR-02, CHR-03.

use crate::server::{PhnServer, blocking};
use phn_core::Envelope;
use phn_tools::chronic::{
    BpToolkitArgs, BpToolkitResult, DiabetesPreventionArgs, DiabetesPreventionResult,
};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

// Phase 15 screen: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase15_screen_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "community_bp_screening_toolkit",
        description = "Community blood pressure screening kit for adults. Gives the measurement \
            technique checklist (CDC's steps, with any step not verified at a source marked), \
            the current ACC/AHA categories (the 2025 AHA/ACC guideline retired and replaced the \
            2017 one: normal, elevated, stage 1, stage 2, and the severe range above 180 or above \
            120, with the sources' own wording, a confidence level for each, and CDC's pregnancy \
            thresholds of 140/90 and 160/110), referral tiers that are this project's defaults \
            labelled 'tool default, pending review' (the guideline sources give no community \
            screening referral timing, so follow the agency's standing orders), a screening log \
            template (XLSX and CSV), follow-up letter templates, a reference card, and a \
            plain-language client handout checked for reading grade, all written to \
            /data/output. Classifies readings typed in (numbers only, averaged, any severe \
            reading flagged). A screening log CSV/XLSX in /data is summarized only as suppressed \
            counts by blood pressure category, age group, referral status, and number of \
            readings; follow-up letters are filled from its rows into /data/output only, and \
            nothing from the file (names, addresses, readings) is returned. Decision support, \
            not medical advice.",
        annotations(
            title = "Community blood pressure screening toolkit",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<BpToolkitResult>>()
    )]
    async fn community_bp_screening_toolkit(
        &self,
        Parameters(args): Parameters<BpToolkitArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::chronic::community_bp_screening_toolkit(ctx, args)
        })
        .await
    }

    #[tool(
        name = "diabetes_prevention_finder",
        description = "Scores the CDC Prediabetes Risk Test (CDC's published points: age, sex, \
            gestational diabetes, family history, high blood pressure, physical activity, BMI \
            from height and weight or given, with the lower BMI cut point for Asian American \
            people; 5 or higher of 10 is high risk), checks CDC's eligibility rules for the \
            National Diabetes Prevention Program lifestyle change program, and lists the \
            Massachusetts program sites on the Department of Public Health's list for a town \
            (and its county), a county, or the state. Honest limits: the list is DPH's, \
            undated, and not CDC's registry (which could not be reached), so recognition status \
            and delivery mode are not stated and no online providers are listed; the tool says \
            how to find them on CDC's page. Also gives CDC's delivery modes and the Medicare \
            Diabetes Prevention Program facts from CMS (no MassHealth coverage is stated: no \
            source was found). Optional plain-language client handout (reading grade checked) to \
            /data/output. A screening-event file in /data is summarized only as suppressed \
            counts by result and age group. Decision support, not a diagnosis.",
        annotations(
            title = "Diabetes prevention finder",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<DiabetesPreventionResult>>()
    )]
    async fn diabetes_prevention_finder(
        &self,
        Parameters(args): Parameters<DiabetesPreventionArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::chronic::diabetes_prevention_finder(ctx, args)
        })
        .await
    }
}
