//! Phase 19 tools (Law search, policy briefs, information sharing): POL-02, POL-04, POL-07.

use phn_core::Envelope;
use phn_tools::policy::{BriefArgs, BriefResult, LawResult, LawSearchArgs, SharingArgs, SharingResult};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking};

// Phase 19 law: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase19_law_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "search_public_health_law",
        description = "Searches a bundled corpus of Massachusetts public health law and answers \
            with citations: M.G.L. chapter 111 (Public Health), 105 CMR (Department of Public \
            Health regulations), 244 CMR (Board of Registration in Nursing), and, if you keep \
            them in a local folder, your board of health's own regulations. Give a citation \
            (105 CMR 300.200, M.G.L. c. 111, section 95, 105 CMR 410) to look it up, or words \
            (board of health order to quarantine, lead paint abatement, delegation to unlicensed \
            personnel) to search. Each hit has the section number, title, link, the date shown \
            on the chapter's page, the date the snapshot was taken, whether the section is in \
            full text or index-only (title and link), and a short excerpt of 25 words or fewer \
            when the text is bundled; it never returns long text. Topic starting points are \
            suggested for common topics (reportable diseases, isolation and quarantine, \
            confidentiality, immunization, school health, lead, housing, food, tobacco, board of \
            health powers, nuisances, tuberculosis, rabies, nursing practice). The bundled \
            corpus is partial and says exactly what it covers; an optional larger corpus can be \
            installed in law_corpus/ (data or cache folder). Local regulation files are indexed \
            in your container, contact details are masked, and files that look like client \
            records are refused. Decision support, not legal advice; regulations on mass.gov \
            are unofficial copies. Content pending review.",
        annotations(
            title = "Search Massachusetts public health law",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<LawResult>>()
    )]
    async fn search_public_health_law(
        &self,
        Parameters(args): Parameters<LawSearchArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::policy::search_public_health_law(ctx, args)
        })
        .await
    }

    #[tool(
        name = "draft_policy_brief",
        description = "Drafts a one-page or longer policy brief (Word and/or Markdown, in \
            /data/output) in the order of the Eightfold Path of policy analysis: problem, \
            evidence, options, criteria, the options compared against the criteria in an \
            accessible table (ratings are words, not colors), trade-offs, recommendation, \
            implementation, references. It is a template with local fill: you supply the \
            problem, the evidence with citations (for example from the evidence tools), the \
            options, the criteria, the ratings, and the recommendation; everything else is a \
            [NURSE: ...] placeholder. It never invents facts or statistics, reports which steps \
            are filled, counts the placeholders left, and runs the plain-language readability \
            check on the prose you supplied. The method is described from open-access articles \
            and abstracts; Bardach's book was not read. Content pending review.",
        annotations(
            title = "Draft a policy brief",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<BriefResult>>()
    )]
    async fn draft_policy_brief(
        &self,
        Parameters(args): Parameters<BriefArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::policy::draft_policy_brief(ctx, args)
        })
        .await
    }

    #[tool(
        name = "health_information_sharing_guide",
        description = "A decision guide for which rules govern a health record and what \
            questions decide whether it can be shared. You answer structured questions (who holds \
            the record and in what role, what kind of record, who the recipient is, the purpose, \
            consent, whether the person is a minor); it returns the regimes that may govern, \
            each with provisions, 25-word quotations, summaries, cautions, and facts to confirm: \
            HIPAA (45 CFR 160 and 164, including the public health, immunization-to-school, and \
            serious threat provisions), FERPA (34 CFR 99, school health records), 42 CFR Part 2 \
            (substance use disorder records, as amended in 2024), the Massachusetts public \
            records law (M.G.L. c. 66 section 10, c. 4 section 7), 105 CMR 300.000 and the \
            Massachusetts statutes on HIV test results, venereal disease records, immunization \
            registry data, and patient access. Call with no answers to get the questions. It \
            NEVER says that a disclosure is or is not permitted: it is decision support, not \
            legal advice, and refers you to agency counsel. Partial snapshot (coverage in every \
            result): the FERPA statute, the HHS and Education joint guidance, and 603 CMR 23.00 \
            could not be read. Content pending review.",
        annotations(
            title = "Health information sharing decision guide",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<SharingResult>>()
    )]
    async fn health_information_sharing_guide(
        &self,
        Parameters(args): Parameters<SharingArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::policy::health_information_sharing_guide(ctx, args)
        })
        .await
    }
}
