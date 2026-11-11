//! Outbreak line-list column mapping shared by CD-04 and CD-05.
//!
//! Columns are found by name (onset date and time, case status or ill
//! yes/no, age, sex, town) and yes/no columns are split into symptoms and
//! exposures; every choice can be overridden. Identifier columns (names,
//! phone numbers, IDs) are never used, and no cell values are returned.

use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime};
use phn_core::text::normalize;
use phn_core::{PhnError, Result};
use phn_local::phi;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::localdata::{self, LocalTable};

/// Column choices and the case definition (all optional; columns are found
/// by name when not given).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct ColumnOptions {
    /// Column with the onset date (or date and time).
    #[serde(default)]
    pub onset_date_column: Option<String>,
    /// Column with the onset time (HH:MM), if separate from the date.
    #[serde(default)]
    pub onset_time_column: Option<String>,
    /// Column that says who is a case: a case status (confirmed, probable, …)
    /// or an ill yes/no column.
    #[serde(default)]
    pub case_column: Option<String>,
    /// Values of the case column that count as cases (default: confirmed and
    /// probable for a status column; yes for an ill column). Matching ignores
    /// case. In a status column, values such as "not a case", "well", or
    /// "ruled out" are non-cases; other values (e.g. suspect) are left out.
    #[serde(default)]
    pub case_values: Option<Vec<String>>,
    /// Case definition: also require at least one of these yes/no symptom
    /// columns to be yes (e.g. ["Diarrhea", "Vomiting"]).
    #[serde(default)]
    pub require_any_symptom: Option<Vec<String>>,
    /// Case definition: onset on or after this date (YYYY-MM-DD).
    #[serde(default)]
    pub onset_from: Option<String>,
    /// Case definition: onset on or before this date (YYYY-MM-DD).
    #[serde(default)]
    pub onset_to: Option<String>,
    /// Age column (years).
    #[serde(default)]
    pub age_column: Option<String>,
    /// Sex or gender column.
    #[serde(default)]
    pub sex_column: Option<String>,
    /// Town or city of residence column.
    #[serde(default)]
    pub town_column: Option<String>,
    /// Yes/no exposure columns (foods, events). Default: every yes/no column
    /// that is not a symptom, outcome, or the case column.
    #[serde(default)]
    pub exposure_columns: Option<Vec<String>>,
}

/// The columns used.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ColumnMap {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub onset_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub onset_time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case: Option<String>,
    /// Values counted as cases.
    pub case_values: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub age: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub town: Option<String>,
    pub symptoms: Vec<String>,
    pub exposures: Vec<String>,
    /// Columns skipped because they look like identifiers (names only).
    pub identifier_columns_skipped: Vec<String>,
    /// How each column was chosen.
    pub notes: Vec<String>,
}

/// One analyzed record (kept in memory only; never returned).
#[derive(Debug, Clone)]
pub struct Rec {
    /// `Some(true)` case, `Some(false)` non-case, `None` excluded by the case definition.
    pub case: Option<bool>,
    pub onset: Option<NaiveDateTime>,
    pub age: Option<f64>,
    pub sex: Option<String>,
    pub town: Option<String>,
    pub exposures: Vec<Option<bool>>,
    pub symptoms: Vec<Option<bool>>,
}

/// A mapped line list.
pub struct LineList {
    pub table: LocalTable,
    pub map: ColumnMap,
    pub recs: Vec<Rec>,
    /// True when onset times (not only dates) were available.
    pub has_times: bool,
    /// Records whose onset could not be read (among cases).
    pub cases_without_onset: usize,
}

const SYMPTOM_WORDS: &[&str] = &[
    "diarrhea",
    "diarrhoea",
    "vomit",
    "vomiting",
    "nausea",
    "fever",
    "cramp",
    "cramps",
    "abdominal",
    "bloody",
    "headache",
    "chills",
    "cough",
    "rash",
    "symptom",
    "symptoms",
    "fatigue",
    "myalgia",
    "jaundice",
    "sore",
];
const OUTCOME_WORDS: &[&str] = &[
    "hospitalized",
    "hospitalised",
    "hospital",
    "admitted",
    "died",
    "death",
    "deceased",
    "ed",
    "er",
    "visit",
    "tested",
    "test",
    "specimen",
    "lab",
    "positive",
    "outcome",
    "ill",
    "sick",
    "case",
];

