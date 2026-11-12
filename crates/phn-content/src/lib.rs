//! Reviewed reference content.
//!
//! Content lives in the repository's `content/` folder as YAML, each file
//! with a JSON Schema in `content/schemas/`. Every item carries review
//! metadata ([`meta::ContentMeta`]): its source, effective date, who reviewed
//! it, and when the next review is due. CI fails when a review is overdue
//! (`cargo xtask validate-content`).
//!
//! Files are embedded in the binary at build time, so the server works
//! offline and the content shipped is exactly the content that was tested.

pub mod disease_rules;
pub mod infection_control;
pub mod measles;
pub mod meta;
pub mod reportable;
pub mod sources;

pub use meta::{Citation, ContentMeta, Review, ReviewStatus};

/// A bundled content file.
#[derive(Debug, Clone, Copy)]
pub struct ContentItem {
    /// Stable identifier (matches `meta.id` in the file).
    pub id: &'static str,
    /// Path relative to the repository root.
    pub path: &'static str,
    /// MCP resource URI used to expose the file.
    pub resource_uri: &'static str,
    /// The YAML text.
    pub yaml: &'static str,
    /// The JSON Schema text.
    pub schema: &'static str,
}

/// Every bundled content item.
pub const ITEMS: &[ContentItem] = &[
    ContentItem {
        id: "ma-reportable-conditions",
        path: "content/cd/reportable_conditions.yaml",
        resource_uri: "phn://content/ma-reportable-conditions",
        yaml: include_str!("../../../content/cd/reportable_conditions.yaml"),
        schema: include_str!("../../../content/schemas/reportable_conditions.schema.json"),
    },
    ContentItem {
        id: "data-source-registry",
        path: "content/data_sources.yaml",
        resource_uri: "phn://content/data-source-registry",
        yaml: include_str!("../../../content/data_sources.yaml"),
        schema: include_str!("../../../content/schemas/data_sources.schema.json"),
    },
    // Phase 2 content items go below this line.

    // Phase 3 content items go below this line.
    ContentItem {
        id: "ma-disease-rules",
        path: "content/cd/disease_rules.yaml",
        resource_uri: "phn://content/ma-disease-rules",
        yaml: include_str!("../../../content/cd/disease_rules.yaml"),
        schema: include_str!("../../../content/schemas/disease_rules.schema.json"),
    },
    ContentItem {
        id: "ma-measles-exposure",
        path: "content/cd/measles_exposure.yaml",
        resource_uri: "phn://content/ma-measles-exposure",
        yaml: include_str!("../../../content/cd/measles_exposure.yaml"),
        schema: include_str!("../../../content/schemas/measles_exposure.schema.json"),
    },
    ContentItem {
        id: "ma-infection-control",
        path: "content/cd/infection_control.yaml",
        resource_uri: "phn://content/ma-infection-control",
        yaml: include_str!("../../../content/cd/infection_control.yaml"),
        schema: include_str!("../../../content/schemas/infection_control.schema.json"),
    },
    // Phase 4 content items go below this line.
    ContentItem {
        id: "language-access-rules",
        path: "content/equity/language_access.yaml",
        resource_uri: "phn://content/language-access-rules",
        yaml: include_str!("../../../content/equity/language_access.yaml"),
        schema: include_str!("../../../content/schemas/language_access.schema.json"),
    },
    // Phase 5 content items go below this line.
    ContentItem {
        id: "vax-schedule",
        path: "content/vax/immunization_schedule.yaml",
        resource_uri: "phn://content/vax-schedule",
        yaml: include_str!("../../../content/vax/immunization_schedule.yaml"),
        schema: include_str!("../../../content/schemas/vax_schedule.schema.json"),
    },
    ContentItem {
        id: "vax-recommendations",
        path: "content/vax/recommendations.yaml",
        resource_uri: "phn://content/vax-recommendations",
        yaml: include_str!("../../../content/vax/recommendations.yaml"),
        schema: include_str!("../../../content/schemas/vax_recommendations.schema.json"),
    },
    ContentItem {
        id: "ma-school-immunization-requirements",
        path: "content/vax/ma_school_requirements.yaml",
        resource_uri: "phn://content/ma-school-immunization-requirements",
        yaml: include_str!("../../../content/vax/ma_school_requirements.yaml"),
        schema: include_str!("../../../content/schemas/vax_school_requirements.schema.json"),
    },
    ContentItem {
        id: "vax-storage-handling",
        path: "content/vax/storage_handling.yaml",
        resource_uri: "phn://content/vax-storage-handling",
        yaml: include_str!("../../../content/vax/storage_handling.yaml"),
        schema: include_str!("../../../content/schemas/vax_storage_handling.schema.json"),
    },
    ContentItem {
        id: "vax-products",
        path: "content/vax/vaccine_products.yaml",
        resource_uri: "phn://content/vax-products",
        yaml: include_str!("../../../content/vax/vaccine_products.yaml"),
        schema: include_str!("../../../content/schemas/vax_products.schema.json"),
    },
    ContentItem {
        id: "vax-clinic-planning",
        path: "content/vax/clinic_planning.yaml",
        resource_uri: "phn://content/vax-clinic-planning",
        yaml: include_str!("../../../content/vax/clinic_planning.yaml"),
        schema: include_str!("../../../content/schemas/vax_clinic_planning.schema.json"),
    },
    ContentItem {
        id: "vax-confidence-guide",
        path: "content/vax/confidence_guide.yaml",
        resource_uri: "phn://content/vax-confidence-guide",
        yaml: include_str!("../../../content/vax/confidence_guide.yaml"),
        schema: include_str!("../../../content/schemas/vax_confidence_guide.schema.json"),
    },
    ContentItem {
        id: "vax-coverage-thresholds",
        path: "content/vax/coverage_thresholds.yaml",
        resource_uri: "phn://content/vax-coverage-thresholds",
        yaml: include_str!("../../../content/vax/coverage_thresholds.yaml"),
        schema: include_str!("../../../content/schemas/vax_coverage_thresholds.schema.json"),
    },
    // Phase 6 content items go below this line.
    ContentItem {
        id: "cdsi-supporting-data",
        path: "content/vax/cdsi_supporting_data.yaml",
        resource_uri: "phn://content/cdsi-supporting-data",
        yaml: include_str!("../../../content/vax/cdsi_supporting_data.yaml"),
        schema: include_str!("../../../content/schemas/cdsi_supporting_data.schema.json"),
    },
    // Phase 7 content items go below this line.
    ContentItem {
        id: "ma-school-screening-requirements",
        path: "content/school/screening_requirements.yaml",
        resource_uri: "phn://content/ma-school-screening-requirements",
        yaml: include_str!("../../../content/school/screening_requirements.yaml"),
        schema: include_str!("../../../content/schemas/school_screening_requirements.schema.json"),
    },
    ContentItem {
        id: "cdc-bmi-for-age",
        path: "content/school/cdc_bmi_for_age.yaml",
        resource_uri: "phn://content/cdc-bmi-for-age",
        yaml: include_str!("../../../content/school/cdc_bmi_for_age.yaml"),
        schema: include_str!("../../../content/schemas/cdc_bmi_for_age.schema.json"),
    },
    ContentItem {
        id: "ma-school-medication-administration",
        path: "content/school/medication_administration.yaml",
        resource_uri: "phn://content/ma-school-medication-administration",
        yaml: include_str!("../../../content/school/medication_administration.yaml"),
        schema: include_str!(
            "../../../content/schemas/school_medication_administration.schema.json"
        ),
    },
    ContentItem {
        id: "ma-school-concussion",
        path: "content/school/concussion.yaml",
        resource_uri: "phn://content/ma-school-concussion",
        yaml: include_str!("../../../content/school/concussion.yaml"),
        schema: include_str!("../../../content/schemas/school_concussion.schema.json"),
    },
    ContentItem {
        id: "ma-school-absenteeism",
        path: "content/school/absenteeism.yaml",
        resource_uri: "phn://content/ma-school-absenteeism",
        yaml: include_str!("../../../content/school/absenteeism.yaml"),
        schema: include_str!("../../../content/schemas/school_absenteeism.schema.json"),
    },
    ContentItem {
        id: "ma-school-health-plans",
        path: "content/school/health_plans.yaml",
        resource_uri: "phn://content/ma-school-health-plans",
        yaml: include_str!("../../../content/school/health_plans.yaml"),
        schema: include_str!("../../../content/schemas/school_health_plans.schema.json"),
    },
    ContentItem {
        id: "ma-school-annual-report",
        path: "content/school/annual_report.yaml",
        resource_uri: "phn://content/ma-school-annual-report",
        yaml: include_str!("../../../content/school/annual_report.yaml"),
        schema: include_str!("../../../content/schemas/school_annual_report.schema.json"),
    },
    ContentItem {
        id: "ma-child-care-health-checklist",
        path: "content/school/child_care_checklist.yaml",
        resource_uri: "phn://content/ma-child-care-health-checklist",
        yaml: include_str!("../../../content/school/child_care_checklist.yaml"),
        schema: include_str!("../../../content/schemas/child_care_health_checklist.schema.json"),
    },
    // End of Phase 7 content items.
    // Phase 8 content items go below this line.
    ContentItem {
        id: "mch-growth-interpretation",
        path: "content/mch/growth_interpretation.yaml",
        resource_uri: "phn://content/mch-growth-interpretation",
        yaml: include_str!("../../../content/mch/growth_interpretation.yaml"),
        schema: include_str!("../../../content/schemas/mch_growth.schema.json"),
    },
    ContentItem {
        id: "mch-lead-screening",
        path: "content/mch/lead_screening.yaml",
        resource_uri: "phn://content/mch-lead-screening",
        yaml: include_str!("../../../content/mch/lead_screening.yaml"),
        schema: include_str!("../../../content/schemas/mch_lead.schema.json"),
    },
    ContentItem {
        id: "mch-mandated-reporting",
        path: "content/mch/mandated_reporting.yaml",
        resource_uri: "phn://content/mch-mandated-reporting",
        yaml: include_str!("../../../content/mch/mandated_reporting.yaml"),
        schema: include_str!("../../../content/schemas/mch_mandated_reporting.schema.json"),
    },
    ContentItem {
        id: "mch-developmental-milestones",
        path: "content/mch/developmental_milestones.yaml",
        resource_uri: "phn://content/mch-developmental-milestones",
        yaml: include_str!("../../../content/mch/developmental_milestones.yaml"),
        schema: include_str!("../../../content/schemas/mch_milestones.schema.json"),
    },
    ContentItem {
        id: "mch-newborn-postpartum",
        path: "content/mch/newborn_postpartum.yaml",
        resource_uri: "phn://content/mch-newborn-postpartum",
        yaml: include_str!("../../../content/mch/newborn_postpartum.yaml"),
        schema: include_str!("../../../content/schemas/mch_newborn.schema.json"),
    },
    ContentItem {
        id: "equity-benefit-eligibility",
        path: "content/equity/benefit_eligibility.yaml",
        resource_uri: "phn://content/equity-benefit-eligibility",
        yaml: include_str!("../../../content/equity/benefit_eligibility.yaml"),
        schema: include_str!("../../../content/schemas/equity_benefits.schema.json"),
    },
    // End of Phase 8 content items.
    // Phase 9 content items go below this line.
    ContentItem {
        id: "tb-investigation-and-ltbi",
        path: "content/cd/tuberculosis.yaml",
        resource_uri: "phn://content/tb-investigation-and-ltbi",
        yaml: include_str!("../../../content/cd/tuberculosis.yaml"),
        schema: include_str!("../../../content/schemas/tuberculosis.schema.json"),
    },
    ContentItem {
        id: "ahc-hrsn-screening",
        path: "content/equity/ahc_hrsn.yaml",
        resource_uri: "phn://content/ahc-hrsn-screening",
        yaml: include_str!("../../../content/equity/ahc_hrsn.yaml"),
        schema: include_str!("../../../content/schemas/ahc_hrsn.schema.json"),
    },
    ContentItem {
        id: "omaha-system",
        path: "content/core/omaha_system.yaml",
        resource_uri: "phn://content/omaha-system",
        yaml: include_str!("../../../content/core/omaha_system.yaml"),
        schema: include_str!("../../../content/schemas/omaha_system.schema.json"),
    },
    ContentItem {
        id: "public-health-intervention-wheel",
        path: "content/core/intervention_wheel.yaml",
        resource_uri: "phn://content/public-health-intervention-wheel",
        yaml: include_str!("../../../content/core/intervention_wheel.yaml"),
        schema: include_str!("../../../content/schemas/intervention_wheel.schema.json"),
    },
    ContentItem {
        id: "nursing-documentation-templates",
        path: "content/core/documentation_templates.yaml",
        resource_uri: "phn://content/nursing-documentation-templates",
        yaml: include_str!("../../../content/core/documentation_templates.yaml"),
        schema: include_str!("../../../content/schemas/documentation_templates.schema.json"),
    },
    ContentItem {
        id: "newcomer-health",
        path: "content/equity/newcomer_health.yaml",
        resource_uri: "phn://content/newcomer-health",
        yaml: include_str!("../../../content/equity/newcomer_health.yaml"),
        schema: include_str!("../../../content/schemas/newcomer_health.schema.json"),
    },
    // End of Phase 9 content items.
    // Phase 10 content items go below this line.
    ContentItem {
        id: "bh-mental-health-screening",
        path: "content/bh/mental_health_screening.yaml",
        resource_uri: "phn://content/bh-mental-health-screening",
        yaml: include_str!("../../../content/bh/mental_health_screening.yaml"),
        schema: include_str!("../../../content/schemas/bh_mental_health_screening.schema.json"),
    },
    ContentItem {
        id: "bh-crisis-resources",
        path: "content/bh/crisis_resources.yaml",
        resource_uri: "phn://content/bh-crisis-resources",
        yaml: include_str!("../../../content/bh/crisis_resources.yaml"),
        schema: include_str!("../../../content/schemas/bh_crisis_resources.schema.json"),
    },
    ContentItem {
        id: "aging-steadi-falls",
        path: "content/aging/steadi_falls.yaml",
        resource_uri: "phn://content/aging-steadi-falls",
        yaml: include_str!("../../../content/aging/steadi_falls.yaml"),
        schema: include_str!("../../../content/schemas/aging_steadi_falls.schema.json"),
    },
    ContentItem {
        id: "aging-protective-services",
        path: "content/aging/protective_services.yaml",
        resource_uri: "phn://content/aging-protective-services",
        yaml: include_str!("../../../content/aging/protective_services.yaml"),
        schema: include_str!("../../../content/schemas/aging_protective_services.schema.json"),
    },
    ContentItem {
        id: "aging-services-guide",
        path: "content/aging/aging_services.yaml",
        resource_uri: "phn://content/aging-services-guide",
        yaml: include_str!("../../../content/aging/aging_services.yaml"),
        schema: include_str!("../../../content/schemas/aging_services.schema.json"),
    },
    ContentItem {
        id: "aging-coverage-options",
        path: "content/aging/coverage_options.yaml",
        resource_uri: "phn://content/aging-coverage-options",
        yaml: include_str!("../../../content/aging/coverage_options.yaml"),
        schema: include_str!("../../../content/schemas/aging_coverage_options.schema.json"),
    },
    ContentItem {
        id: "aging-nursing-home-guide",
        path: "content/aging/nursing_home_guide.yaml",
        resource_uri: "phn://content/aging-nursing-home-guide",
        yaml: include_str!("../../../content/aging/nursing_home_guide.yaml"),
        schema: include_str!("../../../content/schemas/aging_nursing_home_guide.schema.json"),
    },
    ContentItem {
        id: "aging-advance-care-planning",
        path: "content/aging/advance_care_planning.yaml",
        resource_uri: "phn://content/aging-advance-care-planning",
        yaml: include_str!("../../../content/aging/advance_care_planning.yaml"),
        schema: include_str!("../../../content/schemas/aging_advance_care_planning.schema.json"),
    },
    // End of Phase 10 content items.
    // Phase 11 content items go below this line.
    ContentItem {
        id: "comms-readability",
        path: "content/comms/readability.yaml",
        resource_uri: "phn://content/comms-readability",
        yaml: include_str!("../../../content/comms/readability.yaml"),
        schema: include_str!("../../../content/schemas/comms_readability.schema.json"),
    },
    ContentItem {
        id: "bh-overdose-data",
        path: "content/bh/overdose_data.yaml",
        resource_uri: "phn://content/bh-overdose-data",
        yaml: include_str!("../../../content/bh/overdose_data.yaml"),
        schema: include_str!("../../../content/schemas/bh_overdose_data.schema.json"),
    },
    ContentItem {
        id: "bh-youth-survey",
        path: "content/bh/youth_survey.yaml",
        resource_uri: "phn://content/bh-youth-survey",
        yaml: include_str!("../../../content/bh/youth_survey.yaml"),
        schema: include_str!("../../../content/schemas/bh_youth_survey.schema.json"),
    },
    ContentItem {
        id: "bh-substance-use-resources",
        path: "content/bh/substance_use_resources.yaml",
        resource_uri: "phn://content/bh-substance-use-resources",
        yaml: include_str!("../../../content/bh/substance_use_resources.yaml"),
        schema: include_str!("../../../content/schemas/bh_substance_use_resources.schema.json"),
    },
    ContentItem {
        id: "comms-safe-messaging",
        path: "content/comms/safe_messaging.yaml",
        resource_uri: "phn://content/comms-safe-messaging",
        yaml: include_str!("../../../content/comms/safe_messaging.yaml"),
        schema: include_str!("../../../content/schemas/comms_safe_messaging.schema.json"),
    },
    ContentItem {
        id: "bh-school-postvention",
        path: "content/bh/postvention.yaml",
        resource_uri: "phn://content/bh-school-postvention",
        yaml: include_str!("../../../content/bh/postvention.yaml"),
        schema: include_str!("../../../content/schemas/bh_postvention.schema.json"),
    },
    ContentItem {
        id: "comms-translation",
        path: "content/comms/translation.yaml",
        resource_uri: "phn://content/comms-translation",
        yaml: include_str!("../../../content/comms/translation.yaml"),
        schema: include_str!("../../../content/schemas/comms_translation.schema.json"),
    },
    ContentItem {
        id: "comms-campaigns",
        path: "content/comms/campaigns.yaml",
        resource_uri: "phn://content/comms-campaigns",
        yaml: include_str!("../../../content/comms/campaigns.yaml"),
        schema: include_str!("../../../content/schemas/comms_campaigns.schema.json"),
    },
    // End of Phase 11 content items.
    // Phase 12 content items go below this line.
    ContentItem {
        id: "epr-hazard-health-actions",
        path: "content/epr/hazard_health_actions.yaml",
        resource_uri: "phn://content/epr-hazard-health-actions",
        yaml: include_str!("../../../content/epr/hazard_health_actions.yaml"),
        schema: include_str!("../../../content/schemas/epr_hazard_health_actions.schema.json"),
    },
    ContentItem {
        id: "epr-dispensing-site",
        path: "content/epr/dispensing.yaml",
        resource_uri: "phn://content/epr-dispensing-site",
        yaml: include_str!("../../../content/epr/dispensing.yaml"),
        schema: include_str!("../../../content/schemas/epr_dispensing.schema.json"),
    },
    ContentItem {
        id: "env-air-quality",
        path: "content/env/air_quality.yaml",
        resource_uri: "phn://content/env-air-quality",
        yaml: include_str!("../../../content/env/air_quality.yaml"),
        schema: include_str!("../../../content/schemas/env_air_quality.schema.json"),
    },
    ContentItem {
        id: "env-heat-health",
        path: "content/env/heat.yaml",
        resource_uri: "phn://content/env-heat-health",
        yaml: include_str!("../../../content/env/heat.yaml"),
        schema: include_str!("../../../content/schemas/env_heat.schema.json"),
    },
    ContentItem {
        id: "epr-hazard-vulnerability",
        path: "content/epr/hva.yaml",
        resource_uri: "phn://content/epr-hazard-vulnerability",
        yaml: include_str!("../../../content/epr/hva.yaml"),
        schema: include_str!("../../../content/schemas/epr_hva.schema.json"),
    },
    ContentItem {
        id: "epr-functional-needs-shelter",
        path: "content/epr/functional_needs_shelter.yaml",
        resource_uri: "phn://content/epr-functional-needs-shelter",
        yaml: include_str!("../../../content/epr/functional_needs_shelter.yaml"),
        schema: include_str!("../../../content/schemas/epr_functional_needs_shelter.schema.json"),
    },
    ContentItem {
        id: "epr-electricity-dependence",
        path: "content/epr/electricity_dependence.yaml",
        resource_uri: "phn://content/epr-electricity-dependence",
        yaml: include_str!("../../../content/epr/electricity_dependence.yaml"),
        schema: include_str!("../../../content/schemas/epr_electricity_dependence.schema.json"),
    },
    ContentItem {
        id: "epr-casper",
        path: "content/epr/casper.yaml",
        resource_uri: "phn://content/epr-casper",
        yaml: include_str!("../../../content/epr/casper.yaml"),
        schema: include_str!("../../../content/schemas/epr_casper.schema.json"),
    },
    // End of Phase 12 content items.
    // Phase 13 content items go below this line (EPR-01, EPR-07, EPR-08/09, EPR-10).
    // Phase 13 EPR-01 (ICS).
    ContentItem {
        id: "epr-ics-forms",
        path: "content/epr/ics_forms.yaml",
        resource_uri: "phn://content/epr-ics-forms",
        yaml: include_str!("../../../content/epr/ics_forms.yaml"),
        schema: include_str!("../../../content/schemas/epr_ics_forms.schema.json"),
    },
    // Phase 13 EPR-07 (COOP).
    ContentItem {
        id: "epr-continuity",
        path: "content/epr/continuity.yaml",
        resource_uri: "phn://content/epr-continuity",
        yaml: include_str!("../../../content/epr/continuity.yaml"),
        schema: include_str!("../../../content/schemas/epr_continuity.schema.json"),
    },
    // Phase 13 EPR-10 (CERC).
    ContentItem {
        id: "epr-cerc",
        path: "content/epr/cerc.yaml",
        resource_uri: "phn://content/epr-cerc",
        yaml: include_str!("../../../content/epr/cerc.yaml"),
        schema: include_str!("../../../content/schemas/epr_cerc.schema.json"),
    },
    // Phase 13 EPR-08/EPR-09 (HSEEP).
    ContentItem {
        id: "epr-capabilities",
        path: "content/epr/capabilities.yaml",
        resource_uri: "phn://content/epr-capabilities",
        yaml: include_str!("../../../content/epr/capabilities.yaml"),
        schema: include_str!("../../../content/schemas/epr_capabilities.schema.json"),
    },
    ContentItem {
        id: "epr-hseep",
        path: "content/epr/hseep.yaml",
        resource_uri: "phn://content/epr-hseep",
        yaml: include_str!("../../../content/epr/hseep.yaml"),
        schema: include_str!("../../../content/schemas/epr_hseep.schema.json"),
    },
    ContentItem {
        id: "epr-exercise-scenarios",
        path: "content/epr/exercise_scenarios.yaml",
        resource_uri: "phn://content/epr-exercise-scenarios",
        yaml: include_str!("../../../content/epr/exercise_scenarios.yaml"),
        schema: include_str!("../../../content/schemas/epr_exercise_scenarios.schema.json"),
    },
    // End of Phase 13 content items.
    // Phase 14 content items go below this line.
    // Phase 14 Water (ENV-03, CHR-07).
    ContentItem {
        id: "env-drinking-water",
        path: "content/env/drinking_water.yaml",
        resource_uri: "phn://content/env-drinking-water",
        yaml: include_str!("../../../content/env/drinking_water.yaml"),
        schema: include_str!("../../../content/schemas/env_drinking_water.schema.json"),
    },
    ContentItem {
        id: "chronic-fluoridation",
        path: "content/chronic/fluoridation.yaml",
        resource_uri: "phn://content/chronic-fluoridation",
        yaml: include_str!("../../../content/chronic/fluoridation.yaml"),
        schema: include_str!("../../../content/schemas/chronic_fluoridation.schema.json"),
    },
    // End of Phase 14 water.
    // Phase 14 EJ and climate (ENV-04, ENV-05).
    ContentItem {
        id: "env-environmental-justice",
        path: "content/env/environmental_justice.yaml",
        resource_uri: "phn://content/env-environmental-justice",
        yaml: include_str!("../../../content/env/environmental_justice.yaml"),
        schema: include_str!("../../../content/schemas/env_environmental_justice.schema.json"),
    },
    ContentItem {
        id: "env-climate-vulnerability",
        path: "content/env/climate_vulnerability.yaml",
        resource_uri: "phn://content/env-climate-vulnerability",
        yaml: include_str!("../../../content/env/climate_vulnerability.yaml"),
        schema: include_str!("../../../content/schemas/env_climate_vulnerability.schema.json"),
    },
    // End of Phase 14 ejclimate.
    // Phase 14 Vectors and housing (ENV-06, ENV-07).
    ContentItem {
        id: "env-vector-borne",
        path: "content/env/vector_borne.yaml",
        resource_uri: "phn://content/env-vector-borne",
        yaml: include_str!("../../../content/env/vector_borne.yaml"),
        schema: include_str!("../../../content/schemas/env_vector_borne.schema.json"),
    },
    ContentItem {
        id: "env-housing-conditions",
        path: "content/env/housing_conditions.yaml",
        resource_uri: "phn://content/env-housing-conditions",
        yaml: include_str!("../../../content/env/housing_conditions.yaml"),
        schema: include_str!("../../../content/schemas/env_housing_conditions.schema.json"),
    },
    // End of Phase 14 vectorhousing.
    // Phase 14 Chemicals and food (ENV-08, CHR-06).
    ContentItem {
        id: "env-chemical-exposure",
        path: "content/env/chemical_exposure.yaml",
        resource_uri: "phn://content/env-chemical-exposure",
        yaml: include_str!("../../../content/env/chemical_exposure.yaml"),
        schema: include_str!("../../../content/schemas/env_chemical_exposure.schema.json"),
    },
    ContentItem {
        id: "chronic-food-access",
        path: "content/chronic/food_access.yaml",
        resource_uri: "phn://content/chronic-food-access",
        yaml: include_str!("../../../content/chronic/food_access.yaml"),
        schema: include_str!("../../../content/schemas/chronic_food_access.schema.json"),
    },
    // End of Phase 14 chemfood.
    // End of Phase 14 content items.
    // Phase 15 content items go below this line.
    // Phase 15 screen (CHR-02, CHR-03).
    ContentItem {
        id: "chronic-bp-screening",
        path: "content/chronic/bp_screening.yaml",
        resource_uri: "phn://content/chronic-bp-screening",
        yaml: include_str!("../../../content/chronic/bp_screening.yaml"),
        schema: include_str!("../../../content/schemas/chronic_bp_screening.schema.json"),
    },
    ContentItem {
        id: "chronic-diabetes-prevention",
        path: "content/chronic/diabetes_prevention.yaml",
        resource_uri: "phn://content/chronic-diabetes-prevention",
        yaml: include_str!("../../../content/chronic/diabetes_prevention.yaml"),
        schema: include_str!("../../../content/schemas/chronic_diabetes_prevention.schema.json"),
    },
    // End of Phase 15 screen.
    // Phase 15 prevent (CHR-04, CHR-05).
    ContentItem {
        id: "chronic-preventive-services",
        path: "content/chronic/preventive_services.yaml",
        resource_uri: "phn://content/chronic-preventive-services",
        yaml: include_str!("../../../content/chronic/preventive_services.yaml"),
        schema: include_str!("../../../content/schemas/chronic_preventive_services.schema.json"),
    },
    ContentItem {
        id: "chronic-asthma-home",
        path: "content/chronic/asthma_home.yaml",
        resource_uri: "phn://content/chronic-asthma-home",
        yaml: include_str!("../../../content/chronic/asthma_home.yaml"),
        schema: include_str!("../../../content/schemas/chronic_asthma_home.schema.json"),
    },
    // End of Phase 15 prevent.
    // Phase 15 cha (EQ-08, EQ-09, EQ-10).
    ContentItem {
        id: "equity-prioritization",
        path: "content/equity/prioritization.yaml",
        resource_uri: "phn://content/equity-prioritization",
        yaml: include_str!("../../../content/equity/prioritization.yaml"),
        schema: include_str!("../../../content/schemas/equity_prioritization.schema.json"),
    },
    ContentItem {
        id: "equity-qualitative-codebook",
        path: "content/equity/qualitative_codebook.yaml",
        resource_uri: "phn://content/equity-qualitative-codebook",
        yaml: include_str!("../../../content/equity/qualitative_codebook.yaml"),
        schema: include_str!("../../../content/schemas/equity_qualitative_codebook.schema.json"),
    },
    ContentItem {
        id: "equity-cha-framework",
        path: "content/equity/cha_framework.yaml",
        resource_uri: "phn://content/equity-cha-framework",
        yaml: include_str!("../../../content/equity/cha_framework.yaml"),
        schema: include_str!("../../../content/schemas/equity_cha_framework.schema.json"),
    },
    // End of Phase 15 cha.
    // Phase 15 geo (EPI-13).
    // End of Phase 15 geo.
    // End of Phase 15 content items.
    // Phase 16 content items go below this line.
    // Phase 16 design (PLAN-01, PLAN-02, PLAN-05).
    ContentItem {
        id: "planning-logic-model",
        path: "content/planning/logic_model.yaml",
        resource_uri: "phn://content/planning-logic-model",
        yaml: include_str!("../../../content/planning/logic_model.yaml"),
        schema: include_str!("../../../content/schemas/planning_logic_model.schema.json"),
    },
    ContentItem {
        id: "planning-smartie",
        path: "content/planning/smartie.yaml",
        resource_uri: "phn://content/planning-smartie",
        yaml: include_str!("../../../content/planning/smartie.yaml"),
        schema: include_str!("../../../content/schemas/planning_smartie.schema.json"),
    },
    ContentItem {
        id: "planning-evaluation-framework",
        path: "content/planning/evaluation_framework.yaml",
        resource_uri: "phn://content/planning-evaluation-framework",
        yaml: include_str!("../../../content/planning/evaluation_framework.yaml"),
        schema: include_str!("../../../content/schemas/planning_evaluation_framework.schema.json"),
    },
    // End of Phase 16 design.
    // Phase 16 interventions (PLAN-03, PLAN-04, PLAN-11).
    ContentItem {
        id: "planning-behavior-theories",
        path: "content/planning/behavior_theories.yaml",
        resource_uri: "phn://content/planning-behavior-theories",
        yaml: include_str!("../../../content/planning/behavior_theories.yaml"),
        schema: include_str!("../../../content/schemas/planning_behavior_theories.schema.json"),
    },
    ContentItem {
        id: "planning-evidence-sources",
        path: "content/planning/evidence_sources.yaml",
        resource_uri: "phn://content/planning-evidence-sources",
        yaml: include_str!("../../../content/planning/evidence_sources.yaml"),
        schema: include_str!("../../../content/schemas/planning_evidence_sources.schema.json"),
    },
    // End of Phase 16 interventions.
    // Phase 16 appraise (EBP-03, EBP-04).
    ContentItem {
        id: "ebp-appraisal-checklists",
        path: "content/ebp/appraisal_checklists.yaml",
        resource_uri: "phn://content/ebp-appraisal-checklists",
        yaml: include_str!("../../../content/ebp/appraisal_checklists.yaml"),
        schema: include_str!("../../../content/schemas/ebp_appraisal_checklists.schema.json"),
    },
    ContentItem {
        id: "ebp-grade-certainty",
        path: "content/ebp/grade_certainty.yaml",
        resource_uri: "phn://content/ebp-grade-certainty",
        yaml: include_str!("../../../content/ebp/grade_certainty.yaml"),
        schema: include_str!("../../../content/schemas/ebp_grade_certainty.schema.json"),
    },
    ContentItem {
        id: "ebp-evidence-synthesis",
        path: "content/ebp/evidence_synthesis.yaml",
        resource_uri: "phn://content/ebp-evidence-synthesis",
        yaml: include_str!("../../../content/ebp/evidence_synthesis.yaml"),
        schema: include_str!("../../../content/schemas/ebp_evidence_synthesis.schema.json"),
    },
    // End of Phase 16 appraise.
    // Phase 16 qi (EBP-05).
    ContentItem {
        id: "ebp-quality-improvement",
        path: "content/ebp/quality_improvement.yaml",
        resource_uri: "phn://content/ebp-quality-improvement",
        yaml: include_str!("../../../content/ebp/quality_improvement.yaml"),
        schema: include_str!("../../../content/schemas/ebp_quality_improvement.schema.json"),
    },
    // End of Phase 16 qi.
    // End of Phase 16 content items.
    // Phase 17 content items go below this line.
    // Phase 17 grants (PLAN-06, PLAN-07, PLAN-08).
    // End of Phase 17 grants.
    // Phase 17 budget (PLAN-09, PLAN-10).
    ContentItem {
        id: "planning-budget-reference",
        path: "content/planning/budget_reference.yaml",
        resource_uri: "phn://content/planning-budget-reference",
        yaml: include_str!("../../../content/planning/budget_reference.yaml"),
        schema: include_str!("../../../content/schemas/planning_budget_reference.schema.json"),
    },
    ContentItem {
        id: "planning-economic-evaluation",
        path: "content/planning/economic_evaluation.yaml",
        resource_uri: "phn://content/planning-economic-evaluation",
        yaml: include_str!("../../../content/planning/economic_evaluation.yaml"),
        schema: include_str!("../../../content/schemas/planning_economic_evaluation.schema.json"),
    },
    // End of Phase 17 budget.
    // Phase 17 finance (LEAD-01, LEAD-02).
    ContentItem {
        id: "leadership-department-finance",
        path: "content/leadership/department_finance.yaml",
        resource_uri: "phn://content/leadership-department-finance",
        yaml: include_str!("../../../content/leadership/department_finance.yaml"),
        schema: include_str!("../../../content/schemas/leadership_department_finance.schema.json"),
    },
    ContentItem {
        id: "leadership-staffing-model",
        path: "content/leadership/staffing_model.yaml",
        resource_uri: "phn://content/leadership-staffing-model",
        yaml: include_str!("../../../content/leadership/staffing_model.yaml"),
        schema: include_str!("../../../content/schemas/leadership_staffing_model.schema.json"),
    },
    // End of Phase 17 finance.
    // End of Phase 17 content items.
    // Phase 18 content items go below this line.
    // Phase 18 strategy (LEAD-03, LEAD-04, LEAD-05).
    ContentItem {
        id: "leadership-fphs-framework",
        path: "content/leadership/fphs_framework.yaml",
        resource_uri: "phn://content/leadership-fphs-framework",
        yaml: include_str!("../../../content/leadership/fphs_framework.yaml"),
        schema: include_str!("../../../content/schemas/leadership_fphs_framework.schema.json"),
    },
    ContentItem {
        id: "leadership-phab-standards",
        path: "content/leadership/phab_standards.yaml",
        resource_uri: "phn://content/leadership-phab-standards",
        yaml: include_str!("../../../content/leadership/phab_standards.yaml"),
        schema: include_str!("../../../content/schemas/leadership_phab_standards.schema.json"),
    },
    ContentItem {
        id: "leadership-strategic-planning",
        path: "content/leadership/strategic_planning.yaml",
        resource_uri: "phn://content/leadership-strategic-planning",
        yaml: include_str!("../../../content/leadership/strategic_planning.yaml"),
        schema: include_str!("../../../content/schemas/leadership_strategic_planning.schema.json"),
    },
    // End of Phase 18 strategy.
    // Phase 18 workforce (LEAD-06, LEAD-07, LEAD-08).
    // End of Phase 18 workforce.
    // Phase 18 ethics (EBP-07, POL-05, POL-06).
    ContentItem {
        id: "ebp-qi-research-determination",
        path: "content/ebp/qi_research_determination.yaml",
        resource_uri: "phn://content/ebp-qi-research-determination",
        yaml: include_str!("../../../content/ebp/qi_research_determination.yaml"),
        schema: include_str!("../../../content/schemas/ebp_qi_research_determination.schema.json"),
    },
    ContentItem {
        id: "policy-public-health-ethics",
        path: "content/policy/public_health_ethics.yaml",
        resource_uri: "phn://content/policy-public-health-ethics",
        yaml: include_str!("../../../content/policy/public_health_ethics.yaml"),
        schema: include_str!("../../../content/schemas/policy_public_health_ethics.schema.json"),
    },
    ContentItem {
        id: "policy-nursing-scope-of-practice",
        path: "content/policy/nursing_scope_of_practice.yaml",
        resource_uri: "phn://content/policy-nursing-scope-of-practice",
        yaml: include_str!("../../../content/policy/nursing_scope_of_practice.yaml"),
        schema: include_str!(
            "../../../content/schemas/policy_nursing_scope_of_practice.schema.json"
        ),
    },
    // End of Phase 18 ethics.
    // End of Phase 18 content items.
    // Phase 19 content items go below this line.
    // Phase 19 tracking (POL-01, POL-03).
    // End of Phase 19 tracking.
    // Phase 19 law (POL-02, POL-04, POL-07).
    // End of Phase 19 law.
    // End of Phase 19 content items.
];

