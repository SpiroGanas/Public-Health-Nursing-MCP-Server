//! Phase 19 tools (Legislation and federal rules): POL-01, POL-03.

use crate::server::{PhnServer, respond};
use phn_core::Envelope;
use phn_tools::policy::{FederalArgs, FederalResult, TrackArgs, TrackResult};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

// Phase 19 tracking: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase19_tracking_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "track_ma_legislation",
        description = "Tracks Massachusetts bills on public health topics in the Legislature's \
            public data (the 194th General Court, 2025-2026): find bills by topic (vaccines, \
            communicable disease, tobacco and vaping, substance use, mental health, maternal and \
            child health, school health, older adults, environmental health, food and nutrition, \
            local public health, nursing workforce, health equity, oral health, preparedness, \
            coverage, reproductive health, injury prevention), by your own title keywords, or by \
            bill number. For each bill: number, title, link, stage (filed, in committee, reported \
            out, on the floor, with the Governor, enacted, or sent to study or replaced by a new \
            draft; this project's reading of the history), the committee, hearing dates with days \
            until the hearing, the committee's reporting deadline, the last action with its date, \
            and a cited excerpt of the bill text of 25 words or fewer. Adds structured questions \
            for the nurse's own analysis; it never says what a bill would do or what it means for \
            local health. Reads a topic watchlist CSV (kind, value, note) from /data and can save \
            one to /data/output. Live from the Legislature's API (no key; requests carry only a \
            General Court and bill numbers), falling back to a dated bundled snapshot offline, and \
            says which was used. Bills are found by words in their titles only.",
        annotations(
            title = "Track Massachusetts legislation",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        output_schema = schema_for_output::<Envelope<TrackResult>>()
    )]
    async fn track_ma_legislation(
        &self,
        Parameters(args): Parameters<TrackArgs>,
    ) -> CallToolResult {
        respond(phn_tools::policy::track_ma_legislation(&self.ctx, args).await)
    }

    #[tool(
        name = "monitor_federal_rules",
        description = "Watches the Federal Register for proposed rules, final rules, and notices \
            from CDC, CMS, HRSA, EPA, USDA, and HHS (also FDA, SAMHSA, ACF, FNS, FSIS, or any \
            agency slug you give), filtered by agency, document type, topic words, and dates. \
            Lists each document with its comment close date counted down against today's date \
            (days left, closes today, or closed), docket IDs, and links. Optionally cross-checks \
            open comment periods on Regulations.gov when a free api.data.gov key is set in \
            PHN_REGULATIONS_GOV_API_KEY (the key is never shown). Can write a public comment \
            outline (Word, with [NURSE: ...] placeholders) for one document to /data/output, \
            built from the tips EPA publishes for effective comments, filled from a field/value \
            CSV in /data if you give one. Live data only: when the Federal Register cannot be \
            reached the tool says so and lists no documents instead of showing old data as \
            current. Decision support; it does not file comments.",
        annotations(
            title = "Monitor federal rules and comment periods",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        output_schema = schema_for_output::<Envelope<FederalResult>>()
    )]
    async fn monitor_federal_rules(
        &self,
        Parameters(args): Parameters<FederalArgs>,
    ) -> CallToolResult {
        respond(phn_tools::policy::monitor_federal_rules(&self.ctx, args).await)
    }
}
