//! Calendar arithmetic for immunization rules: ages and intervals in days,
//! weeks, calendar months, and years, and the CDC 4-day grace period.
//!
//! Conventions follow CDC's General Best Practice Guidelines (Table 3-2):
//! weeks are 7 days ("4 weeks = 28 days"); months and years are calendar
//! months and years, so "12 months" after 2025-01-31 is 2026-01-31. Dates are
//! added as in the CDSi Logic Specification (v4.6, Table 3-6), so one month
//! after January 31 is March 1 (a date that does not exist moves to the first
//! of the next month), matching the Phase 6 forecaster. A dose given up to 4
//! days before a minimum age or interval is valid when the grace period
//! applies. This module is deliberately small and free of vaccine knowledge so
//! it can later be shared with isolation-date rules (Phase 3) and the CDSi
//! engine (Phase 6).

use chrono::{Datelike, Duration, Months, NaiveDate};
use phn_core::{PhnError, Result};

/// Days in the CDC grace period.
pub const GRACE_DAYS: i64 = 4;

/// A calendar span such as "6 weeks" or "9 years + 5 months".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub years: u32,
    pub months: u32,
    pub weeks: u32,
    pub days: u32,
}

impl Span {
    /// Parse "birth", "28 days", "4 weeks", "12 months", "4 years", or a sum
    /// joined by "+" ("9 years + 5 months"). Returns `None` when unrecognized.
    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim().to_ascii_lowercase();
        if t == "birth" || t == "0" {
            return Some(Self::default());
        }
        let mut span = Self::default();
        for part in t.split('+') {
            let mut it = part.split_whitespace();
            let n: u32 = it.next()?.parse().ok()?;
            let unit = it.next()?;
            if it.next().is_some() {
                return None;
            }
            match unit.trim_end_matches('s') {
                "day" => span.days += n,
                "week" => span.weeks += n,
                "month" => span.months += n,
                "year" => span.years += n,
                _ => return None,
            }
        }
        Some(span)
    }

    /// Spans of whole years, months, weeks, or days.
    pub fn years(n: u32) -> Self {
        Self {
            years: n,
            ..Self::default()
        }
    }

    /// Calendar months.
    pub fn months(n: u32) -> Self {
        Self {
            months: n,
            ..Self::default()
        }
    }

    /// Weeks of 7 days.
    pub fn weeks(n: u32) -> Self {
        Self {
            weeks: n,
            ..Self::default()
        }
    }

    /// Days.
    pub fn days(n: u32) -> Self {
        Self {
            days: n,
            ..Self::default()
        }
    }

    /// Plain-language label ("4 years", "6 months", "28 days").
    pub fn label(&self) -> String {
        let mut parts = Vec::new();
        let unit =
            |n: u32, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        if self.years > 0 {
            parts.push(unit(self.years, "year", "years"));
        }
        if self.months > 0 {
            parts.push(unit(self.months, "month", "months"));
        }
        if self.weeks > 0 {
            parts.push(unit(self.weeks, "week", "weeks"));
        }
        if self.days > 0 {
            parts.push(unit(self.days, "day", "days"));
        }
        if parts.is_empty() {
            "birth".into()
        } else {
            parts.join(" + ")
        }
    }
}

/// `date + span`, following the CDC CDSi Logic Specification (v4.6, Table
/// 3-6): years and months first, holding the day of the month constant
/// (CALCDT-1, -2, -6); a date that does not exist moves forward to the first
/// day of the next month (CALCDT-5: March 31 + 6 months = October 1); then
/// weeks and days (CALCDT-3).
pub fn add(date: NaiveDate, span: Span) -> NaiveDate {
    let months = span.years * 12 + span.months;
    let first = date.with_day(1).expect("day 1 exists");
    let Some(target_month) = first.checked_add_months(Months::new(months)) else {
        return NaiveDate::MAX;
    };
    let d = target_month.with_day(date.day()).unwrap_or_else(|| {
        target_month
            .checked_add_months(Months::new(1))
            .unwrap_or(NaiveDate::MAX)
    });
    d + Duration::days(i64::from(span.weeks) * 7 + i64::from(span.days))
}

/// The date a person reaches an age.
pub fn date_at_age(dob: NaiveDate, age: Span) -> NaiveDate {
    add(dob, age)
}

