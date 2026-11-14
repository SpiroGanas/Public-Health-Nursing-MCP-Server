//! Phase 17 tools (Department budget and staffing): LEAD-01, LEAD-02.

use phn_core::Envelope;
use phn_tools::leadership::{BudgetArgs, BudgetResult, StaffingArgs, StaffingResult};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking};

// Phase 17 finance: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase17_finance_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "analyze_health_department_budget",
        description = "Analyzes your health department's budget or ledger file (CSV or XLSX in \
            /data) and returns category totals only: year-to-date spending and revenue against the \
            share of the fiscal year that has passed (Massachusetts municipal year starting July \
            1 by default; configurable), the amount available after encumbrances, flags for lines \
            over budget or ahead of or behind pace, the revenue mix (fees and revolving funds, \
            municipal appropriation, state Public Health Excellence (PHE) grants, other state, \
            federal, other grants), cost per service from volumes you give, and a year-end \
            forecast by two labelled methods (straight line, and recent run rate when the file \
            has a month column). Budget per resident can be shown beside DPH's published \
            2022-2023 figures. Privacy: only the category, type, source, amount, account, month, \
            and position columns are read; vendor, employee, and payee columns are refused and \
            never returned; all salary and wage categories are combined into one row, which is \
            hidden (and left out of totals) when it has fewer than 3 positions. Writes an XLSX \
            with live formulas and a DOCX summary to /data/output. Decision support: the figures \
            come from your file and are not checked against the general ledger; the PHE content \
            is drafted from DPH's public pages and pending review (award terms were not read).",
        annotations(
            title = "Analyze the health department budget",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<BudgetResult>>()
    )]
    async fn analyze_health_department_budget(
        &self,
        Parameters(args): Parameters<BudgetArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::leadership::analyze_health_department_budget(ctx, args)
        })
        .await
    }

    #[tool(
        name = "model_staffing_needs",
        description = "Estimates how many public health nurse full-time equivalents (FTE) your \
            services need, from annual volumes and minutes per unit that you supply (typed in, or \
            read from a CSV/XLSX of services in /data; only the service, volume, and minutes \
            columns are read). Productive hours per FTE are calculated from paid hours minus \
            holiday, vacation, sick and personal, and training hours, less the share that is not \
            direct service; every input is adjustable and the defaults are labelled placeholders \
            with no published source. Compares scenarios (baseline, volume growth, added \
            services), shows the gap to your current FTE, residents per nurse and clients per \
            nurse, and compares with a benchmark only if you supply one: no public health nurse \
            staffing benchmark is bundled because none could be read from a source this server \
            could fetch (the tool lists what was looked for). Writes an XLSX with live formulas \
            and a DOCX summary to /data/output. Decision support for workforce planning, not a \
            staffing determination.",
        annotations(
            title = "Model public health nurse staffing needs",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<StaffingResult>>()
    )]
    async fn model_staffing_needs(
        &self,
        Parameters(args): Parameters<StaffingArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::leadership::model_staffing_needs(ctx, args)
        })
        .await
    }
}
