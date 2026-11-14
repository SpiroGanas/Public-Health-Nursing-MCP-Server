//! Typed access to `content/leadership/department_finance.yaml` and
//! `content/leadership/staffing_model.yaml` (both pending review).

use std::collections::BTreeMap;

use phn_content::ContentMeta;
use phn_core::{PhnError, Result};
use serde::Deserialize;

pub use crate::epr::content::{Assumption, Cited, ColumnAlias, Confidence, DocSource};
use crate::epr::content::{load, review_caveats};

/// Content id of `content/leadership/department_finance.yaml`.
pub const FINANCE: &str = "leadership-department-finance";
/// Content id of `content/leadership/staffing_model.yaml`.
pub const STAFFING: &str = "leadership-staffing-model";

#[derive(Debug, Clone, Deserialize)]
pub struct FiscalYear {
    pub default_start_month: u32,
    pub default_start_day: u32,
    pub statements: Vec<Cited>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PheProgram {
    pub statements: Vec<Cited>,
    pub label_words: Vec<String>,
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RevenueGroup {
    pub id: String,
    pub label: String,
    pub words: Vec<String>,
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Benchmark {
    pub id: String,
    pub label: String,
    pub value: f64,
    pub unit: String,
    pub source: String,
    pub section: String,
    pub confidence: Confidence,
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ItemDetail {
    pub item: String,
    pub detail: String,
}

/// A statement that may have no published source (this project's method).
#[derive(Debug, Clone, Deserialize)]
pub struct MethodNote {
    pub text: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FinanceContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub fiscal_year: FiscalYear,
    pub definitions: Vec<Cited>,
    pub phe_program: PheProgram,
    pub revenue_groups: Vec<RevenueGroup>,
    pub personnel_words: Vec<String>,
    pub revenue_type_words: Vec<String>,
    pub expense_type_words: Vec<String>,
    pub blocked_header_words: Vec<String>,
    pub column_aliases: Vec<ColumnAlias>,
    pub defaults: Vec<Assumption>,
    pub per_capita_benchmarks: Vec<Benchmark>,
    pub not_verified: Vec<ItemDetail>,
}

impl FinanceContent {
    pub fn load() -> Result<Self> {
        load(FINANCE)
    }

    /// A default value by id.
    pub fn default_value(&self, id: &str) -> Result<f64> {
        self.defaults
            .iter()
            .find(|a| a.id == id)
            .map(|a| a.value)
            .ok_or_else(|| PhnError::Internal(format!("finance default {id} missing")))
    }

    /// Column names a field may have.
    pub fn aliases(&self, field: &str) -> Vec<String> {
        self.column_aliases
            .iter()
            .find(|c| c.field == field)
            .map(|c| c.aliases.clone())
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StaffingContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub method: Vec<MethodNote>,
    pub productive_hours_defaults: Vec<Assumption>,
    pub benchmarks: Vec<Benchmark>,
    pub not_bundled: Vec<ItemDetail>,
}

impl StaffingContent {
    pub fn load() -> Result<Self> {
        load(STAFFING)
    }

    /// A productive-hours default by id.
    pub fn default_value(&self, id: &str) -> Result<f64> {
        self.productive_hours_defaults
            .iter()
            .find(|a| a.id == id)
            .map(|a| a.value)
            .ok_or_else(|| PhnError::Internal(format!("staffing default {id} missing")))
    }
}

/// Review caveats for the given content ids (empty once reviewed).
pub fn caveats(ids: &[&str]) -> Vec<String> {
    review_caveats(ids)
}

/// Lower-case text with every run of non-alphanumeric characters turned into one space, padded
/// with a space at both ends, so a word or phrase can be tested with `contains(" word ")`.
pub fn padded(s: &str) -> String {
    let mut out = String::from(" ");
    let mut last_space = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            for l in c.to_lowercase() {
                out.push(l);
            }
            last_space = false;
        } else if !last_space {
            out.push(' ');
            last_space = true;
        }
    }
    if !last_space {
        out.push(' ');
    }
    out
}

/// True when the label contains the word or phrase as whole words.
pub fn has_word(label: &str, word: &str) -> bool {
    let w = padded(word);
    let w = w.trim();
    !w.is_empty() && padded(label).contains(&format!(" {w} "))
}

/// True when the label contains any of the words.
pub fn has_any(label: &str, words: &[String]) -> bool {
    words.iter().any(|w| has_word(label, w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finance_content_loads_and_every_citation_resolves() {
        let c = FinanceContent::load().unwrap();
        for s in c
            .fiscal_year
            .statements
            .iter()
            .chain(&c.definitions)
            .chain(&c.phe_program.statements)
        {
            assert!(c.sources.contains_key(&s.source), "{}", s.source);
        }
        for b in &c.per_capita_benchmarks {
            assert!(c.sources.contains_key(&b.source));
        }
        assert_eq!(c.fiscal_year.default_start_month, 7);
        assert_eq!(c.default_value("min_personnel_group").unwrap(), 3.0);
        assert!(c.revenue_groups.iter().any(|g| g.id == "other"));
        for f in [
            "category",
            "type",
            "source",
            "budget",
            "actual",
            "encumbrance",
            "account",
            "period",
            "positions",
        ] {
            assert!(!c.aliases(f).is_empty(), "{f}");
        }
    }

    #[test]
    fn staffing_content_loads_and_bundles_no_benchmark() {
        let c = StaffingContent::load().unwrap();
        assert!(c.benchmarks.is_empty());
        assert!(!c.not_bundled.is_empty());
        assert_eq!(c.default_value("paid_hours_per_year").unwrap(), 2080.0);
        assert!(
            c.productive_hours_defaults.iter().all(|a| {
                a.basis == "tool_default" || a.basis == "computed" || a.source.is_some()
            })
        );
    }

    #[test]
    fn word_matching_is_by_whole_words() {
        assert!(has_word("State PHE grant (FY27)", "phe"));
        assert!(!has_word("Telephone", "phe"));
        assert!(has_word("Salaries & Wages", "salaries"));
        assert!(has_word("Per-Diem staff", "per diem"));
        assert!(!has_word("Inspections", "inspection"));
    }
}
