//! Dated data snapshots.
//!
//! Snapshots are built by `cargo xtask build-snapshots`, committed under
//! `snapshots/` with a `manifest.json` (sources, vintage, retrieval date,
//! SHA-256 checksums), and embedded in the binary. A newer copy can be placed
//! in `/cache/snapshots/` (same layout); it is used only when its manifest
//! entry is newer and every file matches its checksum.

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
    // End of Phase 8 snapshot files.
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
    }
}
