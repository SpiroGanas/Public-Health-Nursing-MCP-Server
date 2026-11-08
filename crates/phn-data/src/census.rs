//! U.S. Census Bureau American Community Survey (ACS) 5-year estimates.
//!
//! * [`INDICATORS`] defines every community-profile indicator as ACS detailed
//!   table cells (numerator and denominator lines), so the figure can be cited
//!   by table ID and recomputed from the live API or from the bundled snapshot
//!   with the same code.
//! * Margins of error (90% confidence) for sums, proportions, and percentages
//!   use the approximation formulas in the Census Bureau's handbook
//!   *Understanding and Using American Community Survey Data: What All Data
//!   Users Need to Know* (2020), chapter 8. The coefficient of variation is
//!   `CV = (MOE / 1.645) / estimate`; estimates with CV above 30% are flagged
//!   as unreliable.
//! * [`AcsApi`] calls `api.census.gov`. The optional key comes from
//!   `PHN_CENSUS_API_KEY`; it is never cached or cited. From some networks the
//!   API answers keyless requests with a redirect to `missing_key.html`; that
//!   is treated as a failure and the bundled snapshot is used.
//! * [`AcsSnapshot`] reads the bundled 2020–2024 snapshot for the nation,
//!   Massachusetts, its 14 counties, 351 cities and towns, and census tracts.

use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

use phn_core::{PhnError, Result, Source, SourceMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::apifetch::{self, expect_json};
use crate::cache::Cache;
use crate::fetch::SnapshotFallback;
use crate::http::HttpClient;
use crate::snapshots::{SnapshotEntry, SnapshotOrigin, SnapshotStore};

/// ACS 5-year vintage (last year of the period) of the bundled snapshot.
pub const SNAPSHOT_VINTAGE: u32 = 2024;
/// Snapshot id in `snapshots/manifest.json`.
pub const SNAPSHOT_ID: &str = "census-acs5-ma";
/// Wide indicator file.
pub const SNAPSHOT_FILE: &str = "ma_acs5_indicators.csv";
/// B16001 detailed languages (Massachusetts and its PUMAs).
pub const LANGUAGE_DETAIL_FILE: &str = "ma_acs5_languages_detailed.csv";
/// API root.
pub const API_BASE: &str = "https://api.census.gov/data";
/// Environment variable holding the optional API key.
pub const KEY_ENV: &str = "PHN_CENSUS_API_KEY";
/// Where to request a free key.
pub const KEY_SIGNUP: &str = "https://api.census.gov/data/key_signup.html";
/// Z value for the ACS 90% margin of error.
pub const Z90: f64 = 1.645;
/// CV above which an estimate is flagged unreliable (percent).
pub const CV_UNRELIABLE: f64 = 30.0;

/// Geographic level.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum GeoLevel {
    Nation,
    State,
    County,
    /// City or town (Census county subdivision).
    Town,
    Tract,
    /// ZIP Code Tabulation Area.
    Zcta,
    /// Public Use Microdata Area (used only for detailed languages).
    Puma,
}

impl GeoLevel {
    /// Lower-case label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Nation => "nation",
            Self::State => "state",
            Self::County => "county",
            Self::Town => "town",
            Self::Tract => "tract",
            Self::Zcta => "zcta",
            Self::Puma => "puma",
        }
    }

    /// Parse the label used in snapshot files.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "nation" | "us" => Self::Nation,
            "state" => Self::State,
            "county" => Self::County,
            "town" | "city" | "cousub" => Self::Town,
            "tract" => Self::Tract,
            "zcta" | "zip" => Self::Zcta,
            "puma" => Self::Puma,
            _ => return None,
        })
    }
}

/// How an indicator is computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IndicatorKind {
    /// Sum of the numerator cells.
    Count,
    /// 100 × sum(numerator) / sum(denominator).
    Percent,
    /// A published median (one cell).
    Median,
    /// A published dollar amount (one cell).
    Dollars,
}

/// One profile indicator defined from ACS detailed-table cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndicatorDef {
    /// Stable id (snapshot column name).
    pub id: &'static str,
    /// Profile section.
    pub section: &'static str,
    /// Plain-language label.
    pub label: &'static str,
    /// ACS table ID, e.g. `B01001`.
    pub table: &'static str,
    pub kind: IndicatorKind,
    /// Cells summed for the numerator (or the value), e.g. `B01001_003`.
    pub num: &'static [&'static str],
    /// Cells summed for the denominator (percent indicators only).
    pub den: &'static [&'static str],
    /// The universe (denominator population).
    pub universe: &'static str,
}

macro_rules! ind {
    ($id:literal, $section:literal, $label:literal, $table:literal, $kind:ident, [$($n:literal),* $(,)?], [$($d:literal),* $(,)?], $universe:literal) => {
        IndicatorDef {
            id: $id,
            section: $section,
            label: $label,
            table: $table,
            kind: IndicatorKind::$kind,
            num: &[$($n),*],
            den: &[$($d),*],
            universe: $universe,
        }
    };
}

/// C16001 language groups: (id, label, total line, "less than very well" line).
pub const LANGUAGE_GROUPS: [(&str, &str, &str, &str); 11] = [
    ("spanish", "Spanish", "C16001_003", "C16001_005"),
    ("french_haitian_cajun", "French, Haitian, or Cajun", "C16001_006", "C16001_008"),
    ("german_west_germanic", "German or other West Germanic languages", "C16001_009", "C16001_011"),
    ("russian_polish_slavic", "Russian, Polish, or other Slavic languages", "C16001_012", "C16001_014"),
    ("other_indo_european", "Other Indo-European languages (includes Portuguese, Cape Verdean Creole, Italian, Greek, Hindi, and others)", "C16001_015", "C16001_017"),
    ("korean", "Korean", "C16001_018", "C16001_020"),
    ("chinese", "Chinese (incl. Mandarin, Cantonese)", "C16001_021", "C16001_023"),
    ("vietnamese", "Vietnamese", "C16001_024", "C16001_026"),
    ("tagalog", "Tagalog (incl. Filipino)", "C16001_027", "C16001_029"),
    ("other_asian_pacific", "Other Asian and Pacific Island languages (includes Khmer and others)", "C16001_030", "C16001_032"),
    ("arabic", "Arabic", "C16001_033", "C16001_035"),
];

