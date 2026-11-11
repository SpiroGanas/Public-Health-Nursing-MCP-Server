//! De-identification under the HIPAA Safe Harbor method, or preparation of a
//! limited data set.
//!
//! **Safe Harbor** (45 CFR 164.514(b)(2)) removes the 18 identifiers: names;
//! geographic units smaller than a state (except the first three ZIP digits
//! when that area has more than 20,000 people); all date elements except year,
//! and ages over 89; contact numbers and addresses; record, plan, account,
//! license, vehicle, and device numbers; URLs and IP addresses; biometrics;
//! photographs; and any other unique code.
//!
//! A **limited data set** (45 CFR 164.514(e)) may keep dates, town, county,
//! and five-digit ZIP codes, but may be shared only under a data use
//! agreement. It still removes direct identifiers.

use std::collections::{BTreeMap, HashMap};

use chrono::{Datelike, NaiveDate};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::phi::{self, ColumnFinding, Identifier, RedactionLevel, is_missing, parse_date};
use crate::table::Table;

/// Three-digit ZIP prefixes whose combined population was 20,000 or fewer
/// (2000 Census), which Safe Harbor requires be replaced with "000". Listed in
/// HHS guidance on de-identification.
pub const RESTRICTED_ZIP3: [&str; 17] = [
    "036", "059", "063", "102", "203", "556", "692", "790", "821", "823", "830", "831", "878",
    "879", "884", "890", "893",
];

/// De-identification method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeidMode {
    /// HIPAA Safe Harbor.
    #[default]
    SafeHarbor,
    /// Limited data set (keeps dates, town, county, ZIP; requires a data use agreement).
    LimitedDataSet,
}

/// How to replace a date of birth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BirthDateHandling {
    /// Age in whole years at the reference date (90+ when 90 or older).
    #[default]
    Age,
    /// Year of birth (withheld as "90+" when the person is 90 or older).
    Year,
}

/// Options for [`deidentify`].
#[derive(Debug, Clone)]
pub struct DeidOptions {
    pub mode: DeidMode,
    /// Date used to compute ages (normally today).
    pub reference_date: NaiveDate,
    pub birth_date: BirthDateHandling,
    /// Columns to keep unchanged even if flagged (makes output non-compliant).
    pub keep_columns: Vec<String>,
    /// Extra columns to drop.
    pub drop_columns: Vec<String>,
    /// Replace ID columns with new sequential codes instead of dropping them.
    pub replace_ids_with_codes: bool,
    /// Keep free-text columns after strict redaction instead of dropping them.
    pub scrub_free_text: bool,
    /// Group size below which a combination of quasi-identifiers is "rare".
    pub k_threshold: u32,
}

/// What happened to a column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ColumnAction {
    Kept,
    Dropped,
    /// Dates reduced to year.
    YearOnly,
    /// Date of birth replaced by age in years.
    AgeFromBirthDate,
    /// Date of birth replaced by year of birth.
    BirthYear,
    /// ZIP reduced to three digits (or 000).
    Zip3,
    /// Ages over 89 replaced by "90+".
    AgeTopCoded,
    /// Replaced by new sequential codes.
    Pseudonymized,
    /// Free text kept after redacting detected identifiers.
    FreeTextScrubbed,
    /// Kept unchanged at the nurse's request although it is an identifier.
    KeptByRequest,
}

/// Report for one column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ColumnReport {
    pub column: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier: Option<Identifier>,
    pub action: ColumnAction,
    pub detail: String,
    pub values_changed: usize,
}

/// Rare combinations of quasi-identifiers in the output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RareCombinations {
    pub quasi_identifiers: Vec<String>,
    pub k: u32,
    /// Records whose combination is shared by fewer than k records.
    pub records_in_small_groups: usize,
}

