//! Resolve what a nurse types ("Pediarix", "DTaP", "CVX 110", "110") to a
//! CVX code, using only CDC sources: the supporting data's CVX short
//! descriptions, vaccine types and trade names, and the vaccine names CDC's
//! test cases pair with a CVX code.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use crate::data::{SupportingData, normalize_cvx};

const TEST_CASE_NAMES: &str = include_str!("../data/vaccine-names-from-test-cases.json");

/// Result of resolving a vaccine name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// One CVX code.
    Cvx { cvx: String, label: String },
    /// Several CVX codes match; the nurse must choose.
    Ambiguous(Vec<(String, String)>),
    /// Nothing matched; close names are suggested.
    Unknown { suggestions: Vec<String> },
}

fn key(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

struct Index {
    /// normalized name -> CVX codes
    by_name: BTreeMap<String, BTreeSet<String>>,
    /// display name for each normalized name
    display: BTreeMap<String, String>,
}

fn index() -> &'static Index {
    static IDX: OnceLock<Index> = OnceLock::new();
    IDX.get_or_init(|| {
        let sd = SupportingData::get();
        let mut by_name: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut display: BTreeMap<String, String> = BTreeMap::new();
        let mut add = |name: &str, cvx: &str| {
            let k = key(name);
            if k.is_empty() || cvx.is_empty() {
                return;
            }
            by_name.entry(k.clone()).or_default().insert(normalize_cvx(cvx));
            display.entry(k).or_insert_with(|| name.trim().to_string());
        };
        for (cvx, e) in &sd.schedule.cvx_map {
            add(&e.short_description, cvx);
        }
        for a in sd.antigens.values() {
            for s in &a.series {
                for d in &s.doses {
                    for p in &d.preferable_vaccines {
                        add(&p.vaccine_type, &p.cvx);
                        if let Some(t) = &p.trade_name {
                            add(t, &p.cvx);
                        }
                    }
                    for v in &d.allowable_vaccines {
                        add(&v.vaccine_type, &v.cvx);
                    }
                }
            }
        }
        for (name, cvx) in test_case_names() {
            add(&name, &cvx);
        }
        Index { by_name, display }
    })
}

/// Short description of a CVX code from the supporting data.
pub fn cvx_label(cvx: &str) -> Option<String> {
    SupportingData::get()
        .schedule
        .cvx_map
        .get(&normalize_cvx(cvx))
        .map(|e| e.short_description.clone())
}

/// Resolve a CVX code, vaccine type, or trade name.
pub fn resolve(text: &str) -> Resolved {
    let t = text.trim();
    let sd = SupportingData::get();
    // "CVX 110", "cvx:110", "110"
    let digits = t
        .trim_start_matches(|c: char| c.is_ascii_alphabetic() && "cvxCVX".contains(c))
        .trim_start_matches([':', ' ', '#', '-'])
        .trim();
    if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
        let cvx = normalize_cvx(digits);
        return match sd.schedule.cvx_map.get(&cvx) {
            Some(e) => Resolved::Cvx {
                label: e.short_description.clone(),
                cvx,
            },
            None => Resolved::Unknown {
                suggestions: Vec::new(),
            },
        };
    }
    let idx = index();
    let k = key(t);
    let label_of = |cvx: &String| cvx_label(cvx).unwrap_or_else(|| format!("CVX {cvx}"));
    if let Some(cvxs) = idx.by_name.get(&k) {
        return if cvxs.len() == 1 {
            let cvx = cvxs.iter().next().cloned().unwrap_or_default();
            Resolved::Cvx {
                label: label_of(&cvx),
                cvx,
            }
        } else {
            Resolved::Ambiguous(cvxs.iter().map(|c| (c.clone(), label_of(c))).collect())
        };
    }
    // Unique prefix match on the normalized name ("gardasil9" -> "gardasil 9").
    if k.len() >= 4 {
        let hits: BTreeSet<String> = idx
            .by_name
            .iter()
            .filter(|(n, _)| n.starts_with(&k))
            .flat_map(|(_, c)| c.iter().cloned())
            .collect();
        if hits.len() == 1 {
            let cvx = hits.into_iter().next().unwrap_or_default();
            return Resolved::Cvx {
                label: label_of(&cvx),
                cvx,
            };
        }
    }
    let mut scored: Vec<(f64, String)> = idx
        .display
        .iter()
        .map(|(k2, d)| (strsim_ratio(&k, k2), d.clone()))
        .filter(|(s, _)| *s >= 0.6)
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    Resolved::Unknown {
        suggestions: scored.into_iter().take(5).map(|(_, d)| d).collect(),
    }
}

/// Normalized similarity (1 − Levenshtein distance / longer length).
fn strsim_ratio(a: &str, b: &str) -> f64 {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let n = a.len().max(b.len());
    if n == 0 {
        return 1.0;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j] + usize::from(ca != cb))
                .min(prev[j + 1] + 1)
                .min(cur[j] + 1);
        }
        prev = cur;
    }
    1.0 - prev[b.len()] as f64 / n as f64
}

fn test_case_names() -> Vec<(String, String)> {
    #[derive(serde::Deserialize)]
    struct NamesFile {
        names: BTreeMap<String, String>,
    }
    serde_json::from_str::<NamesFile>(TEST_CASE_NAMES)
        .map(|f| f.names.into_iter().collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cvx(text: &str) -> String {
        match resolve(text) {
            Resolved::Cvx { cvx, .. } => cvx,
            other => panic!("{text}: {other:?}"),
        }
    }

    #[test]
    fn resolves_codes_types_and_trade_names() {
        assert_eq!(cvx("110"), "110");
        assert_eq!(cvx("CVX 08"), "8");
        assert_eq!(cvx("Pediarix"), "110");
        assert_eq!(cvx("M-M-R II"), "3");
        assert_eq!(cvx("mmr"), "3");
        assert_eq!(cvx("PRP-T"), "48");
        assert_eq!(cvx("Varivax"), "21");
        assert_eq!(cvx("Gardasil 9"), "165");
        assert!(matches!(resolve("9999"), Resolved::Unknown { .. }));
        match resolve("Pedarix") {
            Resolved::Unknown { suggestions } => {
                assert!(suggestions.iter().any(|s| s.eq_ignore_ascii_case("PEDIARIX")))
            }
            other => panic!("{other:?}"),
        }
        assert!(test_case_names().len() > 100);
    }
}