/// The twelfth C16001 group, kept separate because it is a residual category.
pub const LANGUAGE_OTHER: (&str, &str, &str, &str) = (
    "other_unspecified",
    "Other and unspecified languages",
    "C16001_036",
    "C16001_038",
);

/// C16002 household language groups: (id, label, total line, limited-English line).
pub const HOUSEHOLD_LANGUAGE_GROUPS: [(&str, &str, &str, &str); 4] = [
    ("spanish", "Spanish", "C16002_003", "C16002_004"),
    ("other_indo_european", "Other Indo-European languages", "C16002_006", "C16002_007"),
    ("asian_pacific", "Asian and Pacific Island languages", "C16002_009", "C16002_010"),
    ("other", "Other languages", "C16002_012", "C16002_013"),
];

/// Every "speak English less than very well" line in C16001.
pub const LEP_LINES: [&str; 12] = [
    "C16001_005", "C16001_008", "C16001_011", "C16001_014", "C16001_017", "C16001_020",
    "C16001_023", "C16001_026", "C16001_029", "C16001_032", "C16001_035", "C16001_038",
];

/// Community-profile indicators.
pub const INDICATORS: &[IndicatorDef] = &[
    // Population and age
    ind!("pop_total", "Population and age", "Total population", "B01003", Count, ["B01003_001"], [], "Total population"),
    ind!("median_age", "Population and age", "Median age (years)", "B01002", Median, ["B01002_001"], [], "Total population"),
    ind!("age_under5_pct", "Population and age", "Children under 5 (%)", "B01001", Percent, ["B01001_003", "B01001_027"], ["B01001_001"], "Total population"),
    ind!("age_under18_pct", "Population and age", "Children under 18 (%)", "B01001", Percent,
        ["B01001_003", "B01001_004", "B01001_005", "B01001_006", "B01001_027", "B01001_028", "B01001_029", "B01001_030"],
        ["B01001_001"], "Total population"),
    ind!("age_65plus_pct", "Population and age", "Adults 65 and older (%)", "B01001", Percent,
        ["B01001_020", "B01001_021", "B01001_022", "B01001_023", "B01001_024", "B01001_025",
         "B01001_044", "B01001_045", "B01001_046", "B01001_047", "B01001_048", "B01001_049"],
        ["B01001_001"], "Total population"),
    ind!("age_85plus_pct", "Population and age", "Adults 85 and older (%)", "B01001", Percent, ["B01001_025", "B01001_049"], ["B01001_001"], "Total population"),
    // Race and ethnicity
    ind!("re_hispanic_pct", "Race and ethnicity", "Hispanic or Latino, any race (%)", "B03002", Percent, ["B03002_012"], ["B03002_001"], "Total population"),
    ind!("re_nh_white_pct", "Race and ethnicity", "White alone, not Hispanic (%)", "B03002", Percent, ["B03002_003"], ["B03002_001"], "Total population"),
    ind!("re_nh_black_pct", "Race and ethnicity", "Black or African American alone, not Hispanic (%)", "B03002", Percent, ["B03002_004"], ["B03002_001"], "Total population"),
    ind!("re_nh_asian_pct", "Race and ethnicity", "Asian alone, not Hispanic (%)", "B03002", Percent, ["B03002_006"], ["B03002_001"], "Total population"),
    ind!("re_nh_aian_pct", "Race and ethnicity", "American Indian and Alaska Native alone, not Hispanic (%)", "B03002", Percent, ["B03002_005"], ["B03002_001"], "Total population"),
    ind!("re_nh_nhpi_pct", "Race and ethnicity", "Native Hawaiian and Other Pacific Islander alone, not Hispanic (%)", "B03002", Percent, ["B03002_007"], ["B03002_001"], "Total population"),
    ind!("re_nh_other_pct", "Race and ethnicity", "Some other race alone, not Hispanic (%)", "B03002", Percent, ["B03002_008"], ["B03002_001"], "Total population"),
    ind!("re_nh_multi_pct", "Race and ethnicity", "Two or more races, not Hispanic (%)", "B03002", Percent, ["B03002_009"], ["B03002_001"], "Total population"),
    // Nativity and language
    ind!("foreign_born_pct", "Nativity and language", "Born outside the United States (%)", "B05002", Percent, ["B05002_013"], ["B05002_001"], "Total population"),
    ind!("other_language_pct", "Nativity and language", "Speak a language other than English at home (%, age 5+)", "C16001", Percent,
        ["C16001_003", "C16001_006", "C16001_009", "C16001_012", "C16001_015", "C16001_018",
         "C16001_021", "C16001_024", "C16001_027", "C16001_030", "C16001_033", "C16001_036"],
        ["C16001_001"], "Population 5 years and over"),
    ind!("lep_pct", "Nativity and language", "Speak English less than \"very well\" (%, age 5+)", "C16001", Percent,
        ["C16001_005", "C16001_008", "C16001_011", "C16001_014", "C16001_017", "C16001_020",
         "C16001_023", "C16001_026", "C16001_029", "C16001_032", "C16001_035", "C16001_038"],
        ["C16001_001"], "Population 5 years and over"),
    ind!("lep_households_pct", "Nativity and language", "Limited English speaking households (%)", "C16002", Percent,
        ["C16002_004", "C16002_007", "C16002_010", "C16002_013"], ["C16002_001"], "Households"),
    // Income and poverty
    ind!("median_household_income", "Income and poverty", "Median household income (dollars, inflation-adjusted to the last survey year)", "B19013", Dollars, ["B19013_001"], [], "Households"),
    ind!("poverty_pct", "Income and poverty", "Income below the federal poverty level (%)", "C17002", Percent, ["C17002_002", "C17002_003"], ["C17002_001"], "Population for whom poverty status is determined"),
    ind!("poverty200_pct", "Income and poverty", "Income below 200% of the poverty level (%)", "C17002", Percent,
        ["C17002_002", "C17002_003", "C17002_004", "C17002_005", "C17002_006", "C17002_007"],
        ["C17002_001"], "Population for whom poverty status is determined"),
    ind!("unemployment_pct", "Income and poverty", "Unemployment rate (% of civilian labor force, age 16+)", "B23025", Percent, ["B23025_005"], ["B23025_003"], "Civilian labor force 16 years and over"),
    // Health insurance and disability
    ind!("uninsured_pct", "Health insurance and disability", "No health insurance (%)", "B27001", Percent,
        ["B27001_005", "B27001_008", "B27001_011", "B27001_014", "B27001_017", "B27001_020", "B27001_023", "B27001_026", "B27001_029",
         "B27001_033", "B27001_036", "B27001_039", "B27001_042", "B27001_045", "B27001_048", "B27001_051", "B27001_054", "B27001_057"],
        ["B27001_001"], "Civilian noninstitutionalized population"),
    ind!("uninsured_child_pct", "Health insurance and disability", "Children under 19 with no health insurance (%)", "B27001", Percent,
        ["B27001_005", "B27001_008", "B27001_033", "B27001_036"],
        ["B27001_003", "B27001_006", "B27001_031", "B27001_034"], "Civilian noninstitutionalized population under 19 years"),
    ind!("disability_pct", "Health insurance and disability", "With a disability (%)", "C18108", Percent,
        ["C18108_003", "C18108_004", "C18108_007", "C18108_008", "C18108_011", "C18108_012"],
        ["C18108_001"], "Civilian noninstitutionalized population"),
    ind!("disability_65plus_pct", "Health insurance and disability", "Adults 65+ with a disability (%)", "C18108", Percent,
        ["C18108_011", "C18108_012"], ["C18108_010"], "Civilian noninstitutionalized population 65 years and over"),
    // Housing and transportation
    ind!("renter_pct", "Housing and transportation", "Renter-occupied households (%)", "B25003", Percent, ["B25003_003"], ["B25003_001"], "Occupied housing units"),
    ind!("cost_burden_pct", "Housing and transportation", "Households spending 30% or more of income on housing (%)", "B25106", Percent,
        ["B25106_006", "B25106_010", "B25106_014", "B25106_018", "B25106_022",
         "B25106_028", "B25106_032", "B25106_036", "B25106_040", "B25106_044"],
        ["B25106_003", "B25106_007", "B25106_011", "B25106_015", "B25106_019",
         "B25106_025", "B25106_029", "B25106_033", "B25106_037", "B25106_041"],
        "Occupied housing units with a computed housing cost burden (excludes zero or negative income and no cash rent)"),
    ind!("renter_cost_burden_pct", "Housing and transportation", "Renter households spending 30% or more of income on rent (%)", "B25106", Percent,
        ["B25106_028", "B25106_032", "B25106_036", "B25106_040", "B25106_044"],
        ["B25106_025", "B25106_029", "B25106_033", "B25106_037", "B25106_041"],
        "Renter-occupied units with a computed cost burden"),
    ind!("crowding_pct", "Housing and transportation", "Crowded housing: more than 1 occupant per room (%)", "B25014", Percent,
        ["B25014_005", "B25014_006", "B25014_007", "B25014_011", "B25014_012", "B25014_013"],
        ["B25014_001"], "Occupied housing units"),
    ind!("no_vehicle_pct", "Housing and transportation", "Households with no vehicle (%)", "B25044", Percent, ["B25044_003", "B25044_010"], ["B25044_001"], "Occupied housing units"),
    ind!("no_internet_pct", "Housing and transportation", "Households with no internet access (%)", "B28002", Percent, ["B28002_013"], ["B28002_001"], "Households"),
    // Education
    ind!("less_than_hs_pct", "Education", "Adults 25+ without a high school diploma (%)", "B15003", Percent,
        ["B15003_002", "B15003_003", "B15003_004", "B15003_005", "B15003_006", "B15003_007", "B15003_008",
         "B15003_009", "B15003_010", "B15003_011", "B15003_012", "B15003_013", "B15003_014", "B15003_015", "B15003_016"],
        ["B15003_001"], "Population 25 years and over"),
    ind!("bachelors_plus_pct", "Education", "Adults 25+ with a bachelor's degree or higher (%)", "B15003", Percent,
        ["B15003_022", "B15003_023", "B15003_024", "B15003_025"], ["B15003_001"], "Population 25 years and over"),
    // Counts used by the language access profile
    ind!("pop5plus", "Language counts", "Population 5 years and over", "C16001", Count, ["C16001_001"], [], "Population 5 years and over"),
    ind!("lep_count", "Language counts", "Persons who speak English less than \"very well\" (age 5+)", "C16001", Count,
        ["C16001_005", "C16001_008", "C16001_011", "C16001_014", "C16001_017", "C16001_020",
         "C16001_023", "C16001_026", "C16001_029", "C16001_032", "C16001_035", "C16001_038"],
        [], "Population 5 years and over"),
    ind!("households", "Language counts", "Households", "C16002", Count, ["C16002_001"], [], "Households"),
    ind!("lep_households", "Language counts", "Limited English speaking households", "C16002", Count,
        ["C16002_004", "C16002_007", "C16002_010", "C16002_013"], [], "Households"),
];

