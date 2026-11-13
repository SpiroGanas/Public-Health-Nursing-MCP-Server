//! Dated data snapshots.
//!
//! Snapshots are built by `cargo xtask build-snapshots`, committed under
//! `snapshots/` with a `manifest.json` (sources, vintage, retrieval date,
//! SHA-256 checksums), and embedded in the binary. A newer copy can be placed
//! in `/cache/snapshots/` (same layout); it is used only when its manifest
//! entry is newer, lists the same files as the bundled entry, and every file
//! matches its checksum.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use phn_core::{PhnError, Result, Source, SourceMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// `snapshots/manifest.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Manifest {
    pub format_version: u32,
    pub snapshots: Vec<SnapshotEntry>,
}

/// One snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SnapshotEntry {
    pub id: String,
    pub title: String,
    pub publisher: String,
    pub source_urls: Vec<String>,
    pub source_sha256: BTreeMap<String, String>,
    pub source_vintage: String,
    pub retrieved: String,
    pub license: String,
    pub derivation: String,
    pub caveats: Vec<String>,
    pub files: Vec<SnapshotFile>,
}

/// A file within a snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SnapshotFile {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    pub rows: u64,
}

impl SnapshotEntry {
    /// A citation for responses that use this snapshot.
    pub fn to_source(&self) -> Source {
        Source {
            title: self.title.clone(),
            publisher: self.publisher.clone(),
            url: self.source_urls.first().cloned(),
            effective_date: Some(self.source_vintage.clone()),
            retrieved: Some(self.retrieved.clone()),
            mode: SourceMode::Snapshot,
        }
    }
}

/// Where a snapshot was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotOrigin {
    /// Embedded in the server binary (the release's snapshot).
    Bundled,
    /// A newer, checksum-verified copy in `/cache/snapshots`.
    CacheOverride,
}

