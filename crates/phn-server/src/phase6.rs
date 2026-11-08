//! Phase 6 tools: immunization forecasting and surveillance feeds.

use phn_core::Envelope;
use phn_tools::vax::{ForecastImmunizationsArgs, ForecastImmunizationsResult};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking};

#[tool_router(router = phase6_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "forecast_immunizations_due",
        description = "Evaluate an immunization history and forecast the doses due, using CDC's \
            Clinical Decision Support for Immunization (CDSi) logic and supporting data (ACIP \
            schedule, childhood and adult). For one person give date_of_birth, sex, and doses \
            (CVX code, vaccine type, or trade name plus date); add conditions such as \
            pregnancy or asplenia for risk-based series. Returns valid and invalid doses with \
            reasons, status per vaccine group (overdue, due now, complete), earliest, \
            recommended, and past-due dates, and a catch-up schedule. For many people give a \
            CSV/XLSX file in /data with one row per dose: per-person results go to \
            /data/output and only suppressed counts are returned. Decision support: verify \
            against MIIS.",
        annotations(
            title = "Forecast immunizations due (CDSi)",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<ForecastImmunizationsResult>>()
    )]
    async fn forecast_immunizations_due(
        &self,
        Parameters(args): Parameters<ForecastImmunizationsArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::vax::forecast_immunizations_due(ctx, args)
        })
        .await
    }
}