/// Ids of the language-group count columns: `lang_<group>_total` and
/// `lang_<group>_lep` for every C16001 group, and `hh_<group>_total` /
/// `hh_<group>_lep` for every C16002 group.
pub fn language_count_columns() -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    for (id, _, total, lep) in LANGUAGE_GROUPS.iter().chain(std::iter::once(&LANGUAGE_OTHER)) {
        out.push((format!("lang_{id}_total"), *total));
        out.push((format!("lang_{id}_lep"), *lep));
    }
    for (id, _, total, lep) in HOUSEHOLD_LANGUAGE_GROUPS {
        out.push((format!("hh_{id}_total"), total));
        out.push((format!("hh_{id}_lep"), lep));
    }
    out
}

/// Look up an indicator definition.
pub fn indicator(id: &str) -> Option<&'static IndicatorDef> {
    INDICATORS.iter().find(|d| d.id == id)
}

/// Every detailed-table cell any indicator or language column needs, sorted.
pub fn required_cells() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = INDICATORS
        .iter()
        .flat_map(|d| d.num.iter().chain(d.den.iter()).copied())
        .chain(language_count_columns().into_iter().map(|(_, c)| c))
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

// ---------------------------------------------------------------------------
// Estimates and margins of error
// ---------------------------------------------------------------------------

