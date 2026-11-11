//! Phase 14 tools (environmental health and climate). Vectors and housing: ENV-06 vector_borne_disease_risk, ENV-07 housing_conditions_health_guide.

use phn_core::Envelope;
use phn_tools::env::{HousingGuideArgs, HousingGuideResult, VectorRiskArgs, VectorRiskResult};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking, respond};

#[tool_router(router = phase14_vectorhousing_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "vector_borne_disease_risk",
        description = "Mosquito- and tick-borne disease risk for a Massachusetts town, county, or \
            the state (default: your saved town). Gives DPH's EEE and West Nile virus risk level \
            for each town (live from DPH's risk workbook during the June-October season; outside \
            the season, DPH's last levels with their date and a plain statement that they are not \
            current), what each level means, what residents can do, and what DPH's Arbovirus \
            Surveillance and Response Plan recommends at that level, positive mosquito, animal, \
            and human findings (human cases by county only), earlier seasons, DPH's recommended \
            cancellation times for evening activities at high EEE risk, tick-borne disease cases \
            and DPH incidence rates by county for Lyme disease, babesiosis, and anaplasmosis \
            (2022-2025, small numbers flagged), emergency department visits for tick-borne \
            disease, current-year NNDSS counts, and cited prevention messages for the public, \
            schools, recreational camps, and outdoor workers. Optional one-page DOCX notice to \
            /data/output.",
        annotations(
            title = "Mosquito- and tick-borne disease risk",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        output_schema = schema_for_output::<Envelope<VectorRiskResult>>()
    )]
    async fn vector_borne_disease_risk(
        &self,
        Parameters(args): Parameters<VectorRiskArgs>,
    ) -> CallToolResult {
        respond(phn_tools::env::vector_borne_disease_risk(&self.ctx, args).await)
    }

    #[tool(
        name = "housing_conditions_health_guide",
        description = "Explains housing conditions that affect health under the Massachusetts \
            State Sanitary Code, 105 CMR 410.000 (2023 revision): moisture and mold, pests, no \
            heat, hot water and utility shutoffs, lead paint, smoke and carbon monoxide alarms, \
            and trash. For each: the code sections and what they require, owner versus occupant \
            responsibilities, health effects and who is most at risk (CDC, EPA), and whether it \
            is a condition always deemed to endanger health. Also: how a tenant or someone on \
            their behalf gets a board of health inspection, inspection and correction timelines, \
            hearings, and tenant protections (retaliation, rent withholding, repair and deduct) \
            as decision support, not legal advice. Writes plain-language tenant teaching sheets \
            (DOCX or PDF, checked for reading grade) to /data/output, using only your agency's \
            approved translations from /data/templates/<language>/. Fills inspection request and \
            repair request letters from a household CSV/XLSX in /data into /data/output only; \
            nothing from that file is returned.",
        annotations(
            title = "Housing conditions and health guide",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<HousingGuideResult>>()
    )]
    async fn housing_conditions_health_guide(
        &self,
        Parameters(args): Parameters<HousingGuideArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::env::housing_conditions_health_guide(ctx, args)
        })
        .await
    }
}
