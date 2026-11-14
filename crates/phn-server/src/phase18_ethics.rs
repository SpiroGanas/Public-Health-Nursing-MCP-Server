//! Phase 18 tools (QI vs research, ethics, scope of practice): EBP-07, POL-05, POL-06.

use phn_core::Envelope;
use phn_tools::ebp::{QiResearchArgs, QiResearchResult};
use phn_tools::policy::{EthicsArgs, EthicsResult, ScopeArgs, ScopeResult};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking};

// Phase 18 ethics: add the `use` lines for your tools above this line, and your `#[tool]`
// wrappers inside the impl block below.
#[tool_router(router = phase18_ethics_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "qi_vs_research_determination",
        description = "Screening aid for deciding whether a project is quality improvement, \
            program evaluation, public health practice or surveillance, or possible human \
            subjects research. You answer the questions (yes, no, or unsure); the tool never \
            answers for you. It applies the definitions of research and human subject in 45 CFR \
            46.102 (including the public health surveillance exclusion), CDC's guidelines for \
            defining public health research and non-research (primary intent, new or untested \
            interventions, program evaluation, surveillance, emergency response), and the \
            quality improvement ethics literature (Hastings Center group 2007; two criteria from \
            JAMA 2000), shows its reasoning and citation for every question, gives a provisional \
            reading only when the required questions are answered, and flags unsure answers and \
            conflicts (for example authority plus intent to generalize). Call with no answers to \
            get the questions. Writes a draft project description and IRB determination request \
            (DOCX with [NURSE: ...] placeholders and your answers) to /data/output. It is \
            decision support, not a determination: the institution's IRB decides, and no \
            Massachusetts institution's policy was reviewed. Content is pending review; OHRP's \
            own FAQ pages on hhs.gov could not be read.",
        annotations(
            title = "QI, evaluation, or research determination aid",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<QiResearchResult>>()
    )]
    async fn qi_vs_research_determination(
        &self,
        Parameters(args): Parameters<QiResearchArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::ebp::qi_vs_research_determination(ctx, args)
        })
        .await
    }

    #[tool(
        name = "public_health_ethics_analysis",
        description = "Structures an ethics analysis of a public health decision such as \
            quarantine or isolation, exclusion from school, or allocation of scarce resources. \
            You supply the facts; the tool supplies Kass's six questions (goals, effectiveness, \
            burdens, minimizing burdens and alternatives, fair implementation, balancing) with \
            prompts and the facts to gather, her three categories of burden and balancing \
            criteria, a table of options from least to most restrictive (this project's drafting \
            aid, plus the options you add) for you to assess, a justification checklist \
            (effectiveness, proportionality, necessity, least infringement, public \
            justification, fair procedure), the six core values and analysis steps of the APHA \
            Public Health Code of Ethics (2019), and a worksheet (DOCX with [NURSE: ...] \
            placeholders) in /data/output. It never recommends a decision and does not cover \
            Massachusetts law on quarantine or exclusion. Source texts are paraphrased and \
            cited, with copyright flagged; the APHA code and Kass's full article could not be \
            read (known from abstracts and open-access descriptions). Content is pending review.",
        annotations(
            title = "Public health ethics analysis",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<EthicsResult>>()
    )]
    async fn public_health_ethics_analysis(
        &self,
        Parameters(args): Parameters<EthicsArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::policy::public_health_ethics_analysis(ctx, args)
        })
        .await
    }

    #[tool(
        name = "nursing_scope_of_practice_guide",
        description = "Finds the Massachusetts provisions that bear on a nursing scope of \
            practice, delegation, or standing order question: sections of 244 CMR (3.00 RN and \
            LPN roles and delegation, 9.00 standards of conduct, 10.00 definitions, parts of \
            4.00 for APRNs), M.G.L. c. 112 section 80B, and Board of Registration in Nursing \
            advisory rulings (orders and standing orders, vaccines, delegation to unlicensed \
            personnel, RN and LPN scope, over-the-counter medications in schools, harm \
            reduction, APRN delegation of immunizations, and others), each with a citation, a \
            short quotation, and a summary, plus the Board's six decision making questions and \
            its flow chart. It never says that a specific act is within a nurse's scope: it \
            shows the citations and questions and refers you to the Board. This is a PARTIAL \
            snapshot taken on 2026-11-12 from mass.gov (the coverage is in every result): \
            some regulations and most advisory rulings were not read (those are listed by title \
            only). Not legal advice; content pending review.",
        annotations(
            title = "Nursing scope of practice guide (Massachusetts)",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        output_schema = schema_for_output::<Envelope<ScopeResult>>()
    )]
    async fn nursing_scope_of_practice_guide(
        &self,
        Parameters(args): Parameters<ScopeArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::policy::nursing_scope_of_practice_guide(ctx, args)
        })
        .await
    }
}