/// An ACS estimate with its 90% margin of error.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Estimate {
    pub value: f64,
    /// 90% margin of error; `None` when the Census Bureau did not publish one.
    pub moe: Option<f64>,
}

impl Estimate {
    /// Coefficient of variation in percent, `(MOE / 1.645) / estimate × 100`.
    pub fn cv(&self) -> Option<f64> {
        let moe = self.moe?;
        if self.value == 0.0 {
            return None;
        }
        Some(moe / Z90 / self.value.abs() * 100.0)
    }

    /// True when the CV exceeds 30%, or when the estimate is zero with a
    /// non-zero margin of error.
    pub fn unreliable(&self) -> bool {
        match self.cv() {
            Some(cv) => cv > CV_UNRELIABLE,
            None => self.value == 0.0 && self.moe.is_some_and(|m| m > 0.0),
        }
    }

    /// 90% confidence interval (lower bound not below zero).
    pub fn interval(&self) -> Option<(f64, f64)> {
        self.moe
            .map(|m| ((self.value - m).max(0.0), self.value + m))
    }
}

/// Decode an ACS cell value. Special values (large negative codes) mean the
/// estimate was not computed.
pub fn decode_estimate(v: f64) -> Option<f64> {
    (v.is_finite() && v > -100_000_000.0).then_some(v)
}

/// Decode an ACS margin of error. `-555555555` means the estimate is
/// controlled (no sampling error, MOE 0); other negative codes mean no MOE.
pub fn decode_moe(v: f64) -> Option<f64> {
    if (v + 555_555_555.0).abs() < 0.5 {
        Some(0.0)
    } else if v.is_finite() && v >= 0.0 {
        Some(v)
    } else {
        None
    }
}

/// Sum of estimates. The MOE is `sqrt(Σ MOE²)`; following the Census
/// Bureau's guidance, when several components are zero only the largest MOE
/// among the zero estimates is included.
pub fn sum(parts: &[Estimate]) -> Estimate {
    let value = parts.iter().map(|p| p.value).sum();
    let mut sq = 0.0;
    let mut zero_max: f64 = 0.0;
    let mut missing = false;
    for p in parts {
        match p.moe {
            None => missing = true,
            Some(m) if p.value == 0.0 => zero_max = zero_max.max(m),
            Some(m) => sq += m * m,
        }
    }
    sq += zero_max * zero_max;
    Estimate {
        value,
        moe: (!missing).then(|| sq.sqrt()),
    }
}

/// Proportion `num / den` with the derived-proportion MOE
/// `sqrt(MOE_num² − p² MOE_den²) / den`; when the term under the root is
/// negative, the ratio formula `sqrt(MOE_num² + p² MOE_den²) / den` is used.
pub fn proportion(num: Estimate, den: Estimate) -> Option<Estimate> {
    if den.value <= 0.0 {
        return None;
    }
    let p = num.value / den.value;
    let moe = match (num.moe, den.moe) {
        (Some(mn), Some(md)) => {
            let under = mn * mn - p * p * md * md;
            let under = if under < 0.0 {
                mn * mn + p * p * md * md
            } else {
                under
            };
            Some(under.sqrt() / den.value)
        }
        _ => None,
    };
    Some(Estimate { value: p, moe })
}

/// Raw cell values: cell id (e.g. `B01001_003`) → estimate.
pub type Cells = HashMap<String, Estimate>;

/// Compute one indicator from raw cells. `None` when a cell is missing or
/// the denominator is zero.
pub fn compute(def: &IndicatorDef, cells: &Cells) -> Option<Estimate> {
    let get = |ids: &[&str]| -> Option<Vec<Estimate>> {
        ids.iter().map(|id| cells.get(*id).copied()).collect()
    };
    match def.kind {
        IndicatorKind::Count => Some(sum(&get(def.num)?)),
        IndicatorKind::Median | IndicatorKind::Dollars => cells.get(def.num[0]).copied(),
        IndicatorKind::Percent => {
            let num = sum(&get(def.num)?);
            let den = sum(&get(def.den)?);
            proportion(num, den).map(|p| Estimate {
                value: p.value * 100.0,
                moe: p.moe.map(|m| m * 100.0),
            })
        }
    }
}

/// Compute every indicator and language column from raw cells.
pub fn compute_all(cells: &Cells) -> BTreeMap<String, Estimate> {
    let mut out = BTreeMap::new();
    for def in INDICATORS {
        if let Some(e) = compute(def, cells) {
            out.insert(def.id.to_string(), e);
        }
    }
    for (col, cell) in language_count_columns() {
        if let Some(e) = cells.get(cell) {
            out.insert(col, *e);
        }
    }
    out
}

