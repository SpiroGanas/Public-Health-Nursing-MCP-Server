//! School-year calendar and grade logic shared by the school health tools.
//!
//! * A school year runs from July 1 to June 30 (the tool's convention; DPH's
//!   school health reports cover September through June, and year-end reports
//!   are due in July and August, so a July boundary keeps a whole school year
//!   together). Screenings done in July or August count toward the coming
//!   school year.
//! * Grades are PK, K, and 1-12; "ungraded" classrooms use the age rules in
//!   105 CMR 200.000.

use chrono::{Datelike, NaiveDate, Weekday};
use phn_core::{PhnError, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A school year, e.g. 2026-2027 (July 1, 2026 to June 30, 2027).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SchoolYear {
    /// Calendar year in which the school year starts.
    pub start_year: i32,
}

impl SchoolYear {
    /// The school year containing a date.
    pub fn of(d: NaiveDate) -> Self {
        Self {
            start_year: if d.month() >= 7 {
                d.year()
            } else {
                d.year() - 1
            },
        }
    }

    /// Parse "2026-2027", "2026-27", or "2026".
    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim();
        let first: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
        let y: i32 = first.parse().ok()?;
        if !(1990..=2100).contains(&y) {
            return None;
        }
        Some(Self { start_year: y })
    }

    /// First day (July 1).
    pub fn start(self) -> NaiveDate {
        NaiveDate::from_ymd_opt(self.start_year, 7, 1).expect("valid date")
    }

    /// Last day (June 30).
    pub fn end(self) -> NaiveDate {
        NaiveDate::from_ymd_opt(self.start_year + 1, 6, 30).expect("valid date")
    }

    /// The school year `n` years earlier.
    pub fn minus(self, n: i32) -> Self {
        Self {
            start_year: self.start_year - n,
        }
    }

    /// True when the date falls in this school year.
    pub fn contains(self, d: NaiveDate) -> bool {
        d >= self.start() && d <= self.end()
    }

    /// Label such as "2026-2027".
    pub fn label(self) -> String {
        format!("{}-{}", self.start_year, self.start_year + 1)
    }
}

/// A grade level.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub enum Grade {
    /// Preschool (PK, K0, K1).
    PK,
    /// Kindergarten.
    K,
    /// Grades 1-12.
    G(u8),
    /// Ungraded classroom (rules by age).
    Ungraded,
}

impl Grade {
    /// Parse a grade label as found in rosters ("Grade 3", "03", "KG", "PK").
    pub fn parse(s: &str) -> Result<Self> {
        let g = s.trim().to_ascii_uppercase();
        let g = g
            .trim_start_matches("GRADE")
            .trim_start_matches("GR.")
            .trim_start_matches("GR")
            .trim()
            .to_string();
        Ok(match g.as_str() {
            "PK" | "PRE-K" | "PREK" | "K0" | "K1" | "PRESCHOOL" | "PS" => Grade::PK,
            "K" | "K2" | "KG" | "KINDERGARTEN" => Grade::K,
            "UNGRADED" | "U" | "UG" | "SP" | "SPED" => Grade::Ungraded,
            other => {
                let n: u8 = other.parse().map_err(|_| {
                    PhnError::InvalidInput(format!(
                        "grade '{s}' is not recognized; use PK, K, 1-12, or ungraded"
                    ))
                })?;
                if !(1..=12).contains(&n) {
                    return Err(PhnError::InvalidInput(format!(
                        "grade '{s}' is out of range; use PK, K, 1-12, or ungraded"
                    )));
                }
                Grade::G(n)
            }
        })
    }

    /// Label used in content and tables ("PK", "K", "1".."12", "ungraded").
    pub fn label(self) -> String {
        match self {
            Grade::PK => "PK".into(),
            Grade::K => "K".into(),
            Grade::G(n) => n.to_string(),
            Grade::Ungraded => "ungraded".into(),
        }
    }

    /// Ordinal for sorting and band arithmetic: PK = -1, K = 0, 1..12.
    pub fn ordinal(self) -> i32 {
        match self {
            Grade::PK => -1,
            Grade::K => 0,
            Grade::G(n) => i32::from(n),
            Grade::Ungraded => 99,
        }
    }

    /// The grade with a given ordinal.
    pub fn from_ordinal(o: i32) -> Option<Self> {
        match o {
            -1 => Some(Grade::PK),
            0 => Some(Grade::K),
            1..=12 => Some(Grade::G(o as u8)),
            _ => None,
        }
    }

    /// Every graded level, PK through 12.
    pub fn all() -> Vec<Grade> {
        (-1..=12).filter_map(Grade::from_ordinal).collect()
    }
}

/// Age in whole years on a date.
pub fn age_years(dob: NaiveDate, on: NaiveDate) -> i32 {
    crate::vax::dates::age_years(dob, on)
}

/// Add `n` business days (Monday-Friday) to a date. Holidays are not known.
pub fn add_business_days(d: NaiveDate, n: u32) -> NaiveDate {
    let mut out = d;
    let mut left = n;
    while left > 0 {
        out = out.succ_opt().expect("date in range");
        if !matches!(out.weekday(), Weekday::Sat | Weekday::Sun) {
            left -= 1;
        }
    }
    out
}

/// The next school-week day on or after `d` (skips Saturday and Sunday).
pub fn next_weekday(d: NaiveDate) -> NaiveDate {
    let mut out = d;
    while matches!(out.weekday(), Weekday::Sat | Weekday::Sun) {
        out = out.succ_opt().expect("date in range");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn school_years_run_july_to_june() {
        assert_eq!(SchoolYear::of(d("2026-09-02")).label(), "2026-2027");
        assert_eq!(SchoolYear::of(d("2027-06-30")).label(), "2026-2027");
        assert_eq!(SchoolYear::of(d("2026-06-30")).label(), "2025-2026");
        assert_eq!(SchoolYear::parse("2025-26").unwrap().label(), "2025-2026");
        assert!(SchoolYear::of(d("2026-09-02")).contains(d("2027-01-15")));
        assert_eq!(
            SchoolYear::parse("2026-2027").unwrap().minus(2).label(),
            "2024-2025"
        );
    }

    #[test]
    fn grades_parse_and_order() {
        assert_eq!(Grade::parse("Grade 03").unwrap(), Grade::G(3));
        assert_eq!(Grade::parse("kg").unwrap(), Grade::K);
        assert_eq!(Grade::parse("K1").unwrap(), Grade::PK);
        assert_eq!(Grade::parse("ungraded").unwrap(), Grade::Ungraded);
        assert!(Grade::parse("13").is_err());
        assert!(Grade::PK < Grade::K && Grade::K < Grade::G(1));
        assert_eq!(Grade::all().len(), 14);
    }

    #[test]
    fn business_days_skip_weekends() {
        // Friday + 1 business day = Monday.
        assert_eq!(add_business_days(d("2026-10-02"), 1), d("2026-10-05"));
        assert_eq!(add_business_days(d("2026-10-05"), 1), d("2026-10-06"));
        assert_eq!(next_weekday(d("2026-10-03")), d("2026-10-05"));
    }
}
