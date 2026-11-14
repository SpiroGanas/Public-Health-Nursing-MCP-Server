//! Phase 15 tools (Preventive services and asthma): CHR-04, CHR-05.

use phn_core::Envelope;
use phn_tools::chronic::{AsthmaHomeArgs, AsthmaHomeResult, PreventiveArgs, PreventiveResult};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking, respond};

// Phase 15 prevent: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase15_prevent_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "preventive_services_lookup",
        description = "Lists the U.S. Preventive Services Task Force (USPSTF) grade A and B \
            recommendations that fit a person's age, sex at birth, pregnancy status (pregnant, \
            postpartum, or not), and risk factors you give (tobacco use and pack-years, BMI or \
            BMI percentile, cardiovascular risk factors and 10-year risk, sexually active, \
            postmenopausal, fair skin, low-fluoride home water, and areas where the clinician \
            judges increased risk such as HIV, hepatitis B, TB, falls, preeclampsia). Each result \
            shows the grade, the date of the Task Force statement, its own population and \
            recommendation wording, what the tool could not check for you to apply, a \
            plain-language client summary, and a link to the statement. A recommendation that \
            depends on a risk factor you did not give is listed under 'needs review', never \
            dropped. Also gives Massachusetts options where DPH pages were read: the \
            Massachusetts Breast and Cervical Cancer Program (free screening for eligible \
            uninsured or underinsured residents), the MA Colorectal Cancer Control Program, and \
            1-800-QUIT-NOW. When online, adds ODPHP MyHealthfinder's list for the same age, sex, \
            and pregnancy (it also covers vaccines and other non-USPSTF services). The A and B \
            list is not the full list of preventive services (not ACIP vaccines, not grade C). \
            Takes no names or identifiers. Optional plain-language handout (DOCX or PDF, reading \
            grade checked) to /data/output.",
        annotations(
            title = "USPSTF preventive services lookup",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        output_schema = schema_for_output::<Envelope<PreventiveResult>>()
    )]
    async fn preventive_services_lookup(
        &self,
        Parameters(args): Parameters<PreventiveArgs>,
    ) -> CallToolResult {
        respond(phn_tools::chronic::preventive_services_lookup(&self.ctx, args).await)
    }

    #[tool(
        name = "asthma_home_assessment",
        description = "Home-based asthma assessment for a home visit. Gives the CDC, EPA, and HUD \
            home asthma trigger checklist for home visitors (core assessment on heating, \
            cooking, smoking, pets, pests, moisture and mold, plus dust mite and mold and \
            moisture modules, each answer with low-cost action steps), pointing at the 105 CMR \
            410 housing code sections already in housing_conditions_health_guide rather than \
            repeating them; asthma control categories from DPH's table (adapted from NHLBI \
            EPR-3, screening level), and it classifies the counts you give (symptom days, \
            nights waking, rescue inhaler use, activity limits) by the most severe item; the \
            NHLBI asthma action plan template (green, yellow, red zones) with peak flow zones \
            worked out from a best peak flow; and Massachusetts home-based asthma services \
            from DPH's Asthma Home Visiting Program Directory (dated, listing regions served, \
            not towns), marking programs that name or are located in your town. The Asthma \
            Control Test is licensed and is not reproduced. Writes the blank checklist and blank \
            action plan (DOCX or PDF) to /data/output. Fills asthma action plans for the people \
            in a CSV/XLSX in /data into /data/output only; nothing from that file is returned, \
            and medicines and doses appear only if they are in your file.",
        annotations(
            title = "Asthma home assessment",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<AsthmaHomeResult>>()
    )]
    async fn asthma_home_assessment(
        &self,
        Parameters(args): Parameters<AsthmaHomeArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::chronic::asthma_home_assessment(ctx, args)
        })
        .await
    }
}