/// Round for storage: counts and dollars to whole numbers, other values to
/// one decimal.
pub fn round_for_storage(id: &str, v: f64) -> f64 {
    let whole = id.starts_with("lang_")
        || id.starts_with("hh_")
        || indicator(id).is_some_and(|d| matches!(d.kind, IndicatorKind::Count | IndicatorKind::Dollars));
    if whole {
        v.round()
    } else {
        (v * 10.0).round() / 10.0
    }
}

// ---------------------------------------------------------------------------
// Snapshot
// ---------------------------------------------------------------------------

/// One geography's indicators.
#[derive(Debug, Clone, PartialEq)]
pub struct AcsRow {
    pub geoid: String,
    pub level: GeoLevel,
    pub name: String,
    pub values: BTreeMap<String, Estimate>,
}

/// B16001 detailed language counts for Massachusetts or one PUMA.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DetailedLanguage {
    pub geoid: String,
    pub language: String,
    pub speakers: Estimate,
    /// Speak English less than "very well".
    pub lep: Estimate,
}

/// The bundled ACS snapshot.
#[derive(Debug, Clone)]
pub struct AcsSnapshot {
    pub vintage: u32,
    pub rows: HashMap<String, AcsRow>,
    pub detailed_languages: Vec<DetailedLanguage>,
    pub entry: SnapshotEntry,
    pub origin: SnapshotOrigin,
}

impl AcsSnapshot {
    /// Parse the snapshot files.
    pub fn load(store: &SnapshotStore) -> Result<Self> {
        let (text, entry, origin) = store.file_text(SNAPSHOT_ID, SNAPSHOT_FILE)?;
        let rows = parse_indicator_csv(&text)?;
        let (detail, _, _) = store.file_text(SNAPSHOT_ID, LANGUAGE_DETAIL_FILE)?;
        let detailed_languages = parse_language_detail(&detail)?;
        Ok(Self {
            vintage: SNAPSHOT_VINTAGE,
            rows,
            detailed_languages,
            entry,
            origin,
        })
    }

    /// The shared, lazily parsed snapshot.
    pub fn shared(store: &SnapshotStore) -> Result<&'static AcsSnapshot> {
        static CELL: OnceLock<std::result::Result<AcsSnapshot, String>> = OnceLock::new();
        CELL.get_or_init(|| Self::load(store).map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| PhnError::Internal(format!("ACS snapshot: {e}")))
    }

    /// A row by GEOID.
    pub fn row(&self, geoid: &str) -> Option<&AcsRow> {
        self.rows.get(geoid)
    }

    /// The nation row.
    pub fn nation(&self) -> Option<&AcsRow> {
        self.rows.get("US")
    }

    /// The Massachusetts row.
    pub fn state(&self) -> Option<&AcsRow> {
        self.rows.get("25")
    }

    /// Citation for figures read from the snapshot.
    pub fn source(&self) -> Source {
        let mut s = self.entry.to_source();
        s.title = acs_title(self.vintage);
        s
    }
}

/// Title used in citations, e.g. "American Community Survey 2020–2024 5-year estimates".
pub fn acs_title(vintage: u32) -> String {
    format!(
        "U.S. Census Bureau, American Community Survey {}–{vintage} 5-year estimates (detailed tables)",
        vintage - 4
    )
}

/// Parse the wide indicator CSV: `geoid,level,name,<id>,<id>_moe,...`.
pub fn parse_indicator_csv(text: &str) -> Result<HashMap<String, AcsRow>> {
    let mut rdr = csv::Reader::from_reader(text.as_bytes());
    let headers = rdr
        .headers()
        .map_err(|e| PhnError::Internal(format!("ACS snapshot header: {e}")))?
        .clone();
    let mut rows = HashMap::new();
    for rec in rdr.records() {
        let rec = rec.map_err(|e| PhnError::Internal(format!("ACS snapshot: {e}")))?;
        let geoid = rec.get(0).unwrap_or_default().to_string();
        let level = GeoLevel::parse(rec.get(1).unwrap_or_default())
            .ok_or_else(|| PhnError::Internal(format!("ACS snapshot: bad level for {geoid}")))?;
        let name = rec.get(2).unwrap_or_default().to_string();
        let mut values = BTreeMap::new();
        let mut i = 3;
        while i < headers.len() {
            let id = &headers[i];
            if id.ends_with("_moe") {
                i += 1;
                continue;
            }
            let value = rec.get(i).and_then(|v| v.parse::<f64>().ok());
            let moe = if headers.get(i + 1) == Some(&format!("{id}_moe")) {
                rec.get(i + 1).and_then(|v| v.parse::<f64>().ok())
            } else {
                None
            };
            if let Some(value) = value {
                values.insert(id.to_string(), Estimate { value, moe });
            }
            i += 1;
        }
        rows.insert(
            geoid.clone(),
            AcsRow {
                geoid,
                level,
                name,
                values,
            },
        );
    }
    Ok(rows)
}

/// Parse `geoid,language,speakers,speakers_moe,lep,lep_moe`.
pub fn parse_language_detail(text: &str) -> Result<Vec<DetailedLanguage>> {
    let mut out = Vec::new();
    let mut rdr = csv::Reader::from_reader(text.as_bytes());
    for rec in rdr.records() {
        let rec = rec.map_err(|e| PhnError::Internal(format!("language snapshot: {e}")))?;
        let f = |i: usize| rec.get(i).and_then(|v| v.parse::<f64>().ok());
        let (Some(s), Some(l)) = (f(2), f(4)) else {
            continue;
        };
        out.push(DetailedLanguage {
            geoid: rec.get(0).unwrap_or_default().to_string(),
            language: rec.get(1).unwrap_or_default().to_string(),
            speakers: Estimate { value: s, moe: f(3) },
            lep: Estimate { value: l, moe: f(5) },
        });
    }
    Ok(out)
}

