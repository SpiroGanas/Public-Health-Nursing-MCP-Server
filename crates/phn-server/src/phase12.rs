//! Phase 12 tools: emergency preparedness I: situational awareness and planning.

use phn_core::Envelope;
use phn_tools::epr::{DispensingArgs, DispensingResult, HazardAlertsArgs, HazardAlertsResult};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

#[allow(unused_imports)]
use crate::server::{PhnServer, blocking, respond};

#[tool_router(router = phase12_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "get_active_hazard_alerts",
        description = "Current National Weather Service watches, warnings, and advisories for a \
            Massachusetts town, county, or the whole state (live from api.weather.gov), with the \
            likely health effects, who is most at risk, suggested public health actions, and \
            public messages for each hazard (heat, cold, winter storm, flood, coastal flood, \
            hurricane, severe storm, tornado, smoke, hazmat), plus power outage and carbon \
            monoxide follow-on risks. Also lists FEMA disaster declarations covering the area \
            (OpenFEMA) with counts by incident type since 1953. Ask for guidance_for to plan \
            for a hazard with no active alert. Says plainly when live alerts cannot be reached.",
        annotations(
            title = "Active hazard alerts and health actions",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for_output::<Envelope<HazardAlertsResult>>()
    )]
    async fn get_active_hazard_alerts(
        &self,
        Parameters(args): Parameters<HazardAlertsArgs>,
    ) -> CallToolResult {
        respond(phn_tools::epr::get_active_hazard_alerts(&self.ctx, args).await)
    }

    #[tool(
        name = "model_dispensing_site_throughput",
        description = "Model an emergency dispensing site (POD) or mass vaccination site as a \
            series of multi-server queues (M/M/c, Erlang C): staff needed at each station \
            (greeting, forms and screening, medical consultation, dispensing, exit; or check-in, \
            screening, vaccination), utilization, chance of waiting, average wait and line \
            length, the bottleneck, site capacity per hour, people inside at once, hours and \
            days to reach a population, staff per shift and total staff shifts, and a surge \
            check. Give population plus operating_hours (or arrivals_per_hour) and sites; \
            override station times or give your staffing to test a plan. Station times are \
            tool defaults to replace with exercise data. Optional XLSX worksheet to /data/output.",
        annotations(
            title = "Model dispensing site throughput",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<DispensingResult>>()
    )]
    async fn model_dispensing_site_throughput(
        &self,
        Parameters(args): Parameters<DispensingArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::epr::model_dispensing_site_throughput(ctx, args)
        })
        .await
    }
}
