//! Phase 9 tools: PHN practice toolkit and case management.

use phn_core::Envelope;
use phn_tools::cd::{TbContactArgs, TbContactPlan};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking};

#[tool_router(router = phase9_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "plan_tb_contact_investigation",
        description = "Plan a tuberculosis contact investigation with the 2005 NTCA/CDC guidelines: \
            whether to investigate, the infectious period (3 months before symptoms or first \
            finding, or 4 weeks before diagnosis), contact priority (high, medium, low) from the \
            published decision trees, initial-encounter and evaluation deadlines in business days, \
            and the retest 8-10 weeks after last exposure, plus DPH TB program contacts, reporting \
            rules, and state TB clinics. Give contact groups as counts and risk facts (never \
            names). Writes an XLSX tracking workbook to /data/output; with contact_file it fills \
            the workbook from a local list and returns only suppressed counts and expansion signals.",
        annotations(title = "Plan a TB contact investigation", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<TbContactPlan>>()
    )]
    async fn plan_tb_contact_investigation(
        &self,
        Parameters(args): Parameters<TbContactArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::cd::plan_tb_contact_investigation(ctx, args)
        })
        .await
    }
}
