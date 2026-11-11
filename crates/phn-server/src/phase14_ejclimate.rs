//! Phase 14 tools (environmental health and climate). Environmental justice and climate: ENV-04 environmental_justice_screen, ENV-05 climate_health_vulnerability_profile.

use phn_core::Envelope;
use phn_tools::env::{ClimateProfileArgs, ClimateProfileResult, EjScreenArgs, EjScreenResult};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking, respond};

#[tool_router(router = phase14_ejclimate_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "environmental_justice_screen",
        description = "Screen a Massachusetts city or town, county, or the whole state for \
            environmental justice (EJ) populations: the 2020 EJ block groups (EEA layer, updated \
            2022) by criterion (minority, income, English isolation) and combination, the people \
            living in them and their share of residents (EEA's municipal statistics for a town), \
            English-isolation block groups and the largest limited-English language groups \
            (language access), Social Vulnerability Index context, towns ranked by EJ population \
            for a county or the state, what a designation changes (MEPA 1- and 5-mile review \
            areas, EIR, translation and interpreter rules, EEA policy), and DPH's four vulnerable \
            health EJ criteria. Works offline. EPA EJScreen is archived (removed in 2025): give \
            ejscreen_path, an archived EJScreen 2.3 block group CSV in /data, to add \
            population-weighted environmental indicator percentiles (EJ vs other block groups); \
            only averages and counts are returned. Optional map (SVG/PNG) and block group table \
            (XLSX) go to /data/output. The CDC Environmental Justice Index is not bundled.",
        annotations(
            title = "Environmental justice screen",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<EjScreenResult>>()
    )]
    async fn environmental_justice_screen(
        &self,
        Parameters(args): Parameters<EjScreenArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::env::environmental_justice_screen(ctx, args)
        })
        .await
    }

    #[tool(
        name = "climate_health_vulnerability_profile",
        description = "Build a climate and health vulnerability profile of one Massachusetts \
            city or town for Municipal Vulnerability Preparedness (MVP) and hazard mitigation \
            planning, every value with a Massachusetts comparison, source, and vintage: extreme \
            heat days, tree canopy, and impervious surface (CDC/ATSDR Heat and Health Index, \
            ZIP areas weighted to the town), NFIP flood insurance claims history and NFIP and \
            Community Rating System status, environmental justice and SVI social vulnerability, \
            older adults, young children, disability, poverty, no vehicle, limited English \
            (ACS), climate-sensitive conditions such as asthma, COPD, heart disease, and \
            diabetes (CDC PLACES), electricity-dependent Medicare beneficiaries (HHS emPOWER), \
            the town's MVP region and planning reports, and FEMA hazard mitigation plan status \
            and expiration (live from OpenFEMA, with a bundled fallback). Lists the most \
            important vulnerabilities by transparent tool-default rules, with heat and flood \
            health effects. Optional Word (DOCX) profile to /data/output.",
        annotations(
            title = "Climate and health vulnerability profile",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        output_schema = schema_for_output::<Envelope<ClimateProfileResult>>()
    )]
    async fn climate_health_vulnerability_profile(
        &self,
        Parameters(args): Parameters<ClimateProfileArgs>,
    ) -> CallToolResult {
        respond(phn_tools::env::climate_health_vulnerability_profile(&self.ctx, args).await)
    }
}