const BUNDLED_MANIFEST: &str = include_str!("../../../snapshots/manifest.json");
const BUNDLED_FILES: &[(&str, &str)] = &[
    (
        "ma_municipalities.csv",
        include_str!("../../../snapshots/ma_municipalities.csv"),
    ),
    (
        "ma_school_districts.csv",
        include_str!("../../../snapshots/ma_school_districts.csv"),
    ),
    // Phase 3 snapshot files go below this line.
    (
        "ma_dph_guide_corpus.txt",
        include_str!("../../../snapshots/ma_dph_guide_corpus.txt"),
    ),
    // Phase 4 snapshot files go below this line.
    (
        "ma_census_geographies.csv",
        include_str!("../../../snapshots/ma_census_geographies.csv"),
    ),
    (
        "ma_acs5_indicators.csv",
        include_str!("../../../snapshots/ma_acs5_indicators.csv"),
    ),
    (
        "ma_acs5_languages_detailed.csv",
        include_str!("../../../snapshots/ma_acs5_languages_detailed.csv"),
    ),
    (
        "ma_svi_2022.csv",
        include_str!("../../../snapshots/ma_svi_2022.csv"),
    ),
    (
        "ma_places_2025.csv",
        include_str!("../../../snapshots/ma_places_2025.csv"),
    ),
    (
        "ma_places_2025_measures.csv",
        include_str!("../../../snapshots/ma_places_2025_measures.csv"),
    ),
    (
        "ma_boundaries_towns.csv",
        include_str!("../../../snapshots/ma_boundaries_towns.csv"),
    ),
    (
        "ma_boundaries_tracts.csv",
        include_str!("../../../snapshots/ma_boundaries_tracts.csv"),
    ),
    (
        "ma_boundaries_zctas.csv",
        include_str!("../../../snapshots/ma_boundaries_zctas.csv"),
    ),
    (
        "ma_boundaries_school_districts.csv",
        include_str!("../../../snapshots/ma_boundaries_school_districts.csv"),
    ),
    // Phase 5 snapshot files go below this line.
    (
        "cdc_cvx.csv",
        include_str!("../../../snapshots/cdc_cvx.csv"),
    ),
    (
        "cdc_mvx.csv",
        include_str!("../../../snapshots/cdc_mvx.csv"),
    ),
    (
        "cdc_vaccine_tradenames.csv",
        include_str!("../../../snapshots/cdc_vaccine_tradenames.csv"),
    ),
    (
        "cdc_vaccine_ndc.csv",
        include_str!("../../../snapshots/cdc_vaccine_ndc.csv"),
    ),
    (
        "ma_school_immunization_survey.csv",
        include_str!("../../../snapshots/ma_school_immunization_survey.csv"),
    ),
    (
        "nis_coverage_ma_us.csv",
        include_str!("../../../snapshots/nis_coverage_ma_us.csv"),
    ),
    // Phase 6 snapshot files go below this line.
    (
        "nndss_weekly.json",
        include_str!("../../../snapshots/nndss_weekly.json"),
    ),
    (
        "resp_nhsn.json",
        include_str!("../../../snapshots/resp_nhsn.json"),
    ),
    (
        "resp_nssp.json",
        include_str!("../../../snapshots/resp_nssp.json"),
    ),
    (
        "resp_nrevss_covid.json",
        include_str!("../../../snapshots/resp_nrevss_covid.json"),
    ),
    (
        "resp_nrevss_rsv.json",
        include_str!("../../../snapshots/resp_nrevss_rsv.json"),
    ),
    (
        "resp_nrevss_national.json",
        include_str!("../../../snapshots/resp_nrevss_national.json"),
    ),
    (
        "resp_ari.json",
        include_str!("../../../snapshots/resp_ari.json"),
    ),
    (
        "resp_wval.json",
        include_str!("../../../snapshots/resp_wval.json"),
    ),
    (
        "mwra_biobot.csv",
        include_str!("../../../snapshots/mwra_biobot.csv"),
    ),
    // Phase 10 snapshot files go below this line.
    (
        "ma_asap_directory.csv",
        include_str!("../../../snapshots/ma_asap_directory.csv"),
    ),
    (
        "ma_shine_directory.csv",
        include_str!("../../../snapshots/ma_shine_directory.csv"),
    ),
    (
        "ma_nutrition_directory.csv",
        include_str!("../../../snapshots/ma_nutrition_directory.csv"),
    ),
    (
        "ma_cbhc_directory.csv",
        include_str!("../../../snapshots/ma_cbhc_directory.csv"),
    ),
    (
        "cms_ma_nursing_homes.csv",
        include_str!("../../../snapshots/cms_ma_nursing_homes.csv"),
    ),
    (
        "cms_ma_nh_infection_citations.csv",
        include_str!("../../../snapshots/cms_ma_nh_infection_citations.csv"),
    ),
    (
        "cms_ma_nh_averages.csv",
        include_str!("../../../snapshots/cms_ma_nh_averages.csv"),
    ),
    (
        "ma_acs5_older_adults.csv",
        include_str!("../../../snapshots/ma_acs5_older_adults.csv"),
    ),
    // End of Phase 10 snapshot files.
    // Phase 8 snapshot files go below this line.
    (
        "cdc_growth_wtage.csv",
        include_str!("../../../snapshots/cdc_growth_wtage.csv"),
    ),
    (
        "cdc_growth_statage.csv",
        include_str!("../../../snapshots/cdc_growth_statage.csv"),
    ),
    (
        "cdc_growth_hcageinf.csv",
        include_str!("../../../snapshots/cdc_growth_hcageinf.csv"),
    ),
    (
        "cdc_growth_bmi_age_2022.csv",
        include_str!("../../../snapshots/cdc_growth_bmi_age_2022.csv"),
    ),
    (
        "who_growth_by_age.csv",
        include_str!("../../../snapshots/who_growth_by_age.csv"),
    ),
    (
        "who_growth_wfl.csv",
        include_str!("../../../snapshots/who_growth_wfl.csv"),
    ),
    (
        "ma_lead_screening_2024.csv",
        include_str!("../../../snapshots/ma_lead_screening_2024.csv"),
    ),
    (
        "ma_ei_programs.csv",
        include_str!("../../../snapshots/ma_ei_programs.csv"),
    ),
    (
        "ma_acs5_housing_age.csv",
        include_str!("../../../snapshots/ma_acs5_housing_age.csv"),
    ),
    (
        "ma_maternal_infant.csv",
        include_str!("../../../snapshots/ma_maternal_infant.csv"),
    ),
    // End of Phase 8 snapshot files.
    // Phase 9 snapshot files go below this line.
    (
        "hrsa_health_centers_ma.csv",
        include_str!("../../../snapshots/hrsa_health_centers_ma.csv"),
    ),
    // End of Phase 9 snapshot files.
    // Phase 12 snapshot files go below this line.
    (
        "fema_declarations_ma.csv",
        include_str!("../../../snapshots/fema_declarations_ma.csv"),
    ),
    (
        "empower_ma.csv",
        include_str!("../../../snapshots/empower_ma.csv"),
    ),
    (
        "ma_pl2020_blocks.txt",
        include_str!("../../../snapshots/ma_pl2020_blocks.txt"),
    ),
    // End of Phase 12 snapshot files.
    // Phase 11 snapshot files go below this line.
    (
        "ma_opioid_deaths_town.csv",
        include_str!("../../../snapshots/ma_opioid_deaths_town.csv"),
    ),
    (
        "cdc_vsrr_overdose_ma_us.csv",
        include_str!("../../../snapshots/cdc_vsrr_overdose_ma_us.csv"),
    ),
    (
        "cdc_vsrr_overdose_ma_counties.csv",
        include_str!("../../../snapshots/cdc_vsrr_overdose_ma_counties.csv"),
    ),
    (
        "ma_youth_survey.csv",
        include_str!("../../../snapshots/ma_youth_survey.csv"),
    ),
    (
        "ma_youth_survey_indicators.csv",
        include_str!("../../../snapshots/ma_youth_survey_indicators.csv"),
    ),
    (
        "cdc_yrbs_mental_health_ma_us.csv",
        include_str!("../../../snapshots/cdc_yrbs_mental_health_ma_us.csv"),
    ),
    (
        "findtreatment_ma.csv",
        include_str!("../../../snapshots/findtreatment_ma.csv"),
    ),
    // End of Phase 11 snapshot files.
    // Phase 14 snapshot files go below this line.
    // Phase 14 Water (ENV-03, CHR-07).
    (
        "ma_water_systems.csv",
        include_str!("../../../snapshots/ma_water_systems.csv"),
    ),
    (
        "ma_water_violations.csv",
        include_str!("../../../snapshots/ma_water_violations.csv"),
    ),
    (
        "ma_water_lead_copper.csv",
        include_str!("../../../snapshots/ma_water_lead_copper.csv"),
    ),
    (
        "sdwis_codes.csv",
        include_str!("../../../snapshots/sdwis_codes.csv"),
    ),
    (
        "ma_water_pfas6.csv",
        include_str!("../../../snapshots/ma_water_pfas6.csv"),
    ),
    (
        "ma_fluoridation_systems.csv",
        include_str!("../../../snapshots/ma_fluoridation_systems.csv"),
    ),
    (
        "ma_fluoridation_summary.csv",
        include_str!("../../../snapshots/ma_fluoridation_summary.csv"),
    ),
    (
        "ma_fluoridation_towns.csv",
        include_str!("../../../snapshots/ma_fluoridation_towns.csv"),
    ),
    // End of Phase 14 water.
    // Phase 14 EJ and climate (ENV-04, ENV-05).
    (
        "ma_ej_2020_block_groups.csv",
        include_str!("../../../snapshots/ma_ej_2020_block_groups.csv"),
    ),
    (
        "ma_ej_2020_towns.csv",
        include_str!("../../../snapshots/ma_ej_2020_towns.csv"),
    ),
    (
        "ma_heat_health_index_2024.csv",
        include_str!("../../../snapshots/ma_heat_health_index_2024.csv"),
    ),
    (
        "fema_nfip_community_status_ma.csv",
        include_str!("../../../snapshots/fema_nfip_community_status_ma.csv"),
    ),
    (
        "fema_nfip_claims_ma_towns.csv",
        include_str!("../../../snapshots/fema_nfip_claims_ma_towns.csv"),
    ),
    (
        "fema_hazard_mitigation_plans_ma.csv",
        include_str!("../../../snapshots/fema_hazard_mitigation_plans_ma.csv"),
    ),
    (
        "ma_mvp_communities.csv",
        include_str!("../../../snapshots/ma_mvp_communities.csv"),
    ),
    // End of Phase 14 ejclimate.
    // Phase 14 Vectors and housing (ENV-06, ENV-07).
    (
        "ma_arbovirus_risk.csv",
        include_str!("../../../snapshots/ma_arbovirus_risk.csv"),
    ),
    (
        "ma_arbovirus_positives.csv",
        include_str!("../../../snapshots/ma_arbovirus_positives.csv"),
    ),
    (
        "ma_tickborne_cases.csv",
        include_str!("../../../snapshots/ma_tickborne_cases.csv"),
    ),
    (
        "ma_tickborne_ed_visits.csv",
        include_str!("../../../snapshots/ma_tickborne_ed_visits.csv"),
    ),
    // End of Phase 14 vectorhousing.
    // Phase 14 Chemicals and food (ENV-08, CHR-06).
    (
        "niosh_npg.csv",
        include_str!("../../../snapshots/niosh_npg.csv"),
    ),
    (
        "niosh_idlh.csv",
        include_str!("../../../snapshots/niosh_idlh.csv"),
    ),
    (
        "atsdr_mrls.csv",
        include_str!("../../../snapshots/atsdr_mrls.csv"),
    ),
    (
        "atsdr_toxfaqs.csv",
        include_str!("../../../snapshots/atsdr_toxfaqs.csv"),
    ),
    (
        "usda_food_access_ma.csv",
        include_str!("../../../snapshots/usda_food_access_ma.csv"),
    ),
    // End of Phase 14 chemfood.
    // End of Phase 14 snapshot files.
    // Phase 15 snapshot files go below this line.
    // Phase 15 screen (CHR-02, CHR-03).
    (
        "ma_dpp_programs.csv",
        include_str!("../../../snapshots/ma_dpp_programs.csv"),
    ),
    // End of Phase 15 screen.
    // Phase 15 prevent (CHR-04, CHR-05).
    (
        "uspstf_ab_recommendations.json",
        include_str!("../../../snapshots/uspstf_ab_recommendations.json"),
    ),
    // End of Phase 15 prevent.
    // Phase 15 cha (EQ-08, EQ-09, EQ-10).
    (
        "ma_hospitals.csv",
        include_str!("../../../snapshots/ma_hospitals.csv"),
    ),
    // End of Phase 15 cha.
    // Phase 15 geo (EPI-13).
    (
        "ma_geocoder_sample.idx",
        include_str!("../../../snapshots/ma_geocoder_sample.idx"),
    ),
    (
        "ma_geocoder_crosswalk.csv",
        include_str!("../../../snapshots/ma_geocoder_crosswalk.csv"),
    ),
    // End of Phase 15 geo.
    // End of Phase 15 snapshot files.
    // Phase 16 snapshot files go below this line.
    // Phase 16 design (PLAN-01, PLAN-02, PLAN-05).
    (
        "healthy_people_2030_objectives.json",
        include_str!("../../../snapshots/healthy_people_2030_objectives.json"),
    ),
    // End of Phase 16 design.
    // Phase 16 interventions (PLAN-03, PLAN-04, PLAN-11).
    (
        "cpstf_publications.json",
        include_str!("../../../snapshots/cpstf_publications.json"),
    ),
    // End of Phase 16 interventions.
    // Phase 16 appraise (EBP-03, EBP-04).
    // End of Phase 16 appraise.
    // Phase 16 qi (EBP-05).
    // End of Phase 16 qi.
    // End of Phase 16 snapshot files.
    // Phase 17 snapshot files go below this line.
    // Phase 17 grants (PLAN-06, PLAN-07, PLAN-08).
    // End of Phase 17 grants.
    // Phase 17 budget (PLAN-09, PLAN-10).
    // End of Phase 17 budget.
    // Phase 17 finance (LEAD-01, LEAD-02).
    // End of Phase 17 finance.
    // End of Phase 17 snapshot files.
    // Phase 18 snapshot files go below this line.
    // Phase 18 strategy (LEAD-03, LEAD-04, LEAD-05).
    (
        "ma_performance_standards.json",
        include_str!("../../../snapshots/ma_performance_standards.json"),
    ),
    (
        "ma_shared_service_arrangements.csv",
        include_str!("../../../snapshots/ma_shared_service_arrangements.csv"),
    ),
    // End of Phase 18 strategy.
    // Phase 18 workforce (LEAD-06, LEAD-07, LEAD-08).
    // End of Phase 18 workforce.
    // Phase 18 ethics (EBP-07, POL-05, POL-06).
    // End of Phase 18 ethics.
    // End of Phase 18 snapshot files.
    // Phase 19 snapshot files go below this line.
    // Phase 19 tracking (POL-01, POL-03).
    // End of Phase 19 tracking.
    // Phase 19 law (POL-02, POL-04, POL-07).
    (
        "ma_law_mgl_c111.json",
        include_str!("../../../snapshots/ma_law_mgl_c111.json"),
    ),
    (
        "ma_law_105_cmr.json",
        include_str!("../../../snapshots/ma_law_105_cmr.json"),
    ),
    (
        "ma_law_244_cmr.json",
        include_str!("../../../snapshots/ma_law_244_cmr.json"),
    ),
    // End of Phase 19 law.
    // End of Phase 19 snapshot files.
];