/// Result of de-identification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeidResult {
    pub table: Table,
    pub columns: Vec<ColumnReport>,
    pub mode: DeidMode,
    /// False when the nurse forced identifier columns to be kept.
    pub safe_harbor_compliant: bool,
    pub warnings: Vec<String>,
    pub rare_combinations: Option<RareCombinations>,
}

/// First three ZIP digits under the Safe Harbor rule: "000" for the
/// low-population prefixes; a leading zero dropped by a spreadsheet is restored.
pub fn zip3(raw: &str) -> Option<String> {
    let digits: String = raw.chars().take_while(|c| c.is_ascii_digit()).collect();
    let five = match digits.len() {
        5 => digits,
        4 => format!("0{digits}"), // Excel dropped the leading zero
        3 => format!("00{digits}"),
        9 => digits[..5].to_string(),
        _ => return None,
    };
    let p = &five[..3];
    Some(if RESTRICTED_ZIP3.contains(&p) {
        "000".into()
    } else {
        p.to_string()
    })
}

/// Age in whole years on a date.
pub fn age_on(dob: NaiveDate, at: NaiveDate) -> i32 {
    let mut age = at.year() - dob.year();
    if (at.month(), at.day()) < (dob.month(), dob.day()) {
        age -= 1;
    }
    age
}

fn is_quasi_header(h: &str) -> bool {
    let n = phn_core::text::normalize(h);
    n.split(' ').any(|t| {
        matches!(
            t,
            "sex"
                | "gender"
                | "race"
                | "ethnicity"
                | "hispanic"
                | "language"
                | "grade"
                | "occupation"
        )
    })
}

