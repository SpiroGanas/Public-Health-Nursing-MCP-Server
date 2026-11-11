//! Chemical hazard reference snapshots (ENV-08): the NIOSH Pocket Guide to
//! Chemical Hazards, the NIOSH IDLH table, ATSDR Minimal Risk Levels, and the
//! ATSDR ToxFAQs index. Built by `xtask/scripts/build_chemfood_snapshots.py`.
//!
//! These are reference tables that change slowly (the MRL table is revised a
//! few times a year), so they are bundled snapshots only; a newer copy can be
//! placed in `/cache/snapshots`.

use std::sync::OnceLock;

use phn_core::{PhnError, Result, Source};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::snapshots::SnapshotStore;

/// NIOSH Pocket Guide snapshot.
pub const NPG_SNAPSHOT_ID: &str = "niosh-npg";
pub const NPG_FILE: &str = "niosh_npg.csv";
/// NIOSH IDLH table snapshot.
pub const IDLH_SNAPSHOT_ID: &str = "niosh-idlh";
pub const IDLH_FILE: &str = "niosh_idlh.csv";
/// ATSDR MRL table snapshot.
pub const MRL_SNAPSHOT_ID: &str = "atsdr-mrls";
pub const MRL_FILE: &str = "atsdr_mrls.csv";
/// ATSDR ToxFAQs index snapshot.
pub const TOXFAQS_SNAPSHOT_ID: &str = "atsdr-toxfaqs";
pub const TOXFAQS_FILE: &str = "atsdr_toxfaqs.csv";

/// One NIOSH Pocket Guide entry (text as published).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NpgEntry {
    /// Page id, e.g. "npgd0049".
    pub id: String,
    pub name: String,
    pub synonyms: Vec<String>,
    /// CAS Registry Numbers found in the entry's CAS field.
    pub cas: Vec<String>,
    /// DOT ID and Emergency Response Guidebook guide number.
    pub dot: String,
    pub conversion: String,
    /// IDLH as printed in the Pocket Guide.
    pub idlh: String,
    /// NIOSH recommended exposure limit.
    pub rel: String,
    /// OSHA permissible exposure limit.
    pub pel: String,
    pub physical_description: String,
    pub exposure_routes: String,
    pub symptoms: String,
    pub target_organs: String,
    pub cancer_site: String,
    pub first_aid: String,
    pub url: String,
    pub last_reviewed: String,
}

/// One row of the NIOSH IDLH table (current values).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IdlhRow {
    pub substance: String,
    pub cas: Vec<String>,
    pub idlh: String,
    pub documentation_url: String,
}

/// One ATSDR minimal risk level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MrlRow {
    pub substance: String,
    /// ATSDR Toxic Substances Portal id.
    pub toxid: String,
    /// "Inh.", "Oral", or "Rad." as printed.
    pub route: String,
    /// "Acute", "Int.", or "Chr." as printed.
    pub duration: String,
    pub mrl: String,
    /// Total uncertainty and modifying factors.
    pub factors: String,
    pub endpoint: String,
    /// "Draft" (provisional) or "Final".
    pub status: String,
    pub cover_date: String,
    pub cas: Vec<String>,
}

/// One ATSDR ToxFAQs fact sheet (linked, not copied).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToxFaq {
    pub title: String,
    pub faqid: String,
    pub toxid: String,
    pub cas: Vec<String>,
    pub url: String,
    pub pdf_url: Option<String>,
}

/// The four loaded tables with their citations.
#[derive(Debug, Clone)]
pub struct ChemicalReference {
    pub npg: Vec<NpgEntry>,
    pub idlh: Vec<IdlhRow>,
    pub mrls: Vec<MrlRow>,
    pub toxfaqs: Vec<ToxFaq>,
    pub npg_source: Source,
    pub idlh_source: Source,
    pub mrl_source: Source,
    pub toxfaqs_source: Source,
}

/// CAS Registry Numbers in a text ("71-43-2", "7440-38-2 (metal)").
pub fn cas_numbers(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in text.split(|c: char| !(c.is_ascii_digit() || c == '-')) {
        let parts: Vec<&str> = raw.split('-').collect();
        if parts.len() == 3
            && (2..=7).contains(&parts[0].len())
            && parts[1].len() == 2
            && parts[2].len() == 1
            && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()))
            && !out.iter().any(|c| c == raw)
        {
            out.push(raw.to_string());
        }
    }
    out
}

/// Whether a CAS Registry Number's check digit is right.
pub fn cas_check_digit_ok(cas: &str) -> bool {
    let digits: Vec<u32> = cas.chars().filter_map(|c| c.to_digit(10)).collect();
    if digits.len() < 5 || cas_numbers(cas).len() != 1 {
        return false;
    }
    let (body, check) = digits.split_at(digits.len() - 1);
    let sum: u32 = body
        .iter()
        .rev()
        .enumerate()
        .map(|(i, d)| (i as u32 + 1) * d)
        .sum();
    sum % 10 == check[0]
}

fn rows(text: &str, what: &str) -> Result<Vec<csv::StringRecord>> {
    let mut rdr = csv::Reader::from_reader(text.as_bytes());
    rdr.records()
        .map(|r| r.map_err(|e| PhnError::Internal(format!("{what} snapshot: {e}"))))
        .collect()
}

