//! Phase 1 tools: local data, privacy guardrails, and documents.

use phn_core::Envelope;
use phn_tools::comms::{AccessibilityArgs, AccessibilityResult};
use phn_tools::core::{
    ExportDocumentArgs, ExportResult, LocalFileResult, PolicySearchResult, ReadLocalFileArgs,
    SearchPoliciesArgs,
};
use phn_tools::epi::{
    DeidentifyArgs, DeidentifyResult, ProfileArgs, ProfileResult, SuppressionArgs,
    SuppressionResult,
};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking};

#[tool_router(router = phase1_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "read_local_file",
        description = "List files in the nurse's mounted /data folder, or extract text and tables \
            from a PDF, DOCX, XLSX, CSV, HTML, Markdown, or text file. Document text is returned \
            with Social Security numbers and labeled dates of birth and record numbers redacted. \
            For CSV/XLSX only the structure and identifier warnings are returned unless \
            include_rows is set, and even then identifiers are masked.",
        annotations(
            title = "Read a local file",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<LocalFileResult>>()
    )]
    async fn read_local_file(
        &self,
        Parameters(args): Parameters<ReadLocalFileArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::core::read_local_file(ctx, args)
        })
        .await
    }

    #[tool(
        name = "search_agency_policies",
        description = "Answer 'what does our policy say?' by searching the agency's own policies \
            and procedures in /data/policies (PDF, DOCX, Markdown, text, HTML). Returns matching \
            passages with file and section (or page) citations. The index updates automatically \
            when files change.",
        annotations(
            title = "Search agency policies",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<PolicySearchResult>>()
    )]
    async fn search_agency_policies(
        &self,
        Parameters(args): Parameters<SearchPoliciesArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::core::search_agency_policies(ctx, args)
        })
        .await
    }

    #[tool(
        name = "export_document",
        description = "Turn Markdown content into a professional, accessible document in the \
            output folder: DOCX or tagged PDF with the agency letterhead, XLSX or CSV for tables, \
            or SVG/PNG images. For letters to many people, give merge_data_path (a CSV/XLSX in \
            /data) and {{Column}} placeholders: merged letters are written to /data/output and \
            never returned. CSV cells that start like a spreadsheet formula are written as text.",
        annotations(
            title = "Export a document",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<ExportResult>>()
    )]
    async fn export_document(
        &self,
        Parameters(args): Parameters<ExportDocumentArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::core::export_document(ctx, args)
        })
        .await
    }

    #[tool(
        name = "apply_small_cell_suppression",
        description = "Make a table of counts safe to share: hide counts below the suppression \
            threshold (DPH-style, default counts of 1–4) and apply complementary suppression so \
            hidden values cannot be recovered from totals. Accepts an inline table or a CSV/XLSX \
            of counts in /data; refuses record-level data.",
        annotations(
            title = "Apply small-cell suppression",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<SuppressionResult>>()
    )]
    async fn apply_small_cell_suppression(
        &self,
        Parameters(args): Parameters<SuppressionArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::epi::apply_small_cell_suppression(ctx, args)
        })
        .await
    }

    #[tool(
        name = "profile_local_dataset",
        description = "Describe a local CSV/XLSX dataset: variables, inferred types, missing \
            values, duplicates, and possible identifiers (names, dates of birth, addresses, phone \
            numbers, record numbers). Shows structure first; numeric summaries and suppressed \
            frequency tables only when include_value_summaries is set, and never for identifier \
            columns.",
        annotations(
            title = "Profile a local dataset",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<ProfileResult>>()
    )]
    async fn profile_local_dataset(
        &self,
        Parameters(args): Parameters<ProfileArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::epi::profile_local_dataset(ctx, args)
        })
        .await
    }

    #[tool(
        name = "deidentify_dataset",
        description = "Write a de-identified copy of a local CSV/XLSX dataset to the output \
            folder using the HIPAA Safe Harbor method (removes the 18 identifiers, reduces dates \
            to year, applies the three-digit ZIP rule, top-codes ages over 89) or prepare a \
            limited data set. Returns only a report of what changed.",
        annotations(
            title = "De-identify a dataset",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<DeidentifyResult>>()
    )]
    async fn deidentify_dataset(
        &self,
        Parameters(args): Parameters<DeidentifyArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::epi::deidentify_dataset(ctx, args)
        })
        .await
    }

    #[tool(
        name = "check_document_accessibility",
        description = "Check a DOCX, HTML, or PDF file in /data against Section 508 and WCAG 2.1 \
            AA: document title and language, alt text, and scanned pages for every format; \
            headings and their order, table header rows, color contrast, and link text for DOCX \
            and HTML; tagging and fonts for PDF (other PDF checks are listed as manual). Lists \
            failures with how to fix them. Quoted excerpts have possible client identifiers \
            redacted.",
        annotations(
            title = "Check document accessibility",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<AccessibilityResult>>()
    )]
    async fn check_document_accessibility(
        &self,
        Parameters(args): Parameters<AccessibilityArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::comms::check_document_accessibility(ctx, args)
        })
        .await
    }
}
