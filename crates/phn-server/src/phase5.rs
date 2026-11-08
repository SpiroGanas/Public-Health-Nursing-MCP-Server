//! Phase 5 tools: immunizations I.

use phn_core::Envelope;
use phn_tools::core::{MedicationArgs, MedicationResult};
use phn_tools::vax::{
    ClinicArgs, ClinicPlan, CompareArgs, CompareResult, ConfidenceArgs, ConfidenceGuideResult,
    CoverageArgs, CoverageResult, ExcursionArgs, ExcursionResult, ProductArgs, ProductLookup,
    ScheduleArgs, ScheduleResult, SchoolCheckArgs, SchoolCheckResult,
};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking, respond};

#[tool_router(router = phase5_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "get_immunization_schedule",
        description = "Routine immunization schedule for one vaccine or all vaccines (children and \
            adolescents, or adults): when doses are due, with each body's position side by side \
            (Massachusetts DPH, AAP/ACOG/AAFP, CDC/ACIP) and edition dates, ordered by the \
            configured recommendation basis. Includes CDC minimum ages and intervals and \
            contraindications and precautions. Does not forecast a person's doses.",
        annotations(title = "Immunization schedule", read_only_hint = true, idempotent_hint = true, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<ScheduleResult>>()
    )]
    async fn get_immunization_schedule(
        &self,
        Parameters(args): Parameters<ScheduleArgs>,
    ) -> CallToolResult {
        respond(phn_tools::vax::get_immunization_schedule(&self.ctx, args))
    }

    #[tool(
        name = "compare_vaccine_recommendations",
        description = "Compare what Massachusetts DPH, AAP, ACOG, AAFP, the Northeast Public Health \
            Collaborative, and CDC/ACIP recommend for a vaccine or question (for example the \
            hepatitis B birth dose, COVID-19 in pregnancy, or MMRV), with dates and sources, and \
            highlight where they differ since the 2025-2026 federal changes. Omit the query to \
            list every topic and whether the bodies agree.",
        annotations(title = "Compare vaccine recommendations", read_only_hint = true, idempotent_hint = true, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<CompareResult>>()
    )]
    async fn compare_vaccine_recommendations(
        &self,
        Parameters(args): Parameters<CompareArgs>,
    ) -> CallToolResult {
        respond(phn_tools::vax::compare_vaccine_recommendations(&self.ctx, args))
    }

    #[tool(
        name = "check_ma_school_immunization_requirements",
        description = "Check a student's dose dates against the Massachusetts school immunization \
            requirements (105 CMR 220.000 and DPH's annual document) for child care/preschool, \
            K-12, or college: met, not met, or not yet due for each requirement, why, doses that \
            do not count, and the earliest date a missing dose would count. Handles proof of \
            immunity and exemptions. With roster_path, checks a CSV/XLSX roster in /data, writes \
            per-student results to /data/output, and returns only suppressed counts by grade.",
        annotations(title = "Check MA school immunization requirements", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<SchoolCheckResult>>()
    )]
    async fn check_ma_school_immunization_requirements(
        &self,
        Parameters(args): Parameters<SchoolCheckArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::vax::check_ma_school_immunization_requirements(ctx, args)
        })
        .await
    }

    #[tool(
        name = "analyze_vaccination_coverage",
        description = "Rank Massachusetts schools and towns by kindergarten or grade 7 immunization \
            coverage or exemptions from DPH's school immunization survey, flag schools below the \
            population-immunity threshold (for example 95% for measles), and compare Massachusetts \
            with the U.S. using CDC National Immunization Survey and VaxView estimates. Can also \
            rank the nurse's own school-level counts from a file in /data.",
        annotations(title = "Analyze vaccination coverage", read_only_hint = true, idempotent_hint = true, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<CoverageResult>>()
    )]
    async fn analyze_vaccination_coverage(
        &self,
        Parameters(args): Parameters<CoverageArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::vax::analyze_vaccination_coverage(ctx, args)
        })
        .await
    }

    #[tool(
        name = "vaccine_temperature_excursion_helper",
        description = "Walk through a vaccine storage temperature excursion (refrigerator, freezer, \
            or ultra-cold): compare readings with the normal range and the MA Vaccine Program's \
            reporting thresholds, list the CDC Storage and Handling Toolkit steps and DPH's steps \
            for state-supplied vaccine, give contacts, and optionally write an excursion worksheet \
            (XLSX or DOCX). Never decides whether vaccine is usable; the manufacturer does.",
        annotations(title = "Vaccine temperature excursion helper", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<ExcursionResult>>()
    )]
    async fn vaccine_temperature_excursion_helper(
        &self,
        Parameters(args): Parameters<ExcursionArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::vax::vaccine_temperature_excursion_helper(ctx, args)
        })
        .await
    }

    #[tool(
        name = "lookup_vaccine_product",
        description = "Look up a vaccine product by name, trade name, CDC CVX code, MVX \
            manufacturer code, or NDC (fuzzy matching): CVX description and status, trade names \
            and manufacturers, active NDCs, licensed ages and storage where bundled, the current \
            Vaccine Information Statement edition and where to find translations, and \
            optionally the current DailyMed package insert.",
        annotations(title = "Look up a vaccine product", read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for_output::<Envelope<ProductLookup>>()
    )]
    async fn lookup_vaccine_product(
        &self,
        Parameters(args): Parameters<ProductArgs>,
    ) -> CallToolResult {
        respond(phn_tools::vax::lookup_vaccine_product(&self.ctx, args).await)
    }

    #[tool(
        name = "plan_vaccination_clinic",
        description = "Plan a vaccination clinic from expected turnout and hours: stations, \
            staffing per station, throughput, observation seating, vaccine doses and vials with \
            waste, supplies, the anaphylaxis kit and protocol with epinephrine and \
            diphenhydramine dosing by age and weight, a layout, and a dose reconciliation sheet \
            (XLSX) in /data/output. Every assumption is shown with its source and can be \
            overridden.",
        annotations(title = "Plan a vaccination clinic", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<ClinicPlan>>()
    )]
    async fn plan_vaccination_clinic(
        &self,
        Parameters(args): Parameters<ClinicArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::vax::plan_vaccination_clinic(ctx, args)
        })
        .await
    }

    #[tool(
        name = "vaccine_confidence_conversation_guide",
        description = "Suggest a motivational-interviewing response to a vaccine concern (autism, \
            ingredients, too many shots, HPV, pregnancy, the changed federal schedule, and \
            others): an open question, a reflection, asking permission, a few cited facts from \
            CDC, AAP, and Immunize.org, and a next step, with the Flesch-Kincaid grade of the \
            script. For other languages it links authoritative translations and never machine \
            translates.",
        annotations(title = "Vaccine confidence conversation guide", read_only_hint = true, idempotent_hint = true, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<ConfidenceGuideResult>>()
    )]
    async fn vaccine_confidence_conversation_guide(
        &self,
        Parameters(args): Parameters<ConfidenceArgs>,
    ) -> CallToolResult {
        respond(phn_tools::vax::vaccine_confidence_conversation_guide(&self.ctx, args))
    }

    #[tool(
        name = "lookup_medication_information",
        description = "Reference lookup for a medication by brand or generic name: RxNorm names \
            and related brands, FDA label highlights (boxed warning, indications, \
            contraindications) with label dates and set IDs from openFDA, recalls (enforcement \
            reports), current drug shortages, and DailyMed package inserts. Reference only; it \
            does not give dosing advice for a patient.",
        annotations(title = "Look up medication information", read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for_output::<Envelope<MedicationResult>>()
    )]
    async fn lookup_medication_information(
        &self,
        Parameters(args): Parameters<MedicationArgs>,
    ) -> CallToolResult {
        respond(phn_tools::core::lookup_medication_information(&self.ctx, args).await)
    }
}
