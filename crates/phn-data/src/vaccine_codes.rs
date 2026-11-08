//! CDC IIS vaccine code sets: CVX (vaccine administered), MVX (manufacturer),
//! product trade names, and vaccine NDCs.
//!
//! The bundled snapshot (`cdc-iis-vaccine-codes`) is converted from CDC's
//! pipe-delimited flat files to CSV. [`parse_cvx_flat`] also reads the raw
//! flat file (`cvx.txt`) so callers can refresh the CVX table live through
//! [`crate::fetch::fetch_with_fallback`]. Phase 6 (CDSi) can use
//! [`VaccineCodes::cvx`] to map CDSi antigen series to CVX codes.

use phn_core::{PhnError, Result, Source, SourceMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::snapshots::{SnapshotOrigin, SnapshotStore};

/// Snapshot id in `snapshots/manifest.json`.
pub const SNAPSHOT_ID: &str = "cdc-iis-vaccine-codes";

/// CDC's live CVX flat file.
pub const CVX_LIVE_URL: &str = "https://www2a.cdc.gov/vaccines/iis/iisstandards/downloads/cvx.txt";

/// CDC's IIS code set landing page (for citations).
pub const CODE_SETS_PAGE: &str =
    "https://www2a.cdc.gov/vaccines/iis/iisstandards/vaccines.asp?rpt=cvx";

/// One CVX code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Cvx {
    /// CVX code, zero-padded to at least two digits (e.g. "03", "208").
    pub cvx: String,
    pub short_description: String,
    pub full_name: String,
    #[serde(default)]
    pub notes: String,
    /// "Active", "Inactive", "Non-US", "Never Active", or "Pending".
    pub status: String,
    /// "True" when the code is not a vaccine (for example immune globulin).
    #[serde(default)]
    pub non_vaccine: String,
    pub last_updated: String,
}

/// One MVX manufacturer code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Mvx {
    pub mvx: String,
    pub manufacturer: String,
    #[serde(default)]
    pub notes: String,
    pub status: String,
    pub last_updated: String,
}

/// A product trade name mapped to CVX and MVX.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TradeName {
    pub trade_name: String,
    pub cvx_description: String,
    pub cvx: String,
    pub manufacturer: String,
    pub mvx: String,
    pub mvx_status: String,
    pub product_status: String,
    pub last_updated: String,
}

/// A vaccine National Drug Code (unit of sale or unit of use).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VaccineNdc {
    /// NDC11 with dashes (5-4-2).
    pub ndc: String,
    /// "sale" (the package ordered) or "use" (the unit administered).
    pub level: String,
    pub proprietary_name: String,
    pub labeler: String,
    pub start_date: String,
    /// Blank while the NDC is still marketed.
    #[serde(default)]
    pub end_date: String,
    pub packaging: String,
    #[serde(default)]
    pub route: String,
    pub cvx: String,
}

impl VaccineNdc {
    /// NDC with only digits (11 digits).
    pub fn digits(&self) -> String {
        self.ndc.chars().filter(char::is_ascii_digit).collect()
    }
}

/// The loaded code sets and where they came from.
#[derive(Debug, Clone)]
pub struct VaccineCodes {
    pub cvx: Vec<Cvx>,
    pub mvx: Vec<Mvx>,
    pub trade_names: Vec<TradeName>,
    pub ndc: Vec<VaccineNdc>,
    /// Citation for the bundled snapshot.
    pub source: Source,
    pub origin: SnapshotOrigin,
}

fn read_csv<T: for<'de> Deserialize<'de>>(text: &str, what: &str) -> Result<Vec<T>> {
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(text.as_bytes());
    rdr.deserialize()
        .collect::<std::result::Result<Vec<T>, _>>()
        .map_err(|e| PhnError::Internal(format!("bundled {what} snapshot is invalid: {e}")))
}

impl VaccineCodes {
    /// Load every code set from the snapshot store.
    pub fn load(store: &SnapshotStore) -> Result<Self> {
        let (cvx, entry, origin) = store.file_text(SNAPSHOT_ID, "cdc_cvx.csv")?;
        let (mvx, _, _) = store.file_text(SNAPSHOT_ID, "cdc_mvx.csv")?;
        let (tn, _, _) = store.file_text(SNAPSHOT_ID, "cdc_vaccine_tradenames.csv")?;
        let (ndc, _, _) = store.file_text(SNAPSHOT_ID, "cdc_vaccine_ndc.csv")?;
        let mut source = entry.to_source();
        source.url = Some(CODE_SETS_PAGE.to_string());
        Ok(Self {
            cvx: read_csv(&cvx, "CVX")?,
            mvx: read_csv(&mvx, "MVX")?,
            trade_names: read_csv(&tn, "trade name")?,
            ndc: read_csv(&ndc, "NDC")?,
            source,
            origin,
        })
    }

    /// Replace the CVX table with a fresher copy (for example from the live
    /// flat file) and record where it came from.
    pub fn with_cvx(mut self, cvx: Vec<Cvx>) -> Self {
        if !cvx.is_empty() {
            self.cvx = cvx;
        }
        self
    }

    /// Look up a CVX code ("3", "03", or "003" all match "03").
    pub fn cvx_code(&self, code: &str) -> Option<&Cvx> {
        let want = normalize_cvx(code)?;
        self.cvx.iter().find(|c| c.cvx == want)
    }

    /// Look up an MVX code (case-insensitive).
    pub fn mvx_code(&self, code: &str) -> Option<&Mvx> {
        let want = code.trim().to_ascii_uppercase();
        self.mvx.iter().find(|m| m.mvx.eq_ignore_ascii_case(&want))
    }

