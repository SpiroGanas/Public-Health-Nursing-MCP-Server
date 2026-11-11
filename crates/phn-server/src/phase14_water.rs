//! Phase 14 tools (environmental health and climate). Water: ENV-03 drinking_water_quality_lookup, CHR-07 community_water_fluoridation_status.

#[allow(unused_imports)]
use phn_core::Envelope;
use phn_tools::chronic::{FluoridationArgs, FluoridationResult};
use phn_tools::env::{DrinkingWaterArgs, DrinkingWaterResult};
#[allow(unused_imports)]
use rmcp::handler::server::common::schema_for_output;
#[allow(unused_imports)]
use rmcp::handler::server::wrapper::Parameters;
#[allow(unused_imports)]
use rmcp::model::CallToolResult;
#[allow(unused_imports)]
use rmcp::{tool, tool_router};

#[allow(unused_imports)]
use crate::server::{PhnServer, blocking, respond};

#[tool_router(router = phase14_water_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "drinking_water_quality_lookup",
        description = "Public drinking water quality for a Massachusetts city or town (default: \
            the saved town), or one public water system by PWSID or name: the community water \
            systems listed under the town (population served, ground or surface water, purchased \
            water), health-based violations in a recent window (years, default 5), the latest \
            lead and copper 90th percentiles against the action levels, and MassDEP PFAS6 results \
            for treated water against the Massachusetts standard of 20 ng/L, plus what residents \
            can do. Live from EPA SDWIS (Envirofacts) and the EEA Data Portal, with a dated \
            snapshot when they cannot be reached (it says so). Adds MassDEP private well testing \
            guidance (what to test, how often, certified labs, the board of health's role, \
            arsenic and uranium towns) when part of the town appears to use private wells, or \
            when asked. Optionally lists non-community systems such as schools and child care \
            with their own wells. Also states the current EPA PFAS rule changes.",
        annotations(
            title = "Drinking water quality lookup",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for_output::<Envelope<DrinkingWaterResult>>()
    )]
    async fn drinking_water_quality_lookup(
        &self,
        Parameters(args): Parameters<DrinkingWaterArgs>,
    ) -> CallToolResult {
        respond(phn_tools::env::drinking_water_quality_lookup(&self.ctx, args).await)
    }

    #[tool(
        name = "community_water_fluoridation_status",
        description = "Community water fluoridation in Massachusetts: a town's status on DPH's \
            2023 list (fully, partially, or not fluoridated; MWRA communities) and the community \
            water systems behind it from CDC My Water's Fluoride (adjusted, consecutive, natural, \
            or non-adjusted, population, reported average fluoride level), with discrepancies \
            and suspensions flagged; a statewide summary (share of people on community systems \
            receiving fluoridated water, towns by status); each body's current position side by \
            side with its date (DPH, CDC, CPSTF, EPA, FDA, AAP), evidence reviews (NTP 2024, JAMA \
            Pediatrics 2025, Cochrane 2024, CDC), where they differ, M.G.L. c. 111, section 8C, \
            and draft discussion points for a board of health. Never picks a side. Bundled data; \
            optional DOCX brief to /data/output.",
        annotations(
            title = "Community water fluoridation status",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<FluoridationResult>>()
    )]
    async fn community_water_fluoridation_status(
        &self,
        Parameters(args): Parameters<FluoridationArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::chronic::community_water_fluoridation_status(ctx, args)
        })
        .await
    }
}