/// Whether `given` meets a minimum `due` date, with or without the grace period.
pub fn meets(given: NaiveDate, due: NaiveDate, grace: bool) -> bool {
    let allowance = if grace { GRACE_DAYS } else { 0 };
    given >= due - Duration::days(allowance)
}

/// Days by which `given` falls short of `due` (0 when it is on time).
pub fn days_early(given: NaiveDate, due: NaiveDate) -> i64 {
    (due - given).num_days().max(0)
}

/// Age in completed years on a date.
pub fn age_years(dob: NaiveDate, on: NaiveDate) -> i32 {
    let mut y = on.year() - dob.year();
    if (on.month(), on.day()) < (dob.month(), dob.day()) {
        y -= 1;
    }
    y
}

/// Age in completed calendar months on a date.
pub fn age_months(dob: NaiveDate, on: NaiveDate) -> i32 {
    let mut m = (on.year() - dob.year()) * 12 + on.month() as i32 - dob.month() as i32;
    if on.day() < dob.day() {
        m -= 1;
    }
    m
}

/// Earliest year accepted from a typed or file date.
const MIN_YEAR: i32 = 1900;
/// Latest year accepted from a typed or file date.
const MAX_YEAR: i32 = 2100;

/// Keep a parsed date only when its year is plausible. `%Y` accepts one to
/// four digits, so "3/4/15" would otherwise be read as the year 15.
fn plausible(d: NaiveDate) -> Option<NaiveDate> {
    (MIN_YEAR..=MAX_YEAR).contains(&d.year()).then_some(d)
}

/// A two-digit year that would land more than a year in the future belongs
/// to the previous century ("5/2/50" is a birth date in 1950, not 2050). The
/// same rule is used by `phn_local::phi::parse_date`.
fn two_digit_century(d: NaiveDate) -> NaiveDate {
    let limit = chrono::Local::now().date_naive() + chrono::Duration::days(366);
    if d > limit {
        d.with_year(d.year() - 100).unwrap_or(d)
    } else {
        d
    }
}

/// Parse a date typed by a person or found in a file: ISO (2024-03-05),
/// US (3/5/2024, 03-05-2024, or 3/5/24), or an Excel serial number. Years
/// outside 1900-2100 are rejected. A two-digit year is read in the current
/// century unless that would be more than a year in the future, in which case
/// it is the previous century (3/4/15 is 2015; 5/2/50 is 1950).
pub fn parse_date(s: &str) -> Option<NaiveDate> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    // Four-digit years first, then two-digit years.
    for fmt in ["%Y-%m-%d", "%m/%d/%Y", "%m-%d-%Y", "%Y/%m/%d"] {
        if let Ok(d) = NaiveDate::parse_from_str(t, fmt)
            && let Some(d) = plausible(d)
        {
            return Some(d);
        }
    }
    for fmt in ["%m/%d/%y", "%m-%d-%y"] {
        if let Ok(d) = NaiveDate::parse_from_str(t, fmt)
            && let Some(d) = plausible(two_digit_century(d))
        {
            return Some(d);
        }
    }
    // ISO date-time ("2024-03-05T00:00:00" or "2024-03-05 00:00:00"). `get`
    // returns `None` when byte 10 is inside a multi-byte character.
    if t.len() > 10
        && let Some(head) = t.get(..10)
        && let Ok(d) = NaiveDate::parse_from_str(head, "%Y-%m-%d")
        && let Some(d) = plausible(d)
    {
        return Some(d);
    }
    if let Ok(serial) = t.parse::<f64>()
        && (20_000.0..80_000.0).contains(&serial)
    {
        return NaiveDate::parse_from_str(
            &phn_local::table::excel_serial_to_iso(serial),
            "%Y-%m-%d",
        )
        .ok()
        .and_then(plausible);
    }
    None
}

