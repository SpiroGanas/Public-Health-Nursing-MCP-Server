//! Phase 13 tools (emergency preparedness II): EPR-10 crisis_risk_communication_builder (CERC).

use phn_core::Envelope;
use phn_tools::epr::{CercArgs, CercResult};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

#[allow(unused_imports)]
use crate::server::{PhnServer, blocking, respond};

#[tool_router(router = phase13_cerc_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "crisis_risk_communication_builder",
        description = "Build crisis and emergency risk communication with CDC's CERC model for \
            a hazard and phase (preparation, initial, maintenance, resolution, or all): a message \
            map (3 key messages, 3 supporting facts each) checked against the 27 words / 9 \
            seconds / 3 messages rule, empathy, grade level, short sentences, absolutes, and \
            positive action steps; a holding statement (what we know, do not know yet, are \
            doing, next update); FAQs; spokesperson preparation (key point in 30 seconds and \
            under 90 words, bridges, reporters' likely questions, holding lines); audience \
            concerns; and the phase's CERC tasks. Pre-scripted sets: measles_exposure, \
            boil_water, extreme_heat, coastal_storm_power_outage, shelter_in_place, \
            respiratory_pandemic, mass_dispensing_pod; describe any other hazard in \
            hazard_description, or pass your own message_map or draft_text to check. Lists the \
            town's priority languages (ACS) and uses agency-approved translations in \
            /data/templates/<language>/cerc_<id>.md, otherwise writes translator briefs; never \
            machine-translates. export \"docx\" writes the packet to /data/output; the media \
            contact's phone and email come only from roster_path and appear only in the packet.",
        annotations(
            title = "Crisis and emergency risk communication (CERC)",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        output_schema = schema_for_output::<Envelope<CercResult>>()
    )]
    async fn crisis_risk_communication_builder(
        &self,
        Parameters(args): Parameters<CercArgs>,
    ) -> CallToolResult {
        respond(phn_tools::epr::crisis_risk_communication_builder(&self.ctx, args).await)
    }
}
