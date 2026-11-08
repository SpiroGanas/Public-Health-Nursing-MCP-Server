//! CDSi ages and intervals ("6 weeks - 4 days", "3 months + 4 weeks") and
//! the date arithmetic rules of the Logic Specification (§3.4, CALCDT-1 to
//! CALCDT-6).

use chrono::{Datelike, Duration, NaiveDate};

/// An age or interval from the supporting data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    /// Signed years.
    pub years: i32,
    /// Signed months.
    pub months: i32,
    /// Signed weeks.
    pub weeks: i32,
    /// Signed days.
    pub days: i32,
}

impl Span {
    /// A span of whole days.
    pub const fn days(days: i32) -> Self {
        Self {
            years: 0,
            months: 0,
            weeks: 0,
            days,
        }
    }

    /// Parse a supporting-data span. Empty text and "n/a" give `None`;
    /// unrecognized text also gives `None` (callers treat it as absent).
    pub fn parse(text: &str) -> Option<Self> {
        let t = text.trim();
        if t.is_empty() || t.eq_ignore_ascii_case("n/a") {
            return None;
        }
        let mut span = Span::default();
        let mut sign = 1;
        let mut tokens = t.split_whitespace().peekable();
        let mut any = false;
        while let Some(tok) = tokens.next() {
            match tok {
                "+" => sign = 1,
                "-" => sign = -1,
                _ => {
                    let n: i32 = tok.parse().ok()?;
                    let unit = tokens.next()?.to_ascii_lowercase();
                    let n = n * sign;
                    match unit.trim_end_matches('s') {
                        "year" => span.years += n,
                        "month" => span.months += n,
                        "week" => span.weeks += n,
                        "day" => span.days += n,
                        _ => return None,
                    }
                    any = true;
                    sign = 1;
                }
            }
        }
        any.then_some(span)
    }

    /// Add this span to a date: years and months first holding the day of
    /// month constant (CALCDT-1, CALCDT-2, CALCDT-6), moving an impossible
    /// date to the first of the next month (CALCDT-5), then days and weeks
    /// (CALCDT-3, CALCDT-4).
    pub fn add_to(&self, date: NaiveDate) -> NaiveDate {
        let total_months = date.year() * 12 + date.month0() as i32 + self.years * 12 + self.months;
        let year = total_months.div_euclid(12);
        let month0 = total_months.rem_euclid(12) as u32;
        let base = match NaiveDate::from_ymd_opt(year, month0 + 1, date.day()) {
            Some(d) => d,
            None => {
                // Not a real date: first day of the following month.
                let (y, m) = if month0 == 11 {
                    (year + 1, 1)
                } else {
                    (year, month0 + 2)
                };
                NaiveDate::from_ymd_opt(y, m, 1).unwrap_or(date)
            }
        };
        base.checked_add_signed(Duration::days(i64::from(self.weeks * 7 + self.days)))
            .unwrap_or(base)
    }

    /// Human-readable text ("4 weeks", "6 months - 4 days").
    pub fn describe(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        let push = |parts: &mut Vec<String>, n: i32, unit: &str| {
            if n == 0 {
                return;
            }
            let abs = n.unsigned_abs();
            let word = if abs == 1 {
                unit.to_string()
            } else {
                format!("{unit}s")
            };
            if parts.is_empty() {
                parts.push(format!("{}{abs} {word}", if n < 0 { "-" } else { "" }));
            } else {
                parts.push(format!("{} {abs} {word}", if n < 0 { "-" } else { "+" }));
            }
        };
        push(&mut parts, self.years, "year");
        push(&mut parts, self.months, "month");
        push(&mut parts, self.weeks, "week");
        push(&mut parts, self.days, "day");
        if parts.is_empty() {
            "0 days".into()
        } else {
            parts.join(" ")
        }
    }
}

/// Earliest representable date used for "assumed value if empty" (01/01/1900).
pub fn min_date() -> NaiveDate {
    NaiveDate::from_ymd_opt(1900, 1, 1).expect("valid date")
}

/// Latest date used for "assumed value if empty" (12/31/2999).
pub fn max_date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2999, 12, 31).expect("valid date")
}

/// Parse a supporting-data date: `YYYYMMDD` or `MM/DD/YYYY`.
pub fn parse_date(text: &str) -> Option<NaiveDate> {
    let t = text.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("n/a") {
        return None;
    }
    NaiveDate::parse_from_str(t, "%Y%m%d")
        .or_else(|_| NaiveDate::parse_from_str(t, "%m/%d/%Y"))
        .or_else(|_| NaiveDate::parse_from_str(t, "%Y-%m-%d"))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%m/%d/%Y").unwrap()
    }

    #[test]
    fn parses_supporting_data_spans() {
        assert_eq!(
            Span::parse("6 weeks - 4 days"),
            Some(Span {
                years: 0,
                months: 0,
                weeks: 6,
                days: -4
            })
        );
        assert_eq!(
            Span::parse("3 months + 4 weeks"),
            Some(Span {
                years: 0,
                months: 3,
                weeks: 4,
                days: 0
            })
        );
        assert_eq!(Span::parse("1 Year").unwrap().years, 1);
        assert_eq!(Span::parse("16 years - 4 months").unwrap().months, -4);
        assert_eq!(Span::parse("0 days - 4 days").unwrap().days, -4);
        assert_eq!(Span::parse(""), None);
        assert_eq!(Span::parse("n/a"), None);
        assert_eq!(Span::parse("soon"), None);
    }

    /// The examples in Table 3-6 of the Logic Specification.
    #[test]
    fn follows_the_logic_specification_date_rules() {
        let add = |date: &str, span: &str| Span::parse(span).unwrap().add_to(d(date));
        assert_eq!(add("01/01/2000", "3 years"), d("01/01/2003")); // CALCDT-1
        assert_eq!(add("01/01/2000", "6 months"), d("07/01/2000")); // CALCDT-2
        assert_eq!(add("11/01/2000", "6 months"), d("05/01/2001"));
        assert_eq!(add("01/01/2000", "3 days"), d("01/04/2000")); // CALCDT-3
        assert_eq!(add("01/01/2000", "3 weeks"), d("01/22/2000"));
        assert_eq!(add("02/01/2000", "5 weeks"), d("03/07/2000"));
        assert_eq!(add("02/01/2001", "5 weeks"), d("03/08/2001"));
        assert_eq!(Span::days(-4).add_to(d("01/15/2000")), d("01/11/2000")); // CALCDT-4
        assert_eq!(add("03/31/2000", "6 months"), d("10/01/2000")); // CALCDT-5
        assert_eq!(add("08/31/2000", "6 months"), d("03/01/2001"));
        assert_eq!(add("01/31/2000", "6 months - 4 days"), d("07/27/2000")); // CALCDT-6
    }

    #[test]
    fn describes_spans() {
        assert_eq!(
            Span::parse("6 weeks - 4 days").unwrap().describe(),
            "6 weeks - 4 days"
        );
        assert_eq!(Span::parse("4 weeks").unwrap().describe(), "4 weeks");
        assert_eq!(
            Span::parse("6 months - 4 days").unwrap().describe(),
            "6 months - 4 days"
        );
        assert_eq!(parse_date("20250701"), NaiveDate::from_ymd_opt(2025, 7, 1));
        assert_eq!(
            parse_date("01/01/1957"),
            NaiveDate::from_ymd_opt(1957, 1, 1)
        );
    }
}