/// Parse a required date argument, with a helpful error.
pub fn require_date(field: &str, s: &str) -> Result<NaiveDate> {
    parse_date(s).ok_or_else(|| {
        PhnError::InvalidInput(format!(
            "{field} '{s}' is not a date; use YYYY-MM-DD (for example 2019-04-15)"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn parses_spans() {
        assert_eq!(Span::parse("6 weeks"), Some(Span::weeks(6)));
        assert_eq!(Span::parse("birth"), Some(Span::default()));
        assert_eq!(
            Span::parse("9 years + 5 months"),
            Some(Span {
                years: 9,
                months: 5,
                ..Span::default()
            })
        );
        assert_eq!(Span::parse("1 month"), Some(Span::months(1)));
        assert!(Span::parse("soon").is_none());
        assert!(Span::parse("4 fortnights").is_none());
        assert_eq!(Span::parse("4 years").unwrap().label(), "4 years");
    }

    #[test]
    fn calendar_arithmetic_follows_cdc_conventions() {
        // CDSi CALCDT-5: a date that does not exist moves to the first of the
        // next month (the specification's own examples).
        assert_eq!(add(d("2000-03-31"), Span::months(6)), d("2000-10-01"));
        assert_eq!(add(d("2000-08-31"), Span::months(6)), d("2001-03-01"));
        assert_eq!(add(d("2025-01-31"), Span::months(1)), d("2025-03-01"));
        assert_eq!(add(d("2024-02-29"), Span::years(1)), d("2025-03-01"));
        assert_eq!(add(d("2024-02-29"), Span::years(4)), d("2028-02-29"));
        // CALCDT-1 to -3 examples.
        assert_eq!(add(d("2000-01-01"), Span::years(3)), d("2003-01-01"));
        assert_eq!(add(d("2000-11-01"), Span::months(6)), d("2001-05-01"));
        assert_eq!(add(d("2000-02-01"), Span::weeks(5)), d("2000-03-07"));
        assert_eq!(add(d("2001-02-01"), Span::weeks(5)), d("2001-03-08"));
        // Months before days: 2000-01-31 + 6 months + 3 days.
        let span = Span {
            months: 6,
            days: 3,
            ..Span::default()
        };
        assert_eq!(add(d("2000-01-31"), span), d("2000-08-03"));
        // 4 weeks = 28 days.
        assert_eq!(add(d("2025-03-01"), Span::weeks(4)), d("2025-03-29"));
        assert_eq!(
            date_at_age(d("2020-06-15"), Span::years(4)),
            d("2024-06-15")
        );
    }

    #[test]
    fn grace_period_allows_four_days_not_five() {
        let due = d("2024-06-15");
        assert!(meets(d("2024-06-11"), due, true));
        assert!(!meets(d("2024-06-10"), due, true));
        assert!(!meets(d("2024-06-14"), due, false));
        assert_eq!(days_early(d("2024-06-10"), due), 5);
        assert_eq!(days_early(d("2024-06-20"), due), 0);
    }

    #[test]
    fn ages_and_dates() {
        assert_eq!(age_years(d("2015-09-01"), d("2026-08-31")), 10);
        assert_eq!(age_years(d("2015-09-01"), d("2026-09-01")), 11);
        assert_eq!(age_months(d("2025-01-31"), d("2025-02-28")), 0);
        assert_eq!(age_months(d("2025-01-15"), d("2026-01-15")), 12);
        assert_eq!(parse_date("3/5/2024"), Some(d("2024-03-05")));
        assert_eq!(parse_date("2024-03-05 00:00:00"), Some(d("2024-03-05")));
        assert_eq!(parse_date("45356"), Some(d("2024-03-05")));
        assert!(parse_date("unknown").is_none());
    }

    #[test]
    fn two_digit_years_and_implausible_dates() {
        // Two-digit years are read as 20YY unless that is more than a year in
        // the future (then 19YY), never as the year YY.
        assert_eq!(parse_date("10/15/26"), Some(d("2026-10-15")));
        assert_eq!(parse_date("5/2/50"), Some(d("1950-05-02")));
        assert_eq!(parse_date("3/4/15"), Some(d("2015-03-04")));
        assert_eq!(parse_date("03-04-15"), Some(d("2015-03-04")));
        assert_eq!(parse_date("6/1/98"), Some(d("1998-06-01")));
        // Four-digit years are unchanged.
        assert_eq!(parse_date("10/15/2026"), Some(d("2026-10-15")));
        // A date range with an en dash is not a date, and must not panic
        // (byte 10 falls inside the dash).
        assert!(parse_date("9/3/2024–10/1/2024").is_none());
        assert!(parse_date("2019年4月15日").is_none());
        // Years outside 1900-2100 are rejected.
        assert!(parse_date("+262143-12-31").is_none());
        assert!(parse_date("0015-03-04").is_none());
        assert!(parse_date("2150-01-01").is_none());
        assert!(require_date("as_of", "+262143-12-31").is_err());
    }
}