/// The B16001 language labels (line of the speaker total → label), from the
/// 2024 table shell. The "less than very well" line is the total line + 2.
pub const DETAILED_LANGUAGES: [(&str, &str); 42] = [
    ("B16001_003", "Spanish"),
    ("B16001_006", "French (incl. Cajun)"),
    ("B16001_009", "Haitian"),
    ("B16001_012", "Italian"),
    ("B16001_015", "Portuguese"),
    ("B16001_018", "German"),
    ("B16001_021", "Yiddish, Pennsylvania Dutch or other West Germanic languages"),
    ("B16001_024", "Greek"),
    ("B16001_027", "Russian"),
    ("B16001_030", "Polish"),
    ("B16001_033", "Serbo-Croatian"),
    ("B16001_036", "Ukrainian or other Slavic languages"),
    ("B16001_039", "Armenian"),
    ("B16001_042", "Persian (incl. Farsi, Dari)"),
    ("B16001_045", "Gujarati"),
    ("B16001_048", "Hindi"),
    ("B16001_051", "Urdu"),
    ("B16001_054", "Punjabi"),
    ("B16001_057", "Bengali"),
    ("B16001_060", "Nepali, Marathi, or other Indic languages"),
    ("B16001_063", "Other Indo-European languages"),
    ("B16001_066", "Telugu"),
    ("B16001_069", "Tamil"),
    ("B16001_072", "Malayalam, Kannada, or other Dravidian languages"),
    ("B16001_075", "Chinese (incl. Mandarin, Cantonese)"),
    ("B16001_078", "Japanese"),
    ("B16001_081", "Korean"),
    ("B16001_084", "Hmong"),
    ("B16001_087", "Vietnamese"),
    ("B16001_090", "Khmer"),
    ("B16001_093", "Thai, Lao, or other Tai-Kadai languages"),
    ("B16001_096", "Other languages of Asia"),
    ("B16001_099", "Tagalog (incl. Filipino)"),
    ("B16001_102", "Ilocano, Samoan, Hawaiian, or other Austronesian languages"),
    ("B16001_105", "Arabic"),
    ("B16001_108", "Hebrew"),
    ("B16001_111", "Amharic, Somali, or other Afro-Asiatic languages"),
    ("B16001_114", "Yoruba, Twi, Igbo, or other languages of Western Africa"),
    ("B16001_117", "Swahili or other languages of Central, Eastern, and Southern Africa"),
    ("B16001_120", "Navajo"),
    ("B16001_123", "Other Native languages of North America"),
    ("B16001_126", "Other and unspecified languages"),
];

/// The "less than very well" line for a B16001 total line.
pub fn detailed_lep_line(total_line: &str) -> String {
    let (table, n) = total_line.split_once('_').unwrap_or((total_line, "0"));
    let n: u32 = n.parse().unwrap_or(0);
    format!("{table}_{:03}", n + 2)
}

// ---------------------------------------------------------------------------
// Live API
// ---------------------------------------------------------------------------

/// A geography the API understands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiGeo {
    Nation,
    State(String),
    County { state: String, county: String },
    CountySubdivision { state: String, county: String, cousub: String },
    Tract { state: String, county: String, tract: String },
}

impl ApiGeo {
    /// Build from a GEOID and level.
    pub fn from_geoid(level: GeoLevel, geoid: &str) -> Option<Self> {
        Some(match level {
            GeoLevel::Nation => Self::Nation,
            GeoLevel::State if geoid.len() == 2 => Self::State(geoid.into()),
            GeoLevel::County if geoid.len() == 5 => Self::County {
                state: geoid[..2].into(),
                county: geoid[2..].into(),
            },
            GeoLevel::Town if geoid.len() == 10 => Self::CountySubdivision {
                state: geoid[..2].into(),
                county: geoid[2..5].into(),
                cousub: geoid[5..].into(),
            },
            GeoLevel::Tract if geoid.len() == 11 => Self::Tract {
                state: geoid[..2].into(),
                county: geoid[2..5].into(),
                tract: geoid[5..].into(),
            },
            _ => return None,
        })
    }

    /// `for=` and `in=` query parameters.
    fn params(&self) -> (String, Option<String>) {
        match self {
            Self::Nation => ("us:1".into(), None),
            Self::State(s) => (format!("state:{s}"), None),
            Self::County { state, county } => (format!("county:{county}"), Some(format!("state:{state}"))),
            Self::CountySubdivision {
                state,
                county,
                cousub,
            } => (
                format!("county subdivision:{cousub}"),
                Some(format!("state:{state} county:{county}")),
            ),
            Self::Tract {
                state,
                county,
                tract,
            } => (
                format!("tract:{tract}"),
                Some(format!("state:{state} county:{county}")),
            ),
        }
    }
}

/// Values fetched from the API (or snapshot), with provenance.
#[derive(Debug, Clone, PartialEq)]
pub struct AcsFetch {
    pub vintage: u32,
    pub name: String,
    pub values: BTreeMap<String, Estimate>,
    pub mode: SourceMode,
    pub as_of: String,
    pub fallback_reason: Option<String>,
    /// Request URLs without the key (for citations).
    pub urls: Vec<String>,
}

impl AcsFetch {
    /// Citation for these figures.
    pub fn source(&self) -> Source {
        Source {
            title: acs_title(self.vintage),
            publisher: "U.S. Census Bureau".into(),
            url: Some(
                self.urls
                    .first()
                    .cloned()
                    .unwrap_or_else(|| format!("{API_BASE}/{}/acs/acs5", self.vintage)),
            ),
            effective_date: Some(format!("{}-12", self.vintage + 1)),
            retrieved: Some(self.as_of.chars().take(10).collect()),
            mode: self.mode,
        }
    }
}