/// Access to bundled snapshots with an optional newer copy in the cache.
#[derive(Debug, Clone)]
pub struct SnapshotStore {
    bundled: Manifest,
    override_dir: Option<PathBuf>,
}

impl SnapshotStore {
    /// The snapshots embedded in this build.
    pub fn bundled() -> Self {
        let bundled: Manifest =
            serde_json::from_str(BUNDLED_MANIFEST).expect("bundled snapshot manifest is valid");
        Self {
            bundled,
            override_dir: None,
        }
    }

    /// Also consider newer snapshots in `dir` (normally `/cache/snapshots`).
    pub fn with_override_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.override_dir = Some(dir.into());
        self
    }

    /// Every bundled manifest entry.
    pub fn bundled_entries(&self) -> &[SnapshotEntry] {
        &self.bundled.snapshots
    }

    /// The manifest entry in force for `id`, and where it comes from.
    pub fn entry(&self, id: &str) -> Option<(SnapshotEntry, SnapshotOrigin)> {
        let bundled = self.bundled.snapshots.iter().find(|e| e.id == id)?;
        if let Some(over) = self.valid_override(id, bundled) {
            return Some((over, SnapshotOrigin::CacheOverride));
        }
        Some((bundled.clone(), SnapshotOrigin::Bundled))
    }

    /// Text of one snapshot file, with the entry it belongs to.
    pub fn file_text(
        &self,
        id: &str,
        path: &str,
    ) -> Result<(Cow<'static, str>, SnapshotEntry, SnapshotOrigin)> {
        let (entry, origin) = self
            .entry(id)
            .ok_or_else(|| PhnError::NotFound(format!("snapshot {id}")))?;
        if !entry.files.iter().any(|f| f.path == path) {
            return Err(PhnError::NotFound(format!("file {path} in snapshot {id}")));
        }
        let text = match origin {
            SnapshotOrigin::Bundled => Cow::Borrowed(bundled_file(path)?),
            SnapshotOrigin::CacheOverride => {
                let dir = self.override_dir.as_ref().expect("override implies dir");
                Cow::Owned(std::fs::read_to_string(dir.join(path))?)
            }
        };
        Ok((text, entry, origin))
    }

    /// Verify that every bundled file matches its manifest checksum.
    pub fn verify_bundled(&self) -> Result<()> {
        for entry in &self.bundled.snapshots {
            for f in &entry.files {
                let text = bundled_file(&f.path)?;
                let actual = sha256_hex(text.as_bytes());
                if actual != f.sha256 {
                    return Err(PhnError::Internal(format!(
                        "bundled snapshot {} has checksum {actual}, manifest says {}",
                        f.path, f.sha256
                    )));
                }
            }
        }
        Ok(())
    }

    fn valid_override(&self, id: &str, bundled: &SnapshotEntry) -> Option<SnapshotEntry> {
        let dir = self.override_dir.as_ref()?;
        let text = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
        let manifest: Manifest = serde_json::from_str(&text).ok()?;
        let entry = manifest.snapshots.into_iter().find(|e| e.id == id)?;
        if entry.retrieved <= bundled.retrieved {
            return None;
        }
        // The override must provide exactly the files the bundled snapshot
        // has; with a missing file the tools could not load it at all.
        let paths = |e: &SnapshotEntry| {
            let mut p: Vec<String> = e.files.iter().map(|f| f.path.clone()).collect();
            p.sort();
            p
        };
        if paths(&entry) != paths(bundled) {
            tracing::warn!(
                snapshot = id,
                "ignoring cached snapshot whose file list differs from the bundled one"
            );
            return None;
        }
        let all_valid = entry.files.iter().all(|f| file_matches(dir, f));
        if !all_valid {
            tracing::warn!(snapshot = id, "ignoring cached snapshot with bad checksum");
            return None;
        }
        Some(entry)
    }
}

