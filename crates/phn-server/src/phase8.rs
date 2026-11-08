//! Phase 8 tools: maternal, child, and family health.

use phn_core::Envelope;
use phn_tools::mch::{
    GrowthArgs, GrowthResult, LeadGuideArgs, LeadGuideResult, MandatedReporterArgs,
    MandatedReporterResult, MilestoneArgs, MilestoneResult,
};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

#[allow(unused_imports)]
use crate::server::{PhnServer, blocking, respond};

#[tool_router(router = phase8_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "calculate_growth_percentiles",
        description = "Growth percentiles and z-scores for a child: WHO 2006 standards from birth \
            to 24 months (weight-for-age, length-for-age, weight-for-length, BMI, head \
            circumference) and CDC 2000 charts from 2 to 20 years (weight, stature, BMI with \
            CDC's 2022 extended BMI-for-age method above the 95th percentile and BMI category). \
            Corrects for prematurity, accepts kg/lb and cm/in, compares several visits (z-score \
            change), and flags values to re-measure. With measurements_path, scores a CSV/XLSX \
            in /data, writes per-row results to /data/output, and returns only suppressed counts.",
        annotations(title = "Growth percentiles", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<GrowthResult>>()
    )]
    async fn calculate_growth_percentiles(
        &self,
        Parameters(args): Parameters<GrowthArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::mch::calculate_growth_percentiles(ctx, args)
        })
        .await
    }

    #[tool(
        name = "childhood_lead_screening_guide",
        description = "Massachusetts childhood lead screening and follow-up (105 CMR 460.000 and \
            DPH's Childhood Lead Poisoning Prevention Program): which screenings a child needs \
            by age (9-12 months, 2, 3, and 4 in a high-risk community; high-risk schedules), \
            whether a town is on DPH's current high-risk list with its screening rate, venous \
            confirmation timing for a capillary result, follow-up testing, case management and \
            environmental actions by venous blood lead level, reporting duties, the Lead Law, \
            and family teaching. Optionally writes a family teaching sheet (DOCX; English or the \
            agency's own translation) to /data/output.",
        annotations(title = "Childhood lead screening guide", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<LeadGuideResult>>()
    )]
    async fn childhood_lead_screening_guide(
        &self,
        Parameters(args): Parameters<LeadGuideArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::mch::childhood_lead_screening_guide(ctx, args)
        })
        .await
    }

    #[tool(
        name = "mandated_reporter_51a_guide",
        description = "Massachusetts child abuse and neglect reporting (M.G.L. c. 119, s. 51A) for \
            nurses: who must report, the immediate call to DCF and the written report within 48 \
            hours (with the deadline from the call time), what the report must contain, \
            penalties and protections, DCF definitions, domestic violence and prenatal \
            substance exposure guidance, and what DCF does next. Returns the written-report \
            narrative template; with notes_path (the nurse's notes in /data) it fills a draft \
            written to /data/output and returns only file names and field counts. Never send \
            the family's names or details in the arguments.",
        annotations(title = "Mandated reporter (51A) guide", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<MandatedReporterResult>>()
    )]
    async fn mandated_reporter_51a_guide(
        &self,
        Parameters(args): Parameters<MandatedReporterArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::mch::mandated_reporter_51a_guide(ctx, args)
        })
        .await
    }

    #[tool(
        name = "check_developmental_milestones",
        description = "CDC \"Learn the Signs. Act Early.\" developmental milestones (2022 revision) \
            for a child's age from 2 months to 5 years, with IDs to mark milestones the child \
            does not yet do; flags missed milestones or lost skills with the act-early steps, \
            AAP screening ages (general at 9, 18, 30 months; autism at 18 and 24), Massachusetts \
            Early Intervention eligibility and referral (birth to 3) or preschool special \
            education (3 and older), and the EI programs that serve the family's town. Shows \
            the corrected-age checklist for babies born early.",
        annotations(title = "Developmental milestones", read_only_hint = true, idempotent_hint = true, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<MilestoneResult>>()
    )]
    async fn check_developmental_milestones(
        &self,
        Parameters(args): Parameters<MilestoneArgs>,
    ) -> CallToolResult {
        respond(phn_tools::mch::check_developmental_milestones(
            &self.ctx, args,
        ))
    }
}
