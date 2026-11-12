//! Phase 17 tools (Budgets and economic evaluation): PLAN-09, PLAN-10.

use phn_core::Envelope;
use phn_tools::planning::{BudgetResult, BuildBudgetArgs, EconomicArgs, EconomicResult};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking};

// Phase 17 budget: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase17_budget_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "build_program_budget",
        description = "Builds a line-item program budget for a grant application in federal object \
            classes (personnel, fringe benefits, travel, equipment, supplies, contractual, other, \
            indirect; SF-424A style) with multi-year escalation, cost share and a match check, and \
            indirect costs on the right base: the de minimis rate (up to 15 percent of modified total \
            direct costs under 2 CFR 200.414(f) as currently published; it was 10 percent before \
            October 2024) or a negotiated rate. Subawards count in the base only up to $50,000 and \
            equipment, participant support, and rental costs are left out, per 2 CFR 200.1. Takes line \
            items typed into the request, or a CSV/XLSX in /data (so salary figures stay local; a file \
            with a name, phone, or other identifier column is refused). Writes an Excel workbook with \
            live formulas (change a rate or a line and everything recalculates) and a Word budget \
            justification drafted from the line items, with [NURSE: ...] placeholders for facts only \
            you know, to /data/output. The response has category totals by year, rates, the cost share \
            check, and the rules used with citations; never individual salaries, lines, or staff names. \
            Decision support: the funder's notice controls.",
        annotations(
            title = "Build a program budget",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<BudgetResult>>()
    )]
    async fn build_program_budget(
        &self,
        Parameters(args): Parameters<BuildBudgetArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::planning::build_program_budget(ctx, args)
        })
        .await
    }

    #[tool(
        name = "economic_evaluation_calculator",
        description = "Calculates the economics of a program from numbers you type: cost per participant, \
            cost per outcome (average cost-effectiveness ratio), the incremental cost-effectiveness \
            ratio (ICER) against a comparator with dominance handled (a ratio is not reported when \
            the program costs less and achieves more, or the reverse), net benefit, benefit-cost ratio, \
            return on investment, payback year, and break-even (the value per outcome, or the share of \
            expected outcomes, at which benefits equal cost). Costs and benefits can be spread over up \
            to 30 years and discounted (default 3 percent a year, following CDC's use of the Second \
            Panel on Cost-Effectiveness in Health and Medicine; change it for your funder). Runs a \
            one-way sensitivity analysis (each number varied alone, with a default range of 20 percent \
            either way unless you give one), returns a tornado table, and writes an accessible tornado \
            chart (SVG and PNG with a text alternative) to /data/output/charts. Gives a plain-language \
            reading that flags what the numbers do not show: causation, benefits not valued in dollars, \
            and conclusions that flip within the tested ranges. No threshold for 'cost-effective' is \
            applied unless you give one. Reads no files. Decision support only.",
        annotations(
            title = "Economic evaluation calculator",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<EconomicResult>>()
    )]
    async fn economic_evaluation_calculator(
        &self,
        Parameters(args): Parameters<EconomicArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::planning::economic_evaluation_calculator(ctx, args)
        })
        .await
    }
}