fn field(r: &csv::StringRecord, i: usize) -> String {
    r.get(i).unwrap_or_default().trim().to_string()
}

impl ChemicalReference {
    /// Parse the bundled (or newer cached) snapshots.
    pub fn load(store: &SnapshotStore) -> Result<Self> {
        let (npg_text, npg_entry, _) = store.file_text(NPG_SNAPSHOT_ID, NPG_FILE)?;
        let npg = rows(&npg_text, "NIOSH Pocket Guide")?
            .iter()
            .map(|r| NpgEntry {
                id: field(r, 0),
                name: field(r, 1),
                synonyms: field(r, 2)
                    .split(", ")
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect(),
                cas: cas_numbers(&field(r, 3)),
                dot: field(r, 4),
                conversion: field(r, 5),
                idlh: field(r, 6),
                rel: field(r, 7),
                pel: field(r, 8),
                physical_description: field(r, 9),
                exposure_routes: field(r, 10),
                symptoms: field(r, 11),
                target_organs: field(r, 12),
                cancer_site: field(r, 13),
                first_aid: field(r, 14),
                url: field(r, 15),
                last_reviewed: field(r, 16),
            })
            .collect();
        let (idlh_text, idlh_entry, _) = store.file_text(IDLH_SNAPSHOT_ID, IDLH_FILE)?;
        let idlh = rows(&idlh_text, "NIOSH IDLH")?
            .iter()
            .map(|r| IdlhRow {
                substance: field(r, 0),
                cas: cas_numbers(&field(r, 1)),
                idlh: field(r, 2),
                documentation_url: field(r, 3),
            })
            .collect();
        let (mrl_text, mrl_entry, _) = store.file_text(MRL_SNAPSHOT_ID, MRL_FILE)?;
        let mrls = rows(&mrl_text, "ATSDR MRL")?
            .iter()
            .map(|r| MrlRow {
                substance: field(r, 0),
                toxid: field(r, 1),
                route: field(r, 2),
                duration: field(r, 3),
                mrl: field(r, 4),
                factors: field(r, 5),
                endpoint: field(r, 6),
                status: field(r, 7),
                cover_date: field(r, 8),
                cas: cas_numbers(&field(r, 9)),
            })
            .collect();
        let (tox_text, tox_entry, _) = store.file_text(TOXFAQS_SNAPSHOT_ID, TOXFAQS_FILE)?;
        let toxfaqs = rows(&tox_text, "ATSDR ToxFAQs")?
            .iter()
            .map(|r| ToxFaq {
                title: field(r, 0),
                faqid: field(r, 1),
                toxid: field(r, 2),
                cas: cas_numbers(&field(r, 3)),
                url: field(r, 4),
                pdf_url: Some(field(r, 5)).filter(|s| !s.is_empty()),
            })
            .collect();
        Ok(Self {
            npg,
            idlh,
            mrls,
            toxfaqs,
            npg_source: npg_entry.to_source(),
            idlh_source: idlh_entry.to_source(),
            mrl_source: mrl_entry.to_source(),
            toxfaqs_source: tox_entry.to_source(),
        })
    }

    /// The shared, lazily parsed tables.
    pub fn shared(store: &SnapshotStore) -> Result<&'static ChemicalReference> {
        static CELL: OnceLock<std::result::Result<ChemicalReference, String>> = OnceLock::new();
        CELL.get_or_init(|| Self::load(store).map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| PhnError::Internal(format!("chemical reference snapshots: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cas_numbers_are_found_and_checked() {
        assert_eq!(cas_numbers("71-43-2"), vec!["71-43-2"]);
        assert_eq!(
            cas_numbers("Mirex 2385-85-5; Chlordecone 143-50-5"),
            vec!["2385-85-5", "143-50-5"]
        );
        assert!(cas_numbers("HZ1800-45-T").is_empty());
        assert!(cas_numbers("1-800-222-1222").is_empty());
        assert!(cas_check_digit_ok("71-43-2"));
        assert!(cas_check_digit_ok("7732-18-5"));
        assert!(!cas_check_digit_ok("71-43-3"));
    }

    #[test]
    fn bundled_tables_load_and_join() {
        let r = ChemicalReference::load(&SnapshotStore::bundled()).unwrap();
        assert!(r.npg.len() > 600, "{}", r.npg.len());
        assert!(r.idlh.len() > 350);
        assert!(r.mrls.len() > 450);
        assert!(r.toxfaqs.len() > 180);
        let benzene = r.npg.iter().find(|e| e.name == "Benzene").unwrap();
        assert_eq!(benzene.cas, vec!["71-43-2"]);
        assert!(benzene.rel.contains("Ca"));
        assert!(!benzene.pel.is_empty());
        assert!(r.mrls.iter().any(|m| m.cas.contains(&"71-43-2".to_string())));
        assert!(
            r.toxfaqs
                .iter()
                .any(|t| t.title == "Benzene" && t.pdf_url.is_some())
        );
        // Every MRL row has a route and duration (continuation rows were merged).
        assert!(
            r.mrls
                .iter()
                .all(|m| !m.route.is_empty() && !m.duration.is_empty())
        );
    }
}
