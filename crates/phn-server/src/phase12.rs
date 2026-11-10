//! Phase 12 tools: emergency preparedness I: situational awareness and planning.

use phn_core::Envelope;
use phn_tools::env::{AirQualityArgs, AirQualityResult, HeatPlanArgs, HeatPlanResult};
use phn_tools::epr::{
    CasperArgs, CasperResult, DispensingArgs, DispensingResult, ElectricityArgs, ElectricityResult,
    HazardAlertsArgs, HazardAlertsResult, HvaArgs, HvaResult, ShelterArgs, ShelterResult,
};
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

    #[tool(
        name = "air_quality_protective_guidance",
        description = "Current and forecast Air Quality Index (AQI) from EPA AirNow for the \
            Massachusetts reporting area nearest a town (or every area statewide), with the \
            AQI category, sensitive groups for the pollutant (ozone or particle pollution), and \
            protective actions for sensitive groups, the public, schools (EPA's outdoor activity \
            guidance), and outdoor events. Also converts a pollutant concentration to an AQI with \
            the 2024 EPA breakpoints, or gives guidance for an AQI value you supply. When AirNow \
            cannot be reached it says so and lists actions for every category.",
        annotations(
            title = "Air quality protective guidance",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for_output::<Envelope<AirQualityResult>>()
    )]
    async fn air_quality_protective_guidance(
        &self,
        Parameters(args): Parameters<AirQualityArgs>,
    ) -> CallToolResult {
        respond(phn_tools::env::air_quality_protective_guidance(&self.ctx, args).await)
    }

    #[tool(
        name = "heat_health_action_planner",
        description = "Heat response plan for a Massachusetts town or county: the 7-day NWS \
            HeatRisk (0-4) over the area and the NWS forecast, active heat alerts, the local \
            heat-vulnerable population (older adults, young children, disability, poverty, no \
            vehicle, limited English, chronic disease from CDC PLACES, socially vulnerable \
            tracts, and electricity-dependent Medicare beneficiaries from HHS emPOWER), and \
            public health activation steps for the forecast peak (or a level you choose). Writes \
            a DOCX checklist to /data/output with cooling centers (from your CSV/XLSX file, \
            which is not shown in chat), outreach call/door/text scripts, school and child care \
            and outdoor worker guidance, heat illness teaching, and medication and power \
            outage planning.",
        annotations(
            title = "Heat health action planner",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        output_schema = schema_for_output::<Envelope<HeatPlanResult>>()
    )]
    async fn heat_health_action_planner(
        &self,
        Parameters(args): Parameters<HeatPlanArgs>,
    ) -> CallToolResult {
        respond(phn_tools::env::heat_health_action_planner(&self.ctx, args).await)
    }

    #[tool(
        name = "hazard_vulnerability_assessment",
        description = "Facilitate a hazard vulnerability assessment (HVA) for a Massachusetts \
            town or county: 21 hazards (the 15 in the 2023 ResilientMass Plan plus pandemic, \
            power outage, water, hazmat, mass casualty, and cyber) with seeds for discussion \
            (ResilientMass statewide likelihood, human consequence, and warning time; FEMA \
            declarations for the county since 1953; FEMA National Risk Index scores from a table \
            you put in /data). Give the group's scores (probability 0-3; impacts on people, \
            property, operations 0-3; preparedness, internal and external response gaps 1-3) to \
            get severity, relative risk (0-100%), and a ranking. Writes an XLSX facilitation \
            workbook with the scales to /data/output.",
        annotations(
            title = "Hazard vulnerability assessment",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        output_schema = schema_for_output::<Envelope<HvaResult>>()
    )]
    async fn hazard_vulnerability_assessment(
        &self,
        Parameters(args): Parameters<HvaArgs>,
    ) -> CallToolResult {
        respond(phn_tools::epr::hazard_vulnerability_assessment(&self.ctx, args).await)
    }

    #[tool(
        name = "functional_needs_shelter_intake",
        description = "Shelter health intake using the CMIST framework (communication, \
            maintaining health, independence, support and safety, transportation): returns the \
            intake questions and dispositions, writes a blank DOCX/XLSX form to /data/output, and \
            estimates for an expected number of residents how many will need interpreters, \
            hearing or vision support, accessible cots, personal assistance, power for medical \
            equipment (HHS emPOWER), insulin refrigeration, or accessible transport, from local \
            ACS, PLACES, and emPOWER rates, with staff per shift. With intake_path (a CSV/XLSX of \
            registrants in /data) it fills one form per person into /data/output and returns \
            only suppressed counts by need and language; agency translations in \
            /data/templates/<language>/cmist_intake.md are used when present.",
        annotations(
            title = "Functional needs shelter intake (CMIST)",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        output_schema = schema_for_output::<Envelope<ShelterResult>>()
    )]
    async fn functional_needs_shelter_intake(
        &self,
        Parameters(args): Parameters<ShelterArgs>,
    ) -> CallToolResult {
        respond(phn_tools::epr::functional_needs_shelter_intake(&self.ctx, args).await)
    }

    #[tool(
        name = "get_electricity_dependent_population",
        description = "HHS emPOWER counts of Medicare beneficiaries who rely on \
            electricity-dependent medical equipment (ventilators, oxygen concentrators, BiPAP, \
            feeding and infusion pumps, home dialysis, power wheelchairs, electric beds, cardiac \
            assist devices) for a Massachusetts town, county, the state, or ZIP codes: totals \
            and rates per 1,000 beneficiaries compared with Massachusetts and the U.S., ZIP-code \
            lists for outreach and shelter planning, and power outage planning actions with \
            utility numbers. Optionally totals a ZIP-level aggregate file in /data (for example \
            DPH's emPOWER Emergency Planning Dataset); individual-level files are refused.",
        annotations(
            title = "Electricity-dependent population (emPOWER)",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for_output::<Envelope<ElectricityResult>>()
    )]
    async fn get_electricity_dependent_population(
        &self,
        Parameters(args): Parameters<ElectricityArgs>,
    ) -> CallToolResult {
        respond(phn_tools::epr::get_electricity_dependent_population(&self.ctx, args).await)
    }

    #[tool(
        name = "plan_casper_assessment",
        description = "Plan a CDC CASPER rapid needs assessment for a Massachusetts town, \
            several towns, or a county: selects clusters (default 30) of 2020 census blocks \
            with probability proportional to housing units (or occupied households) using a \
            reported seed so the selection can be reproduced, lists each cluster's block GEOID, \
            households, interviews, and sampling interval, and writes the cluster list, tracking \
            forms, and a team packet (consent script, household selection steps, interview tips, \
            supplies, referral form, questionnaire guidance) to /data/output. With results_path \
            (one row per completed interview, with a cluster column) it applies the toolkit's \
            weights and returns weighted percentages, estimated households, 95% confidence \
            intervals, design effects, and completion, contact, and cooperation rates \
            (aggregates only; cells under 5 suppressed).",
        annotations(
            title = "Plan a CASPER assessment",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<CasperResult>>()
    )]
    async fn plan_casper_assessment(
        &self,
        Parameters(args): Parameters<CasperArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::epr::plan_casper_assessment(ctx, args)
        })
        .await
    }
}