/// Client for the ACS 5-year detailed tables API.
#[derive(Debug, Clone)]
pub struct AcsApi {
    pub vintage: u32,
    key: Option<String>,
}

/// Cache time-to-live for ACS responses (30 days; estimates are released yearly).
pub const ACS_TTL_SECS: u64 = 30 * 24 * 3600;

impl AcsApi {
    /// Client for `vintage`, with the key from `PHN_CENSUS_API_KEY` if set.
    pub fn new(vintage: u32) -> Self {
        Self {
            vintage,
            key: apifetch::api_key(KEY_ENV),
        }
    }

    /// True when a key is configured.
    pub fn has_key(&self) -> bool {
        self.key.is_some()
    }

    /// The request URLs (without key) for one geography: required cells in
    /// chunks of 24 (48 variables plus NAME, under the API's 50-variable limit).
    pub fn urls(&self, geo: &ApiGeo) -> Vec<String> {
        let (for_, in_) = geo.params();
        required_cells()
            .chunks(24)
            .map(|chunk| {
                let mut vars = vec!["NAME".to_string()];
                for c in chunk {
                    vars.push(format!("{c}E"));
                    vars.push(format!("{c}M"));
                }
                let mut url = format!(
                    "{API_BASE}/{}/acs/acs5?get={}&for={}",
                    self.vintage,
                    vars.join(","),
                    crate::socrata::encode(&for_)
                );
                if let Some(i) = &in_ {
                    url.push_str(&format!("&in={}", crate::socrata::encode(i)));
                }
                url
            })
            .collect()
    }

    /// Fetch every indicator for one geography (live → cache; no snapshot
    /// fallback here, callers use [`AcsSnapshot`] for that).
    pub async fn fetch(
        &self,
        http: &HttpClient,
        cache: Option<&Cache>,
        geo: &ApiGeo,
    ) -> Result<AcsFetch> {
        let urls = self.urls(geo);
        let mut cells = Cells::new();
        let mut name = String::new();
        let mut mode = SourceMode::Live;
        let mut as_of = String::new();
        let mut fallback_reason = None;
        for url in &urls {
            let key = self.key.as_deref().map(|k| ("key", k));
            let fetched = apifetch::fetch_validated(
                http,
                cache,
                url,
                key,
                ACS_TTL_SECS,
                &validate_census_body,
                None,
            )
            .await
            .map_err(|e| census_error(e, self.has_key()))?;
            let (n, c) = parse_api_response(&fetched.body)?;
            name = n;
            cells.extend(c);
            if fetched.mode != SourceMode::Live {
                mode = fetched.mode;
            }
            if as_of.is_empty() || fetched.as_of < as_of {
                as_of = fetched.as_of;
            }
            if fetched.fallback_reason.is_some() {
                fallback_reason = fetched.fallback_reason;
            }
        }
        Ok(AcsFetch {
            vintage: self.vintage,
            name,
            values: compute_all(&cells),
            mode,
            as_of,
            fallback_reason,
            urls,
        })
    }
}

fn census_error(e: PhnError, has_key: bool) -> PhnError {
    if has_key {
        e
    } else {
        PhnError::Network(format!(
            "{e}. The Census API may require a free key from this network; set {KEY_ENV} \
             (sign up at {KEY_SIGNUP})"
        ))
    }
}

/// Validator for API bodies: JSON array of arrays with a header row.
pub fn validate_census_body(body: &[u8]) -> Result<()> {
    expect_json(body)?;
    let v: serde_json::Value = serde_json::from_slice(body)
        .map_err(|e| PhnError::Network(format!("Census API: {e}")))?;
    match v.as_array() {
        Some(rows) if rows.len() >= 2 && rows[0].is_array() => Ok(()),
        _ => Err(PhnError::Network(
            "Census API returned no rows for this geography".into(),
        )),
    }
}

/// Parse an API response for one geography into (NAME, cells).
pub fn parse_api_response(body: &[u8]) -> Result<(String, Cells)> {
    let v: Vec<Vec<serde_json::Value>> = serde_json::from_slice(body)
        .map_err(|e| PhnError::Network(format!("Census API returned unexpected JSON: {e}")))?;
    let (header, rows) = v
        .split_first()
        .ok_or_else(|| PhnError::Network("Census API returned no header".into()))?;
    let row = rows
        .first()
        .ok_or_else(|| PhnError::Network("Census API returned no data row".into()))?;
    let text = |val: &serde_json::Value| -> Option<String> {
        match val {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            _ => None,
        }
    };
    let mut name = String::new();
    let mut est: HashMap<String, f64> = HashMap::new();
    let mut moe: HashMap<String, f64> = HashMap::new();
    for (h, val) in header.iter().zip(row.iter()) {
        let Some(h) = h.as_str() else { continue };
        let Some(t) = text(val) else { continue };
        if h == "NAME" {
            name = t;
            continue;
        }
        let Ok(x) = t.parse::<f64>() else { continue };
        if let Some(cell) = h.strip_suffix('E') {
            est.insert(cell.to_string(), x);
        } else if let Some(cell) = h.strip_suffix('M') {
            moe.insert(cell.to_string(), x);
        }
    }
    let mut cells = Cells::new();
    for (cell, e) in est {
        if let Some(value) = decode_estimate(e) {
            cells.insert(
                cell.clone(),
                Estimate {
                    value,
                    moe: moe.get(&cell).and_then(|m| decode_moe(*m)),
                },
            );
        }
    }
    Ok((name, cells))
}

