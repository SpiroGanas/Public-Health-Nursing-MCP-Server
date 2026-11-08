//! Phase 6 tools: immunization forecasting and surveillance feeds.

use phn_core::Envelope;
use phn_tools::cd::{
    NotifiableTrendsArgs, NotifiableTrendsResult, RespiratoryBriefing, RespiratorySurveillanceArgs,
};
use phn_tools::vax::{ForecastImmunizationsArgs, ForecastImmunizationsResult};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking, respond};

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

    #[tool(
        name = "get_notifiable_disease_trends",
        description = "Weekly National Notifiable Diseases Surveillance System (NNDSS) counts \
            from CDC for Massachusetts, New England, and the United States, compared with the \
            same point in the five previous years (year to date and the last 4 weeks: mean, \
            range, ratio, above/within/below the 5-year range), plus provisional year-end \
            totals. Give a condition (e.g. pertussis, measles, hepatitis A); omit it for an \
            overview of every condition in Massachusetts, highest relative to baseline first. \
            Provisional CDC data; DPH (MAVEN) counts can differ.",
        annotations(
            title = "Notifiable disease trends (NNDSS)",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for_output::<Envelope<NotifiableTrendsResult>>()
    )]
    async fn get_notifiable_disease_trends(
        &self,
        Parameters(args): Parameters<NotifiableTrendsArgs>,
    ) -> CallToolResult {
        respond(phn_tools::cd::get_notifiable_disease_trends(&self.ctx, args).await)
    }

    #[tool(
        name = "get_respiratory_and_wastewater_surveillance",
        description = "Weekly briefing on COVID-19, influenza, and RSV in Massachusetts from \
            CDC data: hospital admissions per 100,000 with CDC level (NHSN), percent of \
            emergency department visits with CDC trend (NSSP), test positivity (NREVSS, New \
            England region), the acute respiratory illness activity level, and wastewater \
            viral activity levels for each Massachusetts sampling site (NWSS), with \
            week-over-week direction, data dates and lag, MWRA Deer Island's final Biobot \
            values, and a link to DPH's dashboard.",
        annotations(
            title = "Respiratory and wastewater surveillance",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for_output::<Envelope<RespiratoryBriefing>>()
    )]
    async fn get_respiratory_and_wastewater_surveillance(
        &self,
        Parameters(args): Parameters<RespiratorySurveillanceArgs>,
    ) -> CallToolResult {
        respond(phn_tools::cd::get_respiratory_and_wastewater_surveillance(&self.ctx, args).await)
    }
}