/// Parse a YAML document into a JSON value (for schema validation).
pub fn yaml_to_json(yaml: &str) -> Result<serde_json::Value, String> {
    serde_saphyr::from_str::<serde_json::Value>(yaml).map_err(|e| e.to_string())
}

/// Read the `meta` block of a content item.
pub fn item_meta(item: &ContentItem) -> Result<ContentMeta, String> {
    #[derive(serde::Deserialize)]
    struct OnlyMeta {
        meta: ContentMeta,
    }
    let v = yaml_to_json(item.yaml)?;
    serde_json::from_value::<OnlyMeta>(v)
        .map(|m| m.meta)
        .map_err(|e| format!("{}: {e}", item.path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_item_matches_its_schema() {
        for item in ITEMS {
            let schema: serde_json::Value = serde_json::from_str(item.schema).unwrap();
            let validator = jsonschema::validator_for(&schema).unwrap();
            let doc = yaml_to_json(item.yaml).unwrap();
            let errors: Vec<String> = validator
                .iter_errors(&doc)
                .map(|e| format!("{} at {}", e, e.instance_path()))
                .collect();
            assert!(errors.is_empty(), "{}: {errors:#?}", item.path);
            let meta = item_meta(item).unwrap();
            assert_eq!(meta.id, item.id);
        }
    }
}
