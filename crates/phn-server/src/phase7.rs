//! Phase 7 tools: school health.

use phn_core::Envelope;
use phn_tools::school::{
    AbsenteeismArgs, AbsenteeismResult, AnnualReportArgs, AnnualReportResult,
    ChildCareChecklistArgs, ChildCareChecklistResult, ConcussionArgs, ConcussionResult,
    HealthPlanArgs, HealthPlanResult, MedicationGuideArgs, MedicationGuideResult,
    ScreeningRequirementsArgs, ScreeningRequirementsResult, TrackScreeningsArgs,
    TrackScreeningsResult,
};
use rmcp::handler::server::common::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use crate::server::{PhnServer, blocking, respond};

#[tool_router(router = phase7_router, vis = "pub(crate)")]
impl PhnServer {
    #[tool(
        name = "get_ma_school_screening_requirements",
        description = "List the school health screenings Massachusetts requires (105 CMR 200.000 and \
            M.G.L. c. 71, sections 57 and 97): vision, hearing, height/weight (BMI), postural \
            (scoliosis), SBIRT, the kindergarten vision certificate, and physical exams, by grade \
            or for one grade (ungraded classrooms by age), with DPH protocols, referral criteria, \
            parent notification, exemptions, reporting to DPH, and confidentiality rules, each \
            cited.",
        annotations(title = "MA school screening requirements", read_only_hint = true, idempotent_hint = true, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<ScreeningRequirementsResult>>()
    )]
    async fn get_ma_school_screening_requirements(
        &self,
        Parameters(args): Parameters<ScreeningRequirementsArgs>,
    ) -> CallToolResult {
        respond(phn_tools::school::get_ma_school_screening_requirements(
            &self.ctx, args,
        ))
    }

    #[tool(
        name = "track_school_screenings",
        description = "From a local roster with screening results (CSV/XLSX in /data), work out who \
            is due for each required screening this school year (vision, hearing, BMI, postural, \
            kindergarten vision certificate), compute completion, referral, and referral \
            follow-up rates by grade and BMI categories for DPH's report, write per-student results \
            and an action list to /data/output, and optionally mail-merge referral letters in each \
            family's language (agency translations in /data/templates). Returns suppressed counts \
            only; student names never reach the AI.",
        annotations(title = "Track school screenings", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<TrackScreeningsResult>>()
    )]
    async fn track_school_screenings(
        &self,
        Parameters(args): Parameters<TrackScreeningsArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::school::track_school_screenings(ctx, args)
        })
        .await
    }

    #[tool(
        name = "school_medication_administration_guide",
        description = "Answer questions about giving medications in Massachusetts public and \
            non-public schools from 105 CMR 210.000 (amended 12/5/2025) and M.G.L. c. 71, section \
            54B, cited by section: orders and parent consent, delegation to unlicensed staff, \
            field trips, self-carry, storage, documentation, medication errors, epinephrine, \
            naloxone, glucagon and other rescue medications, and DPH registration. Omit the \
            question to list topics.",
        annotations(title = "School medication administration guide", read_only_hint = true, idempotent_hint = true, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<MedicationGuideResult>>()
    )]
    async fn school_medication_administration_guide(
        &self,
        Parameters(args): Parameters<MedicationGuideArgs>,
    ) -> CallToolResult {
        respond(phn_tools::school::school_medication_administration_guide(
            &self.ctx, args,
        ))
    }

    #[tool(
        name = "build_individualized_health_plan",
        description = "Draft an individualized health plan (IHP) and an emergency action plan for a \
            student with diabetes, asthma, seizures, or food allergy/anaphylaxis from cited CDC and \
            DPH guidance: recognition, emergency steps, when to call 911, nursing assessment, \
            goals, interventions, 105 CMR 210.000 medication-plan elements, and suggested Section \
            504 accommodations. Returns the template; with student_file (a CSV/XLSX in /data) it \
            fills one plan per student in /data/output and returns counts only. Never put student \
            details in the arguments.",
        annotations(title = "Build an individualized health plan", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<HealthPlanResult>>()
    )]
    async fn build_individualized_health_plan(
        &self,
        Parameters(args): Parameters<HealthPlanArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::school::build_individualized_health_plan(ctx, args)
        })
        .await
    }

    #[tool(
        name = "concussion_return_to_learn_tracker",
        description = "Apply 105 CMR 201.000 to a student's head injury dates (no name needed): \
            removal from play, parent and athletic director/nurse notification deadlines, forms, \
            the graduated reentry plan, DPH return-to-learn stages, accommodations by symptom, \
            symptom-check schedule, medical clearance rules, and the DPH/CDC gradual return to \
            play, with flags such as clearance before full-time classes or 4 weeks without \
            recovery. With tracker_path (a head injury log in /data) it checks every record, \
            writes plans and caregiver letters to /data/output, and returns the 201.017 counts.",
        annotations(title = "Concussion return-to-learn tracker", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<ConcussionResult>>()
    )]
    async fn concussion_return_to_learn_tracker(
        &self,
        Parameters(args): Parameters<ConcussionArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::school::concussion_return_to_learn_tracker(ctx, args)
        })
        .await
    }

    #[tool(
        name = "monitor_school_absenteeism",
        description = "Spot unusual increases in influenza-like or gastrointestinal absences by \
            school (and optionally by grade, classroom, or team) from a local file of daily \
            absences with reasons, using CDC's EARS C1-C3 aberration detection, and say when and \
            how to notify the local board of health (105 CMR 300.134, DPH's school norovirus \
            guidance and online cluster form). Exact counts go to /data/output; small counts are \
            suppressed in the answer. A signal is a prompt to call, not an outbreak decision.",
        annotations(title = "Monitor school absenteeism", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<AbsenteeismResult>>()
    )]
    async fn monitor_school_absenteeism(
        &self,
        Parameters(args): Parameters<AbsenteeismArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::school::monitor_school_absenteeism(ctx, args)
        })
        .await
    }

    #[tool(
        name = "build_school_health_annual_report",
        description = "Compile a school year's health office statistics from local files (visit \
            log, medication log, screening roster): encounters by type, returned-to-class and \
            dismissal rates, 911 calls, head injuries, medication doses, screenings and \
            referrals, and BMI counts by grade, sex, and category with DPH's data-quality checks. \
            Writes a workbook laid out by DPH's published reporting categories and a school \
            committee summary to /data/output, lists the DPH reports due, and returns suppressed \
            aggregates.",
        annotations(title = "Build the school health annual report", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<AnnualReportResult>>()
    )]
    async fn build_school_health_annual_report(
        &self,
        Parameters(args): Parameters<AnnualReportArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::school::build_school_health_annual_report(ctx, args)
        })
        .await
    }

    #[tool(
        name = "child_care_health_consultation_checklist",
        description = "Checklist for child care health consultants against EEC licensing rules (606 \
            CMR 7.00) for family, small group, or large group programs: health care policy, the \
            consultant's approvals, medication administration and consent, individual health care \
            plans, immunization and health records, illness exclusion and notification, infection \
            control, first aid and emergencies, safe sleep, food allergies, and educator health, \
            each cited. Record findings to get a gap list; optionally write a visit worksheet.",
        annotations(title = "Child care health consultation checklist", read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false),
        output_schema = schema_for_output::<Envelope<ChildCareChecklistResult>>()
    )]
    async fn child_care_health_consultation_checklist(
        &self,
        Parameters(args): Parameters<ChildCareChecklistArgs>,
    ) -> CallToolResult {
        blocking(&self.ctx, move |ctx| {
            phn_tools::school::child_care_health_consultation_checklist(ctx, args)
        })
        .await
    }
}
