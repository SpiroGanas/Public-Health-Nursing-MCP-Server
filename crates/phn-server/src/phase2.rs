//! Phase 2 tools: epidemiology and biostatistics engine.

use phn_core::Envelope;
use phn_tools::ebp::{SpcArgs, SpcResult};
use phn_tools::epi::{
    AgeAdjustArgs, AgeAdjustResult, CreateChartArgs, CreateChartResult, SampleSizeArgs,
    SampleSizeResult, StandardizedRatioArgs, StandardizedRatioResult, StatTestArgs, StatTestResult,
    TrendArgs, TrendToolResult, TwoByTwoArgs, TwoByTwoResult,
};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking, respond};

#[tool_router(router = phase2_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "age_adjust_rates",
        description = "Directly age-adjust rates to the 2000 U.S. standard population (NCHS \
            weights) or a custom standard, with gamma-method confidence intervals, crude rates, \
            unreliable-rate flags, and optional comparison of each town or year with a reference \
            group. Give age-specific counts and populations for each group.",
        annotations(title = "Age-adjust rates", read_only_hint = true, idempotent_hint = true, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<AgeAdjustResult>>()
    )]
    async fn age_adjust_rates(
        &self,
        Parameters(args): Parameters<AgeAdjustArgs>,
    ) -> CallToolResult {
        respond(phn_tools::epi::age_adjust_rates(&self.ctx, args))
    }

    #[tool(
        name = "analyze_two_by_two",
        description = "Analyze a 2×2 table (exposure by illness): attack rates, risk ratio, risk \
            difference, odds ratio (Woolf and exact), attributable fractions, chi-square, Fisher's \
            exact and mid-p tests. Several tables (strata) give Mantel–Haenszel adjusted measures \
            and the Breslow–Day homogeneity test. For cohort, case-control, or cross-sectional data.",
        annotations(title = "Analyze a 2×2 table", read_only_hint = true, idempotent_hint = true, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<TwoByTwoResult>>()
    )]
    async fn analyze_two_by_two(
        &self,
        Parameters(args): Parameters<TwoByTwoArgs>,
    ) -> CallToolResult {
        respond(phn_tools::epi::analyze_two_by_two(&self.ctx, args))
    }

    #[tool(
        name = "calculate_sample_size",
        description = "Calculate sample size or power: surveys estimating a proportion or mean \
            (finite population correction, design effect, non-response), comparisons of two \
            proportions or means (cohort, trial, case-control), and cluster samples. Shows the \
            formula and every assumption.",
        annotations(title = "Calculate sample size", read_only_hint = true, idempotent_hint = true, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<SampleSizeResult>>()
    )]
    async fn calculate_sample_size(
        &self,
        Parameters(args): Parameters<SampleSizeArgs>,
    ) -> CallToolResult {
        respond(phn_tools::epi::calculate_sample_size(&self.ctx, args))
    }

    #[tool(
        name = "compute_standardized_ratio",
        description = "Indirect standardization: standardized incidence or mortality ratios \
            (SIR/SMR = observed ÷ expected) with exact confidence intervals and p-values, from \
            local stratum populations (age, sex) and reference rates (e.g. statewide). Useful for \
            community cancer concerns and comparing a town with Massachusetts.",
        annotations(title = "Standardized incidence/mortality ratio", read_only_hint = true, idempotent_hint = true, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<StandardizedRatioResult>>()
    )]
    async fn compute_standardized_ratio(
        &self,
        Parameters(args): Parameters<StandardizedRatioArgs>,
    ) -> CallToolResult {
        respond(phn_tools::epi::compute_standardized_ratio(&self.ctx, args))
    }

    #[tool(
        name = "analyze_trends",
        description = "Analyze a time series of counts or rates: annual percent change (Poisson or \
            negative binomial), joinpoint-style segments with APCs and the average annual percent \
            change, interrupted time series at a known change, seasonality for monthly or weekly \
            data, and a Mann–Kendall check. Saves a chart of observed and modeled values.",
        annotations(title = "Analyze trends", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<TrendToolResult>>()
    )]
    async fn analyze_trends(&self, Parameters(args): Parameters<TrendArgs>) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::epi::analyze_trends(ctx, args)
        })
        .await
    }

    #[tool(
        name = "run_statistical_test",
        description = "Pick and run a suitable test for the question and data — t test, \
            Mann–Whitney, paired tests, ANOVA, Kruskal–Wallis, chi-square or Fisher, correlation, \
            linear or logistic regression — check its assumptions, run sensitivity checks, and \
            explain the result plainly. Takes inline data or a CSV/XLSX in /data (aggregate \
            results only; identifier columns are refused).",
        annotations(title = "Run a statistical test", read_only_hint = true, idempotent_hint = true, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<StatTestResult>>()
    )]
    async fn run_statistical_test(
        &self,
        Parameters(args): Parameters<StatTestArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::epi::run_statistical_test(ctx, args)
        })
        .await
    }

    #[tool(
        name = "create_chart",
        description = "Make a report-quality bar, stacked bar, line, dot (estimates with confidence \
            intervals), or histogram chart as PNG/SVG in the output folder, with color-vision-safe \
            colors, direct labels, generated alt text, and the data table. Returns markdown to \
            embed the chart with export_document.",
        annotations(title = "Create a chart", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<CreateChartResult>>()
    )]
    async fn create_chart(&self, Parameters(args): Parameters<CreateChartArgs>) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::epi::create_chart(ctx, args)
        })
        .await
    }

    #[tool(
        name = "create_spc_chart",
        description = "Build a run chart (median, Anhøj rules) or control chart (XmR, p, u, c) with \
            special-cause detection, baselines, and phases after a process change, for quality \
            improvement. Takes inline values or a local line list grouped by week, month, or \
            quarter (aggregates only). Saves the chart to the output folder.",
        annotations(title = "Create an SPC chart", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<SpcResult>>()
    )]
    async fn create_spc_chart(&self, Parameters(args): Parameters<SpcArgs>) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::ebp::create_spc_chart(ctx, args)
        })
        .await
    }
}