/// De-identify a table.
pub fn deidentify(table: &Table, findings: &[ColumnFinding], opts: &DeidOptions) -> DeidResult {
    let find: HashMap<usize, &ColumnFinding> = findings.iter().map(|f| (f.index, f)).collect();
    let lc = |v: &[String]| {
        v.iter()
            .map(|s| s.trim().to_ascii_lowercase())
            .collect::<Vec<_>>()
    };
    let keep = lc(&opts.keep_columns);
    let drop = lc(&opts.drop_columns);
    let lds = opts.mode == DeidMode::LimitedDataSet;

    let mut out_headers = Vec::new();
    let mut out_cols: Vec<Vec<String>> = Vec::new();
    let mut reports = Vec::new();
    let mut warnings = Vec::new();
    let mut compliant = true;
    let mut quasi: Vec<usize> = Vec::new(); // indices into out_cols

    for (i, header) in table.headers.iter().enumerate() {
        let col: Vec<&str> = table.column(i).collect();
        let kind = find.get(&i).map(|f| f.kind);
        let h = header.trim().to_ascii_lowercase();
        let mut report = ColumnReport {
            column: header.clone(),
            identifier: kind,
            action: ColumnAction::Kept,
            detail: String::new(),
            values_changed: 0,
        };
        let push = |out_headers: &mut Vec<String>,
                    out_cols: &mut Vec<Vec<String>>,
                    name: String,
                    values: Vec<String>| {
            out_headers.push(name);
            out_cols.push(values);
            out_cols.len() - 1
        };

        if drop.contains(&h) {
            report.action = ColumnAction::Dropped;
            report.detail = "dropped at your request".into();
            reports.push(report);
            continue;
        }
        if keep.contains(&h) {
            if kind.is_some() {
                compliant = false;
                report.action = ColumnAction::KeptByRequest;
                report.detail = format!(
                    "kept unchanged at your request although it may contain {}; the output is NOT de-identified",
                    kind.map(|k| k.label()).unwrap_or("identifiers")
                );
            } else {
                report.detail = "kept".into();
            }
            push(
                &mut out_headers,
                &mut out_cols,
                header.clone(),
                col.iter().map(|s| s.to_string()).collect(),
            );
            reports.push(report);
            continue;
        }

        match kind {
            None => {
                // Scrub stray identifiers in kept text cells.
                let mut changed = 0;
                let values: Vec<String> = col
                    .iter()
                    .map(|v| {
                        if v.len() < 6 || v.parse::<f64>().is_ok() {
                            return v.to_string();
                        }
                        let r = phi::redact_text(v, RedactionLevel::Strict);
                        // Safe Harbor also removes dates and ZIP codes found
                        // inside text; a limited data set may keep them.
                        let high = if lds {
                            r.redacted.keys().any(|k| k.is_direct())
                        } else {
                            !r.redacted.is_empty()
                        };
                        if high {
                            changed += 1;
                            r.text
                        } else {
                            v.to_string()
                        }
                    })
                    .collect();
                report.values_changed = changed;
                report.detail = if changed > 0 {
                    format!("kept; identifiers found inside {changed} cell(s) were redacted")
                } else {
                    "kept (no identifiers detected)".into()
                };
                let idx = push(&mut out_headers, &mut out_cols, header.clone(), values);
                if is_quasi_header(header) {
                    quasi.push(idx);
                }
            }
            Some(Identifier::Date) => {
                if lds {
                    report.detail = "kept (limited data set)".into();
                    let idx = push(
                        &mut out_headers,
                        &mut out_cols,
                        header.clone(),
                        col.iter().map(|s| s.to_string()).collect(),
                    );
                    quasi.push(idx);
                } else {
                    let mut changed = 0;
                    let mut unparsed = 0;
                    let values = col
                        .iter()
                        .map(|v| {
                            if is_missing(v) {
                                return String::new();
                            }
                            match parse_date(v) {
                                Some(d) => {
                                    changed += 1;
                                    d.year().to_string()
                                }
                                // A bare year is already year-only.
                                None if phi::plain_year(v).is_some() => v.trim().to_string(),
                                None => {
                                    unparsed += 1;
                                    String::new()
                                }
                            }
                        })
                        .collect();
                    report.action = ColumnAction::YearOnly;
                    report.values_changed = changed + unparsed;
                    report.detail = if unparsed > 0 {
                        format!(
                            "dates reduced to year; {unparsed} value(s) that were not recognizable dates were blanked"
                        )
                    } else {
                        "dates reduced to year".into()
                    };
                    let idx = push(
                        &mut out_headers,
                        &mut out_cols,
                        format!("{header} (year)"),
                        values,
                    );
                    quasi.push(idx);
                }
            }
            Some(Identifier::BirthDate) => {
                if lds {
                    report.detail = "kept (limited data set)".into();
                    let idx = push(
                        &mut out_headers,
                        &mut out_cols,
                        header.clone(),
                        col.iter().map(|s| s.to_string()).collect(),
                    );
                    quasi.push(idx);
                } else {
                    let mut top = 0;
                    let mut changed = 0;
                    let values = col
                        .iter()
                        .map(|v| match parse_date(v) {
                            Some(d) => {
                                changed += 1;
                                let age = age_on(d, opts.reference_date);
                                if age >= 90 {
                                    top += 1;
                                    "90+".into()
                                } else if opts.birth_date == BirthDateHandling::Age {
                                    age.max(0).to_string()
                                } else {
                                    d.year().to_string()
                                }
                            }
                            None => {
                                if !is_missing(v) {
                                    changed += 1;
                                }
                                String::new()
                            }
                        })
                        .collect();
                    let (name, action, what) = match opts.birth_date {
                        BirthDateHandling::Age => (
                            format!("Age at {}", opts.reference_date),
                            ColumnAction::AgeFromBirthDate,
                            "replaced by age in years",
                        ),
                        BirthDateHandling::Year => (
                            "Birth year".to_string(),
                            ColumnAction::BirthYear,
                            "replaced by year of birth",
                        ),
                    };
                    report.action = action;
                    report.values_changed = changed;
                    report.detail =
                        format!("{what}; {top} person(s) aged 90 or older shown as 90+");
                    let idx = push(&mut out_headers, &mut out_cols, name, values);
                    quasi.push(idx);
                }
            }
            Some(Identifier::Age) => {
                let mut top = 0;
                // "Over 89" is about years; ages in months, weeks, or days are kept.
                let years = phi::age_header_is_years(header);
                let values = col
                    .iter()
                    .map(|v| match v.trim().parse::<f64>() {
                        Ok(a) if a > 89.0 && !lds && years => {
                            top += 1;
                            "90+".into()
                        }
                        _ => v.to_string(),
                    })
                    .collect();
                if top > 0 {
                    report.action = ColumnAction::AgeTopCoded;
                }
                report.values_changed = top;
                report.detail = format!("{top} age(s) over 89 shown as 90+");
                let idx = push(&mut out_headers, &mut out_cols, header.clone(), values);
                quasi.push(idx);
            }
            Some(Identifier::ZipCode) => {
                if lds {
                    report.detail = "kept (limited data set)".into();
                    let idx = push(
                        &mut out_headers,
                        &mut out_cols,
                        header.clone(),
                        col.iter().map(|s| s.to_string()).collect(),
                    );
                    quasi.push(idx);
                } else {
                    let mut restricted = 0;
                    let mut changed = 0;
                    let values = col
                        .iter()
                        .map(|v| {
                            if is_missing(v) {
                                return String::new();
                            }
                            changed += 1;
                            match zip3(v) {
                                Some(z) => {
                                    if z == "000" {
                                        restricted += 1;
                                    }
                                    z
                                }
                                None => String::new(),
                            }
                        })
                        .collect();
                    report.action = ColumnAction::Zip3;
                    report.values_changed = changed;
                    report.detail = format!(
                        "reduced to the first three digits; {restricted} value(s) in low-population prefixes set to 000"
                    );
                    let idx = push(
                        &mut out_headers,
                        &mut out_cols,
                        format!("{header} (3-digit)"),
                        values,
                    );
                    quasi.push(idx);
                }
            }
            Some(Identifier::Geography) => {
                // Coordinates and geocodes locate a home as precisely as a
                // street address, which a limited data set may not keep.
                let coordinates = phn_core::text::normalize(header).split(' ').any(|t| {
                    matches!(
                        t,
                        "latitude"
                            | "longitude"
                            | "lat"
                            | "lon"
                            | "lng"
                            | "geocode"
                            | "coordinates"
                            | "xy"
                    )
                });
                if lds && coordinates {
                    report.action = ColumnAction::Dropped;
                    report.detail = "dropped: coordinates locate an address, which a limited data set may not keep".into();
                } else if lds {
                    report.detail = "kept (limited data set)".into();
                    let idx = push(
                        &mut out_headers,
                        &mut out_cols,
                        header.clone(),
                        col.iter().map(|s| s.to_string()).collect(),
                    );
                    quasi.push(idx);
                } else {
                    report.action = ColumnAction::Dropped;
                    report.detail = "dropped: town, county, and smaller areas are identifiers under Safe Harbor \
                                     (use a limited data set or aggregate counts by town instead)"
                        .into();
                }
            }
            Some(Identifier::OtherIdNumber) if opts.replace_ids_with_codes => {
                let mut map: HashMap<&str, String> = HashMap::new();
                let values = col
                    .iter()
                    .map(|v| {
                        if is_missing(v) {
                            return String::new();
                        }
                        let n = map.len() + 1;
                        map.entry(*v).or_insert_with(|| format!("R{n:05}")).clone()
                    })
                    .collect();
                report.action = ColumnAction::Pseudonymized;
                report.values_changed = col.iter().filter(|v| !is_missing(v)).count();
                report.detail = "replaced with new sequential codes that are not derived from the original IDs; no crosswalk is kept".into();
                push(
                    &mut out_headers,
                    &mut out_cols,
                    format!("{header} (new code)"),
                    values,
                );
            }
            Some(Identifier::FreeText) if opts.scrub_free_text => {
                let mut changed = 0;
                let values = col
                    .iter()
                    .map(|v| {
                        let r = phi::redact_text(v, RedactionLevel::Strict);
                        if !r.redacted.is_empty() {
                            changed += 1;
                        }
                        r.text
                    })
                    .collect();
                report.action = ColumnAction::FreeTextScrubbed;
                report.values_changed = changed;
                report.detail = format!(
                    "kept with {changed} cell(s) redacted; automated scrubbing can miss identifiers, so review before sharing"
                );
                warnings.push(format!(
                    "Free-text column '{header}' was kept after automated redaction. Read it before sharing: \
                     detectors can miss names and other details."
                ));
                push(&mut out_headers, &mut out_cols, header.clone(), values);
            }
            Some(k) => {
                report.action = ColumnAction::Dropped;
                report.detail = format!(
                    "dropped: {} — Safe Harbor item {}",
                    k.label(),
                    k.safe_harbor_item()
                );
            }
        }
        reports.push(report);
    }

    let nrows = table.rows.len();
    let rows: Vec<Vec<String>> = (0..nrows)
        .map(|r| out_cols.iter().map(|c| c[r].clone()).collect())
        .collect();

    // Rare combinations of quasi-identifiers ("actual knowledge" check).
    let rare_combinations = (!quasi.is_empty()).then(|| {
        let mut groups: BTreeMap<Vec<&str>, usize> = BTreeMap::new();
        for r in 0..nrows {
            let key: Vec<&str> = quasi.iter().map(|&c| out_cols[c][r].as_str()).collect();
            *groups.entry(key).or_default() += 1;
        }
        let small: usize = groups
            .values()
            .filter(|&&n| n < opts.k_threshold as usize)
            .sum();
        RareCombinations {
            quasi_identifiers: quasi.iter().map(|&c| out_headers[c].clone()).collect(),
            k: opts.k_threshold,
            records_in_small_groups: small,
        }
    });
    if let Some(rc) = &rare_combinations
        && rc.records_in_small_groups > 0
    {
        warnings.push(format!(
                "{} of {nrows} record(s) have a combination of {} shared by fewer than {} records. \
                 Rare combinations can identify people even after Safe Harbor; consider age groups, \
                 removing a column, or sharing aggregates instead.",
                rc.records_in_small_groups,
                rc.quasi_identifiers.join(", "),
                rc.k
            ));
    }
    if lds {
        warnings.push(
            "A limited data set is still protected health information: share it only under a \
             data use agreement (45 CFR 164.514(e))."
                .into(),
        );
    }

    DeidResult {
        table: Table {
            name: table.name.clone(),
            headers: out_headers,
            rows,
            truncated: table.truncated,
        },
        columns: reports,
        mode: opts.mode,
        safe_harbor_compliant: compliant && !lds,
        warnings,
        rare_combinations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(mode: DeidMode) -> DeidOptions {
        DeidOptions {
            mode,
            reference_date: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            birth_date: BirthDateHandling::Age,
            keep_columns: vec![],
            drop_columns: vec![],
            replace_ids_with_codes: true,
            scrub_free_text: false,
            k_threshold: 5,
        }
    }

    fn sample() -> Table {
        Table {
            name: "t".into(),
            headers: [
                "Case ID", "Name", "DOB", "Zip", "Town", "Onset", "Sex", "Notes", "Dx",
            ]
            .map(String::from)
            .to_vec(),
            rows: [
                [
                    "C-1",
                    "Maria Garcia",
                    "1930-05-01",
                    "1002",
                    "Amherst",
                    "2024-01-03",
                    "F",
                    "lives w/ son",
                    "Pertussis",
                ],
                [
                    "C-2",
                    "James Smith",
                    "2015-03-02",
                    "03601",
                    "Boston",
                    "2024-02-11",
                    "M",
                    "",
                    "Pertussis seen by Dr. Wong, call 617-555-0100",
                ],
            ]
            .iter()
            .map(|r| r.iter().map(|s| s.to_string()).collect())
            .collect(),
            truncated: false,
        }
    }

    #[test]
    fn safe_harbor_transforms() {
        let t = sample();
        let f = phi::scan_table(&t);
        let r = deidentify(&t, &f, &opts(DeidMode::SafeHarbor));
        let out = &r.table;
        assert_eq!(
            out.headers,
            vec![
                "Case ID (new code)",
                "Age at 2025-01-01",
                "Zip (3-digit)",
                "Onset (year)",
                "Sex",
                "Dx"
            ]
        );
        assert_eq!(
            out.rows[0],
            vec!["R00001", "90+", "010", "2024", "F", "Pertussis"]
        );
        assert_eq!(out.rows[1][2], "000"); // 036 is a restricted prefix
        assert_eq!(out.rows[1][1], "9");
        assert!(
            !out.rows[1][5].contains("617-555-0100"),
            "{}",
            out.rows[1][5]
        );
        let json = serde_json::to_string(&r).unwrap();
        for leaked in ["Garcia", "Amherst", "1930-05-01", "C-1\""] {
            assert!(!json.contains(leaked), "{leaked} leaked");
        }
        assert!(r.safe_harbor_compliant);
        assert!(r.rare_combinations.unwrap().records_in_small_groups == 2);
    }

    #[test]
    fn limited_data_set_keeps_dates_and_town() {
        let t = sample();
        let f = phi::scan_table(&t);
        let r = deidentify(&t, &f, &opts(DeidMode::LimitedDataSet));
        assert!(r.table.headers.contains(&"Town".to_string()));
        assert!(r.table.headers.contains(&"DOB".to_string()));
        assert!(!r.table.headers.contains(&"Name".to_string()));
        assert!(!r.safe_harbor_compliant);
    }

    #[test]
    fn keep_override_marks_non_compliant() {
        let t = sample();
        let f = phi::scan_table(&t);
        let mut o = opts(DeidMode::SafeHarbor);
        o.keep_columns = vec!["town".into()];
        let r = deidentify(&t, &f, &o);
        assert!(!r.safe_harbor_compliant);
        assert!(r.table.headers.contains(&"Town".to_string()));
    }

    #[test]
    fn dates_in_text_coordinates_and_ages_in_months() {
        let t = Table {
            name: "t".into(),
            headers: ["Dx", "Latitude", "Age (months)", "Visit date"]
                .map(String::from)
                .to_vec(),
            rows: [
                ["Pertussis, onset 3/7/2024", "42.3732", "96", "2024"],
                ["Pertussis", "42.3601", "14", "20240311"],
            ]
            .iter()
            .map(|r| r.iter().map(|s| s.to_string()).collect())
            .collect(),
            truncated: false,
        };
        let f = phi::scan_table(&t);
        let r = deidentify(&t, &f, &opts(DeidMode::SafeHarbor));
        // The date inside the text cell is removed; the months column is not top-coded;
        // a bare year stays and a compact date is reduced to its year.
        assert!(
            !r.table.rows[0][0].contains("3/7/2024"),
            "{:?}",
            r.table.rows[0]
        );
        assert_eq!(
            r.table.headers,
            vec!["Dx", "Age (months)", "Visit date (year)"]
        );
        assert_eq!(r.table.rows[0][1], "96");
        assert_eq!(r.table.rows[0][2], "2024");
        assert_eq!(r.table.rows[1][2], "2024");
        // A limited data set keeps towns but not coordinates.
        let lds = deidentify(&t, &f, &opts(DeidMode::LimitedDataSet));
        assert!(!lds.table.headers.contains(&"Latitude".to_string()));
    }

    #[test]
    fn zip3_rules() {
        assert_eq!(zip3("01002-1234").as_deref(), Some("010"));
        assert_eq!(zip3("2139").as_deref(), Some("021"));
        assert_eq!(zip3("05901").as_deref(), Some("000"));
        assert_eq!(zip3("abc"), None);
    }
}
