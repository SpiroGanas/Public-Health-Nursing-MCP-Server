//! DPH childhood lead screening data by community (snapshot `ma-lead-screening`), shared by
//! `childhood_lead_screening_guide` (MCH-04) and `map_lead_risk` (MCH-05).

use phn_core::{PhnError, Result, Source};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;

/// One community's row as published by DPH (values kept as printed: "NS", ">99", "NA").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LeadTownRow {
    pub town: String,
    /// Calendar year of the screening figures (2024, or 2023 where DPH's 2024 report omitted
    /// the community).
    pub data_year: String,
    pub population_9_47mo: String,
    pub screened: String,
    pub pct_screened: String,
    pub bll_0_4_n: String,
    pub bll_0_4_pct: String,
    pub bll_5_9_n: String,
    pub bll_5_9_pct: String,
    pub bll_10_24_n: String,
    pub bll_10_24_pct: String,
    pub bll_25plus_n: String,
    pub bll_25plus_pct: String,
    pub est_confirmed_5plus_n: String,
    pub est_confirmed_5plus_pct: String,
    pub confirmed_10plus_n: String,
    pub confirmed_10plus_pct: String,
    pub pct_pre1978_housing: String,
    /// "yes" when on DPH's 2024 high-risk community list.
    pub high_risk_2024: String,
    pub hr_rank: String,
    pub hr_pct_screened_5yr: String,
    pub hr_cases_5yr: String,
    pub hr_incidence_per_1000: String,
    pub hr_pct_pir_below_2: String,
    pub hr_pct_pre1978: String,
    pub hr_score: String,
}

impl LeadTownRow {
    /// True when on the 2024 high-risk list.
    pub fn high_risk(&self) -> bool {
        self.high_risk_2024 == "yes"
    }

    /// A published value as a number (`None` for NS, NA, or blank; ">99" reads as 100).
    pub fn num(v: &str) -> Option<f64> {
        let t = v.trim();
        if let Some(r) = t.strip_prefix('>') {
            // ">99": more children screened than the population estimate.
            return r.parse::<f64>().ok().map(|_| 100.0);
        }
        t.parse::<f64>().ok()
    }
}

/// All rows (351 towns plus "Total for MA") and the snapshot citation.
pub fn load(ctx: &Context) -> Result<(Vec<LeadTownRow>, Source)> {
    let (text, entry, _) = ctx
        .snapshots
        .file_text("ma-lead-screening", "ma_lead_screening_2024.csv")?;
    let mut rows = Vec::new();
    for r in csv::Reader::from_reader(text.as_bytes()).deserialize::<LeadTownRow>() {
        rows.push(r.map_err(|e| PhnError::Internal(format!("lead screening snapshot: {e}")))?);
    }
    Ok((rows, entry.to_source()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_has_every_town_and_the_high_risk_list() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        let (rows, src) = load(&ctx).unwrap();
        assert_eq!(rows.len(), 352);
        assert!(src.title.contains("lead"));
        for m in &ctx.ma.municipalities {
            assert!(rows.iter().any(|r| r.town == m.name), "{}", m.name);
        }
        let hr: Vec<&str> = rows
            .iter()
            .filter(|r| r.high_risk())
            .map(|r| r.town.as_str())
            .collect();
        assert_eq!(hr.len(), 17);
        assert!(hr.contains(&"New Bedford") && hr.contains(&"Boston"));
        assert_eq!(LeadTownRow::num(">99"), Some(100.0));
        assert_eq!(LeadTownRow::num("NS"), None);
    }
}
