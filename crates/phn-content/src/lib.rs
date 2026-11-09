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
