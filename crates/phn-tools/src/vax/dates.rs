//! Calendar arithmetic for immunization rules: ages and intervals in days,
//! weeks, calendar months, and years, and the CDC 4-day grace period.
//!
//! Conventions follow CDC's General Best Practice Guidelines (Table 3-2):
//! weeks are 7 days ("4 weeks = 28 days"); months and years are calendar
//! months and years, so "12 months" after 2025-01-31 is 2026-01-31 and one
//! month after January 31 is the last day of February. A dose given up to 4
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

/// `date + span`: calendar years and months first (clamped to the month's
/// last day), then weeks and days.
pub fn add(date: NaiveDate, span: Span) -> NaiveDate {
    let months = span.years * 12 + span.months;
    let d = date
        .checked_add_months(Months::new(months))
        .unwrap_or(NaiveDate::MAX);
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

/// Parse a date typed by a person or found in a file: ISO (2024-03-05),
/// US (3/5/2024 or 03-05-2024), or an Excel serial number.
pub fn parse_date(s: &str) -> Option<NaiveDate> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    for fmt in ["%Y-%m-%d", "%m/%d/%Y", "%m-%d-%Y", "%Y/%m/%d", "%m/%d/%y"] {
        if let Ok(d) = NaiveDate::parse_from_str(t, fmt) {
            return Some(d);
        }
    }
    // ISO date-time ("2024-03-05T00:00:00" or "2024-03-05 00:00:00").
    if t.len() > 10
        && let Ok(d) = NaiveDate::parse_from_str(&t[..10], "%Y-%m-%d")
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
        .ok();
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
        // Calendar months clamp to the last day of the month.
        assert_eq!(add(d("2025-01-31"), Span::months(1)), d("2025-02-28"));
        assert_eq!(add(d("2024-02-29"), Span::years(1)), d("2025-02-28"));
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
}