/// Check whether a newer ACS 5-year vintage than the snapshot is published,
/// using the (keyless) variable metadata endpoint. Returns the newest vintage
/// found, or the snapshot vintage.
pub async fn latest_vintage(http: &HttpClient, cache: Option<&Cache>) -> u32 {
    let mut best = SNAPSHOT_VINTAGE;
    for v in [SNAPSHOT_VINTAGE + 1, SNAPSHOT_VINTAGE + 2] {
        let url = format!("{API_BASE}/{v}/acs/acs5/variables/B01003_001E.json");
        let ok = apifetch::fetch_validated(http, cache, &url, None, 7 * 24 * 3600, &expect_json, None)
            .await
            .is_ok();
        if ok {
            best = v;
        } else {
            break;
        }
    }
    best
}

/// A snapshot fallback for nothing (helper for callers that only want the chain).
pub fn no_snapshot() -> Option<SnapshotFallback> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(value: f64, moe: f64) -> Estimate {
        Estimate {
            value,
            moe: Some(moe),
        }
    }

    #[test]
    fn handbook_formulas() {
        // Sum: sqrt(10² + 20²) = 22.36.
        let s = sum(&[e(100.0, 10.0), e(200.0, 20.0)]);
        assert_eq!(s.value, 300.0);
        assert!((s.moe.unwrap() - 22.3607).abs() < 1e-3);
        // Several zero estimates contribute only the largest zero MOE.
        let z = sum(&[e(0.0, 11.0), e(0.0, 17.0), e(50.0, 12.0)]);
        assert!((z.moe.unwrap() - (17.0f64.powi(2) + 12.0f64.powi(2)).sqrt()).abs() < 1e-9);
        // Proportion: p = 0.25, sqrt(30² − 0.0625·40²)/400 = 0.0707.
        let p = proportion(e(100.0, 30.0), e(400.0, 40.0)).unwrap();
        assert!((p.value - 0.25).abs() < 1e-12);
        assert!((p.moe.unwrap() - (900.0f64 - 0.0625 * 1600.0).sqrt() / 400.0).abs() < 1e-12);
        // Negative radicand falls back to the ratio formula.
        let p = proportion(e(90.0, 5.0), e(100.0, 40.0)).unwrap();
        assert!((p.moe.unwrap() - (25.0f64 + 0.81 * 1600.0).sqrt() / 100.0).abs() < 1e-12);
        // CV and reliability.
        let x = e(100.0, 60.0);
        assert!((x.cv().unwrap() - 36.474).abs() < 1e-3);
        assert!(x.unreliable());
        assert!(!e(1000.0, 60.0).unreliable());
        assert_eq!(decode_moe(-555555555.0), Some(0.0));
        assert_eq!(decode_moe(-222222222.0), None);
        assert_eq!(decode_estimate(-666666666.0), None);
    }

    #[test]
    fn indicators_are_well_formed() {
        for d in INDICATORS {
            for c in d.num.iter().chain(d.den.iter()) {
                assert!(c.starts_with(d.table), "{} uses {c}", d.id);
                assert_eq!(c.len(), 10, "{c}");
            }
            match d.kind {
                IndicatorKind::Percent => assert!(!d.den.is_empty(), "{}", d.id),
                _ => assert!(d.den.is_empty(), "{}", d.id),
            }
        }
        let cells = required_cells();
        assert!(cells.len() > 100 && cells.len() < 200, "{}", cells.len());
        assert_eq!(detailed_lep_line("B16001_003"), "B16001_005");
    }

    #[test]
    fn computes_percent_indicators_from_cells() {
        let mut cells = Cells::new();
        cells.insert("B25044_001".into(), e(1000.0, 50.0));
        cells.insert("B25044_003".into(), e(40.0, 15.0));
        cells.insert("B25044_010".into(), e(60.0, 20.0));
        let v = compute(indicator("no_vehicle_pct").unwrap(), &cells).unwrap();
        assert!((v.value - 10.0).abs() < 1e-9);
        // num MOE = 25; p = .1; sqrt(625 − .01·2500)/1000·100 = 2.449
        assert!((v.moe.unwrap() - (600.0f64).sqrt() / 10.0).abs() < 1e-9);
        assert!(compute(indicator("crowding_pct").unwrap(), &cells).is_none());
    }

    #[test]
    fn parses_api_responses() {
        let body = br#"[["NAME","B01003_001E","B01003_001M","B19013_001E","B19013_001M","state"],
                        ["Massachusetts","7084891","-555555555","103960","527","25"]]"#;
        validate_census_body(body).unwrap();
        let (name, cells) = parse_api_response(body).unwrap();
        assert_eq!(name, "Massachusetts");
        assert_eq!(cells["B01003_001"], e(7084891.0, 0.0));
        assert_eq!(cells["B19013_001"], e(103960.0, 527.0));
        assert!(validate_census_body(b"<html>missing key</html>").is_err());
        let api = AcsApi::new(2024);
        let urls = api.urls(&ApiGeo::from_geoid(GeoLevel::Town, "2501513660").unwrap());
        assert!(urls[0].starts_with("https://api.census.gov/data/2024/acs/acs5?get=NAME,"));
        assert!(urls[0].contains("&for=county%20subdivision%3A13660&in=state%3A25%20county%3A015"));
        assert!(urls.iter().all(|u| !u.contains("key=")));
        assert!(urls.iter().all(|u| u.matches(',').count() <= 49));
    }

    #[test]
    fn bundled_snapshot_has_every_town_and_county() {
        let snap = AcsSnapshot::load(&SnapshotStore::bundled()).unwrap();
        let towns = snap.rows.values().filter(|r| r.level == GeoLevel::Town).count();
        let counties = snap.rows.values().filter(|r| r.level == GeoLevel::County).count();
        assert_eq!(towns, 351);
        assert_eq!(counties, 14);
        let ma = snap.state().unwrap();
        assert!(ma.values["pop_total"].value > 6_500_000.0);
        assert!(snap.nation().unwrap().values["pop_total"].value > 300_000_000.0);
        assert!(snap.detailed_languages.iter().any(|d| d.language == "Portuguese" && d.geoid == "25"));
    }
}