fn bundled_file(path: &str) -> Result<&'static str> {
    BUNDLED_FILES
        .iter()
        .find(|(p, _)| *p == path)
        .map(|(_, t)| *t)
        .ok_or_else(|| PhnError::NotFound(format!("bundled snapshot file {path}")))
}

fn file_matches(dir: &Path, f: &SnapshotFile) -> bool {
    std::fs::read(dir.join(&f.path)).is_ok_and(|b| sha256_hex(&b) == f.sha256)
}

/// Hex SHA-256.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_checksums_match_manifest() {
        SnapshotStore::bundled().verify_bundled().unwrap();
    }

    #[test]
    fn newer_valid_override_wins_and_bad_checksum_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::bundled().with_override_dir(dir.path());
        let (bundled, origin) = store.entry("ma-municipalities").unwrap();
        assert_eq!(origin, SnapshotOrigin::Bundled);

        let text = "town_id,name\n1,Test\n";
        let mut newer = bundled.clone();
        newer.retrieved = "2999-01-01".into();
        newer.files[0].sha256 = sha256_hex(text.as_bytes());
        let manifest = Manifest {
            format_version: 1,
            snapshots: vec![newer],
        };
        std::fs::write(
            dir.path().join("manifest.json"),
            serde_json::to_string(&manifest).unwrap(),
        )
        .unwrap();
        std::fs::write(dir.path().join("ma_municipalities.csv"), text).unwrap();
        let (t, _, origin) = store
            .file_text("ma-municipalities", "ma_municipalities.csv")
            .unwrap();
        assert_eq!(origin, SnapshotOrigin::CacheOverride);
        assert_eq!(t, text);

        // Tamper with the file: the override is rejected.
        std::fs::write(dir.path().join("ma_municipalities.csv"), "tampered").unwrap();
        let (_, origin) = store.entry("ma-municipalities").unwrap();
        assert_eq!(origin, SnapshotOrigin::Bundled);

        // An override that lists no files (or different files) is rejected,
        // so the bundled copy is still served.
        let mut empty = bundled.clone();
        empty.retrieved = "2999-01-01".into();
        empty.files.clear();
        let manifest = Manifest {
            format_version: 1,
            snapshots: vec![empty],
        };
        std::fs::write(
            dir.path().join("manifest.json"),
            serde_json::to_string(&manifest).unwrap(),
        )
        .unwrap();
        let (t, _, origin) = store
            .file_text("ma-municipalities", "ma_municipalities.csv")
            .unwrap();
        assert_eq!(origin, SnapshotOrigin::Bundled);
        assert_ne!(t, text);
    }
}