fn words(h: &str) -> Vec<String> {
    normalize(h)
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

fn has_word(h: &str, list: &[&str]) -> bool {
    words(h).iter().any(|w| list.contains(&w.as_str()))
}

/// Status values that mean "not a case" (well attendees, ruled out, controls).
fn is_non_case(v: &str) -> bool {
    let w: Vec<&str> = v.split_whitespace().collect();
    matches!(
        v,
        "no" | "n" | "well" | "healthy" | "negative" | "control" | "ruled out" | "not ill"
    ) || w
        .first()
        .is_some_and(|f| matches!(*f, "not" | "non" | "noncase"))
        || v.contains("ruled out")
}

/// Years a date in a line list may fall in. chrono's `%Y` also reads a
/// two-digit year such as "26" as the year 26, so anything outside this range
/// is rejected and the two-digit-year formats are tried instead.
fn plausible_year(y: i32) -> bool {
    (1900..=2100).contains(&y)
}

/// Parse a date or date-time cell. Dates are read as month/day/year (U.S.
/// order); two-digit years are read as 1969-2068; years outside 1900-2100
/// are not accepted.
pub fn parse_datetime(s: &str) -> Option<(NaiveDateTime, bool)> {
    let s = s.trim();
    for fmt in [
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%m/%d/%Y %H:%M",
        "%m/%d/%Y %I:%M %p",
        "%m/%d/%y %H:%M",
        "%m/%d/%y %I:%M %p",
    ] {
        if let Ok(d) = NaiveDateTime::parse_from_str(s, fmt)
            && plausible_year(d.date().year())
        {
            // Midnight with seconds from spreadsheets means "date only".
            let date_only = d.time() == NaiveTime::MIN && s.ends_with("00:00:00");
            return Some((d, !date_only));
        }
    }
    if let Some(d) = phi::parse_date(s).filter(|d| plausible_year(d.year())) {
        return Some((d.and_time(NaiveTime::MIN), false));
    }
    // Two-digit years ("5/3/26"), which the four-digit formats misread.
    let head = s.split(['T', ' ']).next().unwrap_or(s);
    for fmt in ["%m/%d/%y", "%m-%d-%y"] {
        if let Ok(d) = NaiveDate::parse_from_str(head, fmt)
            && plausible_year(d.year())
        {
            return Some((d.and_time(NaiveTime::MIN), false));
        }
    }
    None
}

/// Parse a time cell (HH:MM, H:MM AM, H AM).
pub fn parse_time(s: &str) -> Option<NaiveTime> {
    let s = s.trim();
    for fmt in ["%H:%M", "%H:%M:%S", "%I:%M %p", "%I:%M%p", "%I %p", "%I%p"] {
        if let Ok(t) = NaiveTime::parse_from_str(&s.to_uppercase(), fmt) {
            return Some(t);
        }
    }
    None
}

fn is_identifier(lt: &LocalTable, idx: usize) -> bool {
    use phi::Identifier as I;
    lt.findings
        .iter()
        .any(|f| f.index == idx && !matches!(f.kind, I::Age | I::Date | I::Geography | I::ZipCode))
}

fn find_by_words(lt: &LocalTable, want: &[&[&str]], skip: &[usize]) -> Option<usize> {
    for pattern in want {
        for (i, h) in lt.table.headers.iter().enumerate() {
            if skip.contains(&i) || is_identifier(lt, i) {
                continue;
            }
            let w = words(h);
            if pattern.iter().all(|p| w.iter().any(|x| x == p)) {
                return Some(i);
            }
        }
    }
    None
}

/// Share of non-missing cells that read as yes/no.
fn yes_no_share(lt: &LocalTable, idx: usize) -> (f64, usize) {
    let mut present = 0;
    let mut ok = 0;
    for v in lt.table.column(idx) {
        if phi::is_missing(v) {
            continue;
        }
        present += 1;
        if localdata::truthy(v).is_some() {
            ok += 1;
        }
    }
    (
        if present == 0 {
            0.0
        } else {
            ok as f64 / present as f64
        },
        present,
    )
}

fn yes_no(lt: &LocalTable, idx: usize) -> Vec<Option<bool>> {
    lt.table
        .column(idx)
        .map(|v| {
            if phi::is_missing(v) {
                None
            } else {
                localdata::truthy(v)
            }
        })
        .collect()
}

fn date_arg(name: &str, v: &Option<String>) -> Result<Option<NaiveDate>> {
    super::engine::parse_date_arg(name, v)
}

/// Load and map a line list.
pub fn load(ctx: &Context, path: &str, sheet: Option<&str>, o: &ColumnOptions) -> Result<LineList> {
    let lt = localdata::load(ctx, path, sheet)?;
    let mut map = ColumnMap::default();
    let n = lt.table.rows.len();
    let mut used: Vec<usize> = Vec::new();

    // Onset date (and time).
    let onset_idx = match &o.onset_date_column {
        Some(c) => Some(lt.column(c)?),
        None => find_by_words(
            &lt,
            &[
                &["onset", "date"],
                &["onset"],
                &["symptom", "date"],
                &["illness", "date"],
            ],
            &[],
        ),
    };
    if let Some(i) = onset_idx {
        used.push(i);
        map.onset_date = Some(lt.table.headers[i].clone());
        if o.onset_date_column.is_none() {
            map.notes.push(format!(
                "Onset date: '{}' (found by name).",
                lt.table.headers[i]
            ));
        }
    }
    // A time column is used on its own evidence only when its name says it is
    // the onset time; "Meal Time" or "Interview Time" must be named by the caller.
    let time_idx = match &o.onset_time_column {
        Some(c) => Some(lt.column(c)?),
        None => find_by_words(
            &lt,
            &[
                &["onset", "time"],
                &["symptom", "time"],
                &["symptoms", "time"],
                &["illness", "time"],
            ],
            &used,
        ),
    };
    if let Some(i) = time_idx {
        used.push(i);
        map.onset_time = Some(lt.table.headers[i].clone());
        if o.onset_time_column.is_none() {
            map.notes.push(format!(
                "Onset time: '{}' (found by name).",
                lt.table.headers[i]
            ));
        }
    } else if let Some(i) = find_by_words(&lt, &[&["time"]], &used) {
        map.notes.push(format!(
            "The column '{}' was not used as the onset time because its name does not say \
             onset; if it holds onset times, name it with onset_time_column.",
            lt.table.headers[i]
        ));
    }

    // Case column.
    let case_idx = match &o.case_column {
        Some(c) => Some(lt.column(c)?),
        None => find_by_words(
            &lt,
            // An ill yes/no column is preferred to a bare "status" column,
            // which is often something else ("Vaccination Status").
            &[
                &["case", "status"],
                &["case", "classification"],
                &["classification"],
                &["ill"],
                &["sick"],
                &["status"],
                &["case"],
            ],
            &used,
        ),
    };
    let mut case_is_yes_no = false;
    if let Some(i) = case_idx {
        used.push(i);
        map.case = Some(lt.table.headers[i].clone());
        if o.case_column.is_none() {
            map.notes.push(format!(
                "Case column: '{}' (found by name).",
                lt.table.headers[i]
            ));
        }
        let (share, _) = yes_no_share(&lt, i);
        case_is_yes_no = share >= 0.9;
        map.case_values = match &o.case_values {
            Some(v) if !v.is_empty() => v.iter().map(|s| s.trim().to_string()).collect(),
            _ if case_is_yes_no => vec!["yes".into()],
            _ => vec!["confirmed".into(), "probable".into()],
        };
        if !case_is_yes_no {
            // Category values are checked so we never echo identifiers.
            lt.categorical(i)?;
        }
    } else if onset_idx.is_none() {
        return Err(PhnError::InvalidInput(format!(
            "could not find an onset date or case column in '{}'. Name them with \
             onset_date_column and case_column. Columns: {}",
            lt.label,
            usable_columns(&lt).join(", ")
        )));
    } else {
        map.notes.push(
            "No case column found: every record with an onset date is treated as a case, so no \
             attack rates can be calculated."
                .into(),
        );
    }

    // Demographics.
    let pick = |opt: &Option<String>, pats: &[&[&str]], used: &[usize]| -> Result<Option<usize>> {
        match opt {
            Some(c) => lt.column(c).map(Some),
            None => Ok(find_by_words(&lt, pats, used)),
        }
    };
    let age_idx = pick(&o.age_column, &[&["age"], &["age", "years"]], &used)?;
    if let Some(i) = age_idx {
        used.push(i);
        map.age = Some(lt.table.headers[i].clone());
    }
    let sex_idx = pick(&o.sex_column, &[&["sex"], &["gender"]], &used)?;
    if let Some(i) = sex_idx {
        used.push(i);
        map.sex = Some(lt.table.headers[i].clone());
        lt.categorical(i)?;
    }
    let town_idx = pick(
        &o.town_column,
        &[&["town"], &["city"], &["municipality"], &["residence"]],
        &used,
    )?;
    if let Some(i) = town_idx {
        used.push(i);
        map.town = Some(lt.table.headers[i].clone());
    }

    // Yes/no columns: symptoms, outcomes, exposures.
    let mut symptom_idx = Vec::new();
    let mut exposure_idx = Vec::new();
    for (i, h) in lt.table.headers.iter().enumerate() {
        if is_identifier(&lt, i) {
            map.identifier_columns_skipped.push(h.clone());
            continue;
        }
        if used.contains(&i) {
            continue;
        }
        let (share, present) = yes_no_share(&lt, i);
        if share < 0.9 || present == 0 {
            continue;
        }
        if has_word(h, SYMPTOM_WORDS) {
            symptom_idx.push(i);
        } else if has_word(h, OUTCOME_WORDS) {
            continue;
        } else {
            exposure_idx.push(i);
        }
    }
    if let Some(list) = &o.exposure_columns {
        exposure_idx = list
            .iter()
            .map(|c| lt.column(c))
            .collect::<Result<Vec<_>>>()?;
        for &i in &exposure_idx {
            if yes_no_share(&lt, i).0 < 0.9 {
                return Err(PhnError::InvalidInput(format!(
                    "exposure column '{}' is not a yes/no column",
                    lt.table.headers[i]
                )));
            }
        }
    }
    if exposure_idx.len() > 60 {
        return Err(PhnError::InvalidInput(format!(
            "{} yes/no columns look like exposures; name at most 60 with exposure_columns",
            exposure_idx.len()
        )));
    }
    let require: Vec<usize> = match &o.require_any_symptom {
        Some(v) => v.iter().map(|c| lt.column(c)).collect::<Result<Vec<_>>>()?,
        None => Vec::new(),
    };
    for &i in &require {
        if !symptom_idx.contains(&i) {
            symptom_idx.push(i);
        }
        exposure_idx.retain(|x| *x != i);
    }
    map.symptoms = symptom_idx
        .iter()
        .map(|&i| lt.table.headers[i].clone())
        .collect();
    map.exposures = exposure_idx
        .iter()
        .map(|&i| lt.table.headers[i].clone())
        .collect();

    // Records.
    let from = date_arg("onset_from", &o.onset_from)?;
    let to = date_arg("onset_to", &o.onset_to)?;
    let onset_vals: Vec<Option<(NaiveDateTime, bool)>> = match onset_idx {
        Some(i) => lt.table.column(i).map(parse_datetime).collect(),
        None => vec![None; n],
    };
    let time_vals: Vec<Option<NaiveTime>> = match time_idx {
        Some(i) => lt.table.column(i).map(parse_time).collect(),
        None => vec![None; n],
    };
    let ages: Vec<Option<f64>> = match age_idx {
        Some(i) => lt.numeric(i).0,
        None => vec![None; n],
    };
    let col_str = |idx: Option<usize>| -> Vec<Option<String>> {
        match idx {
            Some(i) => lt
                .table
                .column(i)
                .map(|s| (!phi::is_missing(s)).then(|| s.trim().to_string()))
                .collect(),
            None => vec![None; n],
        }
    };
    let sexes = col_str(sex_idx);
    let towns = col_str(town_idx);
    let case_raw = col_str(case_idx);
    let exp_cols: Vec<Vec<Option<bool>>> = exposure_idx.iter().map(|&i| yes_no(&lt, i)).collect();
    let sym_cols: Vec<Vec<Option<bool>>> = symptom_idx.iter().map(|&i| yes_no(&lt, i)).collect();
    let req_cols: Vec<Vec<Option<bool>>> = require.iter().map(|&i| yes_no(&lt, i)).collect();
    let wanted: Vec<String> = map.case_values.iter().map(|v| normalize(v)).collect();

    let mut has_times = false;
    let mut recs = Vec::with_capacity(n);
    for r in 0..n {
        let mut onset = onset_vals[r];
        if let (Some((d, false)), Some(t)) = (onset, time_vals[r]) {
            onset = Some((d.date().and_time(t), true));
        }
        if onset.is_some_and(|o| o.1) {
            has_times = true;
        }
        let mut case = match (&case_raw[r], case_idx) {
            (_, None) => onset.map(|_| true),
            (None, Some(_)) => None,
            (Some(v), Some(_)) => {
                if case_is_yes_no {
                    let t = localdata::truthy(v);
                    let want_yes = wanted
                        .iter()
                        .any(|w| matches!(w.as_str(), "yes" | "y" | "true" | "1"));
                    t.map(|b| b == want_yes)
                } else {
                    let nv = normalize(v);
                    if wanted.contains(&nv) {
                        Some(true)
                    } else if is_non_case(&nv) {
                        Some(false)
                    } else {
                        // Probable or suspect when only confirmed counts: left out.
                        None
                    }
                }
            }
        };
        if case == Some(true) {
            if !req_cols.is_empty() && !req_cols.iter().any(|c| c[r] == Some(true)) {
                case = None;
            }
            if let Some((d, _)) = onset {
                if from.is_some_and(|f| d.date() < f) || to.is_some_and(|t| d.date() > t) {
                    case = None;
                }
            } else if from.is_some() || to.is_some() {
                case = None;
            }
        }
        recs.push(Rec {
            case,
            onset: onset.map(|o| o.0),
            age: ages[r],
            sex: sexes[r].clone(),
            town: towns[r].clone(),
            exposures: exp_cols.iter().map(|c| c[r]).collect(),
            symptoms: sym_cols.iter().map(|c| c[r]).collect(),
        });
    }
    let cases_without_onset = recs
        .iter()
        .filter(|r| r.case == Some(true) && r.onset.is_none())
        .count();
    Ok(LineList {
        table: lt,
        map,
        recs,
        has_times,
        cases_without_onset,
    })
}

/// Column names that are not identifiers (for error messages).
pub fn usable_columns(lt: &LocalTable) -> Vec<String> {
    lt.table
        .headers
        .iter()
        .enumerate()
        .filter(|(i, _)| !is_identifier(lt, *i))
        .map(|(_, h)| h.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dates_and_times() {
        let (d, t) = parse_datetime("2026-05-03 14:00").unwrap();
        assert!(t);
        assert_eq!(d.format("%H").to_string(), "14");
        let (_, t) = parse_datetime("05/03/2026").unwrap();
        assert!(!t);
        assert_eq!(
            parse_time("2:30 pm").unwrap().format("%H:%M").to_string(),
            "14:30"
        );
        assert!(parse_time("later").is_none());
        // Two-digit years are 20YY, not the year 26.
        let (d, t) = parse_datetime("5/3/26 14:00").unwrap();
        assert!(t);
        assert_eq!(d.format("%Y-%m-%d %H:%M").to_string(), "2026-05-03 14:00");
        let (d, t) = parse_datetime("5/3/26").unwrap();
        assert!(!t);
        assert_eq!(d.format("%Y-%m-%d").to_string(), "2026-05-03");
        let (d, _) = parse_datetime("05/03/2026 2:30 PM").unwrap();
        assert_eq!(d.format("%Y-%m-%d %H:%M").to_string(), "2026-05-03 14:30");
        // Implausible years are refused rather than computed with.
        assert!(parse_datetime("0026-05-03").is_none());
        assert!(parse_datetime("+262142-12-31").is_none());
        assert!(parse_datetime("2101-01-01 10:00").is_none());
        assert!(has_word("Abdominal Cramps", SYMPTOM_WORDS));
        assert!(!has_word("Roast Turkey", SYMPTOM_WORDS));
    }
}