    /// NDC entries matching a 10- or 11-digit NDC, with or without dashes.
    ///
    /// A 10-digit NDC (4-4-2, 5-3-2, or 5-4-1) is matched against every
    /// 11-digit conversion, since the segment layout cannot be known without
    /// dashes.
    pub fn ndc_lookup(&self, code: &str) -> Vec<&VaccineNdc> {
        let candidates = ndc11_candidates(code);
        self.ndc
            .iter()
            .filter(|n| candidates.contains(&n.digits()))
            .collect()
    }

    /// Trade names that map to a CVX code.
    pub fn trade_names_for_cvx(&self, cvx: &str) -> Vec<&TradeName> {
        let Some(want) = normalize_cvx(cvx) else {
            return Vec::new();
        };
        self.trade_names.iter().filter(|t| t.cvx == want).collect()
    }

    /// NDCs that map to a CVX code.
    pub fn ndcs_for_cvx(&self, cvx: &str) -> Vec<&VaccineNdc> {
        let Some(want) = normalize_cvx(cvx) else {
            return Vec::new();
        };
        self.ndc.iter().filter(|n| n.cvx == want).collect()
    }
}

/// Normalize a CVX code to CDC's zero-padded form; `None` when it is not numeric.
pub fn normalize_cvx(code: &str) -> Option<String> {
    let t = code.trim().trim_start_matches(['C', 'c']).trim_start_matches(['V', 'v']);
    let t = t.trim_start_matches(['X', 'x']).trim();
    if t.is_empty() || !t.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let n: u32 = t.parse().ok()?;
    Some(format!("{n:02}"))
}

/// Possible 11-digit forms of an NDC.
pub fn ndc11_candidates(code: &str) -> Vec<String> {
    let trimmed = code.trim();
    let parts: Vec<&str> = trimmed.split('-').collect();
    if parts.len() == 3 && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())) {
        let (a, b, c) = (parts[0], parts[1], parts[2]);
        return vec![format!("{a:0>5}{b:0>4}{c:0>2}")];
    }
    let digits: String = trimmed.chars().filter(char::is_ascii_digit).collect();
    match digits.len() {
        11 => vec![digits],
        10 => vec![
            format!("0{digits}"),                                  // 4-4-2
            format!("{}0{}", &digits[..5], &digits[5..]),          // 5-3-2
            format!("{}0{}", &digits[..9], &digits[9..]),          // 5-4-1
        ],
        _ => Vec::new(),
    }
}

/// Parse CDC's raw pipe-delimited `cvx.txt`
/// (`code|short|full|notes|status|nonvaccine|last updated`).
pub fn parse_cvx_flat(text: &str) -> Vec<Cvx> {
    text.trim_start_matches('\u{feff}')
        .lines()
        .filter_map(|line| {
            let p: Vec<&str> = line.split('|').map(str::trim).collect();
            if p.len() < 7 {
                return None;
            }
            let cvx = normalize_cvx(p[0])?;
            Some(Cvx {
                cvx,
                short_description: p[1].to_string(),
                full_name: p[2].to_string(),
                notes: p[3].to_string(),
                status: p[4].to_string(),
                non_vaccine: p[5].to_string(),
                last_updated: p[6].replace('/', "-").chars().take(10).collect(),
            })
        })
        .collect()
}

/// A citation for the live CVX file.
pub fn live_cvx_source(mode: SourceMode, as_of: &str) -> Source {
    Source {
        title: "CVX - Vaccines Administered (live code set)".into(),
        publisher: "Centers for Disease Control and Prevention".into(),
        url: Some(CODE_SETS_PAGE.into()),
        effective_date: None,
        retrieved: Some(as_of.chars().take(10).collect()),
        mode,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_code_sets_load_and_cross_reference() {
        let codes = VaccineCodes::load(&SnapshotStore::bundled()).unwrap();
        assert!(codes.cvx.len() > 200);
        let mmr = codes.cvx_code("3").unwrap();
        assert_eq!(mmr.cvx, "03");
        assert!(mmr.short_description.contains("MMR"));
        assert!(codes.mvx_code("pmc").is_some());
        assert!(!codes.trade_names_for_cvx("03").is_empty());
        // Every NDC maps to a known CVX code.
        for n in &codes.ndc {
            assert!(
                codes.cvx_code(&n.cvx).is_some(),
                "NDC {} has unknown CVX {}",
                n.ndc,
                n.cvx
            );
        }
    }

    #[test]
    fn ndc_forms_are_normalized() {
        assert_eq!(ndc11_candidates("00006-4681-00"), vec!["00006468100"]);
        assert_eq!(ndc11_candidates("6-4681-0"), vec!["00006468100"]);
        assert_eq!(ndc11_candidates("0006-4681-00").len(), 1);
        assert_eq!(ndc11_candidates("0006468100").len(), 3);
        assert!(ndc11_candidates("12").is_empty());
        assert_eq!(normalize_cvx("CVX 3"), Some("03".into()));
        assert_eq!(normalize_cvx("208"), Some("208".into()));
        assert_eq!(normalize_cvx("MMR"), None);
    }

    #[test]
    fn parses_the_raw_flat_file() {
        let raw = "\u{feff}03        |MMR|measles, mumps and rubella virus vaccine||Active|False|2010/05/28\r\nbad line\r\n";
        let c = parse_cvx_flat(raw);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].cvx, "03");
        assert_eq!(c[0].last_updated, "2010-05-28");
    }
}
