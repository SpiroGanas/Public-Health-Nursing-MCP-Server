//! Phase 11 tools: substance use, crisis resources, and health communication.

use phn_core::Envelope;
use phn_tools::comms::{ReadabilityArgs, ReadabilityResult};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking};

#[tool_router(router = phase11_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "check_readability",
        description = "Check the reading level and plain language of a draft (pasted text or a \
            DOCX, HTML, PDF, TXT, or Markdown file in /data): Flesch Reading Ease, \
            Flesch-Kincaid, SMOG, and Gunning Fog grades against a target; long sentences, \
            jargon with CDC Everyday Words alternatives, possible passive voice, acronyms not \
            spelled out, and long paragraphs or lists; and the CDC Clear Communication Index \
            (scored from the nurse's yes/no answers, with the tool's suggestions). Use it before \
            publishing any material for the public.",
        annotations(title = "Check readability", read_only_hint = true, idempotent_hint = true, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<ReadabilityResult>>()
    )]
    async fn check_readability(
        &self,
        Parameters(args): Parameters<ReadabilityArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::comms::check_readability(ctx, args)
        })
        .await
    }
}
