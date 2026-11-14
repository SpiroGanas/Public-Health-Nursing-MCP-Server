//! Phase 16 tools (Quality improvement planning): EBP-05.

use phn_core::Envelope;
use phn_tools::ebp::{QiPlanArgs, QiPlanResult};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking};

// Phase 16 qi: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase16_qi_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "plan_quality_improvement",
        description = "Guided quality improvement (QI) project planning with the Model for \
            Improvement. From what you tell it, builds an aim statement (how much, for whom, by \
            when) with checks, a family of outcome, process, and balancing measures with \
            operational definitions, a driver diagram (primary drivers, secondary drivers, change \
            ideas), a fishbone (cause-and-effect) diagram, a process map, PDSA cycle plans with \
            checks (prediction, small scale, data over time, linked cycles), and the SQUIRE 2.0 \
            reporting checklist (item names with paraphrases and a license note; SQUIRE is \
            copyrighted). Diagrams come as Mermaid text and accessible SVG/PNG with a text \
            alternative. From your measure data (values typed in, a CSV/XLSX of events grouped by \
            period, or a one-row-per-period tracker file in /data) draws a run chart with a \
            median and applies the IHI/Perla rules (shift of 6, trend of 5, too few or too many \
            runs, astronomical-point screen) and the Anhøj rules (long run, crossings), or an \
            XmR, p, u, or c control chart; only aggregates are returned and small counts are \
            hidden. Writes a DOCX plan, an XLSX measurement tracker with live formulas for the \
            rules and control limits, and the diagrams and chart to /data/output. Content is \
            drafted from open-access sources and pending review; IHI's own pages could not be \
            read. It does not decide whether work is QI or research.",
        annotations(
            title = "Plan a quality improvement project",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<QiPlanResult>>()
    )]
    async fn plan_quality_improvement(
        &self,
        Parameters(args): Parameters<QiPlanArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::ebp::plan_quality_improvement(ctx, args)
        })
        .await
    }
}
