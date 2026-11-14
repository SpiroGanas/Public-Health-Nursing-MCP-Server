//! Phase 16 tools (Evidence appraisal and synthesis): EBP-03, EBP-04.

use phn_core::Envelope;
use phn_tools::ebp::{
    AppraiseEvidenceArgs, AppraiseEvidenceResult, SynthesisArgs, SynthesisResult,
};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking, respond};

// Phase 16 appraise: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase16_appraise_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "appraise_evidence",
        description = "Guided critical appraisal of a study you describe or supply: paste an \
            abstract or methods section, or name a TXT, Markdown, DOCX, PDF, or HTML file in /data \
            (checked for client identifiers first and never returned), and optionally give the DOI \
            (looked up in Crossref when online, which sends only the DOI; offline the DOI is used \
            as entered and the response says so). The tool suggests the study design from words in \
            the text (you confirm it with `design`), then lists that design's appraisal questions \
            written from open-access sources, each cited: the 2011 Cochrane risk-of-bias domains and \
            CONSORT items for randomized trials, ROBINS-I bias domains for non-randomized \
            intervention studies, STROBE-based prompts for cohort, case-control, and cross-sectional \
            studies, AMSTAR 2's 16 items for systematic reviews, six appraisal questions for \
            qualitative studies, and SQUIRE 2.0 prompts for quality improvement reports. They are \
            not the CASP checklists (CASP is linked, with its license terms flagged). It never \
            answers a question: give your answers with `answers` and it records them, lists those \
            that need a second look, and applies each instrument's own overall rating rule where \
            one exists (Cochrane summary risk of bias, ROBINS-I overall, AMSTAR 2 overall \
            confidence). For a body of evidence on one outcome, give your GRADE domain judgments \
            (`grade`) to get the certainty level from the starting level (high for randomized \
            trials, low for observational studies), or your GRADE-CERQual concerns (`cerqual`) for a \
            qualitative finding. Writes a Word record of your answers to /data/output. Decision \
            support for your own appraisal, not a substitute for it.",
        annotations(
            title = "Appraise a study (checklists, GRADE)",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        output_schema = schema_for_output::<Envelope<AppraiseEvidenceResult>>()
    )]
    async fn appraise_evidence(
        &self,
        Parameters(args): Parameters<AppraiseEvidenceArgs>,
    ) -> CallToolResult {
        let doi = match phn_tools::ebp::lookup_doi(&self.ctx, args.doi.as_deref()).await {
            Ok(d) => d,
            Err(e) => return respond::<AppraiseEvidenceResult>(Err(e)),
        };
        blocking(&self.ctx, move |ctx| {
            phn_tools::ebp::appraise_evidence(ctx, args, doi)
        })
        .await
    }

    #[tool(
        name = "build_evidence_synthesis_table",
        description = "Builds an evidence synthesis table from the studies you appraised (citation, \
            design, population, sample size, intervention or exposure, comparator, outcome, effect \
            as reported, quality rating and the tool it came from) and a PRISMA 2020 flow diagram \
            from the counts you enter (records identified, duplicates removed, screened, excluded, \
            reports sought and not retrieved, assessed, excluded with reasons, studies and reports \
            included). The counts are checked: any arithmetic error (screened does not equal \
            identified minus removed, reasons that exceed the reports assessed, more studies than \
            reports) is reported as an error and nothing is written. Writes a Word file (table, \
            summary, and the diagram as an image with its text alternative) and an Excel workbook \
            (table, a summary sheet with live COUNT, SUM, MEDIAN, and SUMPRODUCT formulas, and the \
            PRISMA counts with live arithmetic checks) to /data/output, plus the diagram as SVG and \
            Mermaid text; the response also returns the Mermaid text, a self-contained accessible \
            SVG, and the diagram described in sentences. Every entry is what you typed: the tool \
            does not look up, rate, or interpret studies.",
        annotations(
            title = "Evidence synthesis table and PRISMA flow",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<SynthesisResult>>()
    )]
    async fn build_evidence_synthesis_table(
        &self,
        Parameters(args): Parameters<SynthesisArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::ebp::build_evidence_synthesis_table(ctx, args)
        })
        .await
    }
}
