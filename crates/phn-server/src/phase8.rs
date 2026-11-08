//! Phase 8 tools: maternal, child, and family health.

use phn_core::Envelope;
use phn_tools::mch::{GrowthArgs, GrowthResult, LeadGuideArgs, LeadGuideResult};
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
}
