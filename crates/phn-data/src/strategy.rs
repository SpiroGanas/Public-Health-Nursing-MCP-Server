//! Data for `assess_foundational_public_health_services` (LEAD-03).
//!
//! * The bundled `ma-performance-standards` snapshot: the mandated local public health
//!   responsibilities and the workforce standards from DPH's "Performance Standards for Local
//!   Public Health" workbook, read by `xtask/scripts/build_strategy_snapshots.py`.
//! * The bundled `ma-shared-service-arrangements` snapshot: which shared service arrangement,
//!   lead entity, and training hub each municipality is listed in (DPH's list, updated April 2026).
//!
//! There is no live source: both are Excel files on mass.gov that change a few times a year, so
//! the snapshot is the only mode and the tool says how old it is.

use phn_core::{PhnError, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::snapshots::{SnapshotEntry, SnapshotStore};

pub const PERFORMANCE_ID: &str = "ma-performance-standards";
pub const PERFORMANCE_FILE: &str = "ma_performance_standards.json";
pub const SSA_ID: &str = "ma-shared-service-arrangements";
pub const SSA_FILE: &str = "ma_shared_service_arrangements.csv";

/// One mandated local responsibility (a row of DPH's workbook).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Mandate {
    /// This project's ID (PS-ENV-012), not a DPH identifier.
    pub id: String,
    /// Topic area in the workbook (Food Protection, Disease Reporting, ...).
    pub area: String,
    /// Statute or regulation cited.
    pub citation: String,
    /// DPH's one-line summary (may be empty).
    pub summary: String,
    /// State program that supports the topic (name only).
    pub state_program: String,
    /// The workbook marks the standard with * (OLRH has comments that it is outdated).
    pub marked_outdated: bool,
}

/// A workforce standard by position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WorkforcePosition {
    pub id: String,
    pub position: String,
    pub required_at_hire: Vec<String>,
    pub required_after_hire: Vec<String>,
    pub recommended_after_hire: Vec<String>,
}

/// A workforce standard by inspection type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WorkforceInspection {
    pub id: String,
    pub inspection_type: String,
    pub required: Vec<String>,
    pub recommended: Vec<String>,
}

/// The bundled performance standards file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PerformanceStandards {
    pub format_version: u32,
    pub description: String,
    pub retrieved: String,
    pub source_last_modified: String,
    pub not_included: Vec<String>,
    pub mandates: Vec<Mandate>,
    pub workforce_positions: Vec<WorkforcePosition>,
    pub workforce_inspection_types: Vec<WorkforceInspection>,
    pub workforce_notes: Vec<String>,
}

/// One municipality's row in DPH's shared service arrangement list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SsaRow {
    pub municipality: String,
    /// Blank when the list shows no arrangement.
    pub arrangement: String,
    pub lead_entity: String,
    pub training_hub: String,
}

/// Parse and check the performance standards file.
pub fn parse_performance(text: &str) -> Result<PerformanceStandards> {
    let p: PerformanceStandards = serde_json::from_str(text)
        .map_err(|e| PhnError::Internal(format!("performance standards snapshot: {e}")))?;
    let mut seen = std::collections::BTreeSet::new();
    for id in p
        .mandates
        .iter()
        .map(|m| m.id.as_str())
        .chain(p.workforce_positions.iter().map(|w| w.id.as_str()))
        .chain(p.workforce_inspection_types.iter().map(|w| w.id.as_str()))
    {
        if !seen.insert(id) {
            return Err(PhnError::Internal(format!(
                "performance standards snapshot: ID {id} appears twice"
            )));
        }
    }
    if p.mandates.is_empty() {
        return Err(PhnError::Internal(
            "performance standards snapshot has no mandates".into(),
        ));
    }
    Ok(p)
}

/// Parse the shared service arrangement list.
pub fn parse_ssa(text: &str) -> Result<Vec<SsaRow>> {
    let mut rdr = csv::Reader::from_reader(text.as_bytes());
    let mut out = Vec::new();
    for rec in rdr.deserialize::<SsaRow>() {
        out.push(
            rec.map_err(|e| PhnError::Internal(format!("shared service arrangement snapshot: {e}")))?,
        );
    }
    if out.is_empty() {
        return Err(PhnError::Internal(
            "shared service arrangement snapshot is empty".into(),
        ));
    }
    Ok(out)
}

/// Load the bundled (or newer cached) performance standards with their manifest entry.
pub fn load_performance(store: &SnapshotStore) -> Result<(PerformanceStandards, SnapshotEntry)> {
    let (text, entry, _) = store.file_text(PERFORMANCE_ID, PERFORMANCE_FILE)?;
    Ok((parse_performance(&text)?, entry))
}

/// Load the bundled (or newer cached) shared service arrangement list with its manifest entry.
pub fn load_ssa(store: &SnapshotStore) -> Result<(Vec<SsaRow>, SnapshotEntry)> {
    let (text, entry, _) = store.file_text(SSA_ID, SSA_FILE)?;
    Ok((parse_ssa(&text)?, entry))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_snapshots_parse_and_are_consistent() {
        let store = SnapshotStore::bundled();
        let (p, entry) = load_performance(&store).unwrap();
        assert_eq!(entry.id, PERFORMANCE_ID);
        assert_eq!(p.mandates.len(), 93);
        assert_eq!(p.workforce_positions.len(), 6);
        assert_eq!(p.workforce_inspection_types.len(), 7);
        assert!(p.mandates.iter().any(|m| m.citation == "105 CMR 300"));
        let (ssa, entry) = load_ssa(&store).unwrap();
        assert_eq!(entry.id, SSA_ID);
        assert_eq!(ssa.len(), 351);
        let amherst = ssa.iter().find(|r| r.municipality == "Amherst").unwrap();
        assert!(!amherst.arrangement.is_empty());
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let text = r#"{"format_version":1,"description":"d","retrieved":"2026-11-12",
            "source_last_modified":"","not_included":[],
            "mandates":[{"id":"PS-ADM-001","area":"a","citation":"c","summary":"s","state_program":"","marked_outdated":false},
                        {"id":"PS-ADM-001","area":"a","citation":"c","summary":"s","state_program":"","marked_outdated":false}],
            "workforce_positions":[],"workforce_inspection_types":[],"workforce_notes":[]}"#;
        assert!(parse_performance(text).is_err());
    }
}
