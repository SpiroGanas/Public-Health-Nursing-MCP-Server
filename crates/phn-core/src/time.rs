//! Dates and times for the whole workspace, built on [`jiff`].
//!
//! The project used `chrono` until it was marked unmaintained (RUSTSEC-2026-0543). This module
//! re-exports the `jiff` types the crates use and adds a few helpers that keep the arithmetic
//! exactly as it was:
//!
//! * day and week amounts are [`SignedDuration`]s of 24-hour days, as `chrono::Duration` was, so
//!   adding them to a civil date or date-time never depends on a time zone;
//! * month arithmetic clamps to the last day of the month (31 January + 1 month = 28 or 29
//!   February), as `chrono::Months` did;
//! * [`parse_date`] and friends take the format first, like `chrono::NaiveDate::parse_from_str`
//!   took it second; both read strftime-style formats.
//!
//! The `*Schema` types give tool arguments that hold a date the same JSON Schema they had with
//! `chrono` (`{"type": "string", "format": "date"}` and so on); use them with
//! `#[schemars(with = "...")]`.

use std::borrow::Cow;

pub use jiff::civil::{Date, DateTime, Time, Weekday};
pub use jiff::{SignedDuration, Span, Timestamp, ZonedDateTime};

/// The server's local date (from `TZ`, or the system time zone).
pub fn today() -> Date {
    ZonedDateTime::now().date()
}

/// The server's local date and time, without a zone.
pub fn now_local() -> DateTime {
    ZonedDateTime::now().datetime()
}

/// A date from a year, month (1–12), and day, or `None` when it does not exist.
pub fn ymd(year: i32, month: u32, day: u32) -> Option<Date> {
    Date::new(
        i16::try_from(year).ok()?,
        i8::try_from(month).ok()?,
        i8::try_from(day).ok()?,
    )
    .ok()
}

/// A time of day, or `None` when it does not exist.
pub fn hms(hour: u32, minute: u32, second: u32) -> Option<Time> {
    Time::new(
        i8::try_from(hour).ok()?,
        i8::try_from(minute).ok()?,
        i8::try_from(second).ok()?,
        0,
    )
    .ok()
}

/// A date at a time of day, or `None` when the time does not exist.
pub fn and_hms(date: Date, hour: u32, minute: u32, second: u32) -> Option<DateTime> {
    hms(hour, minute, second).map(|t| date.to_datetime(t))
}

/// Parse a date with a strftime-style format (`%Y-%m-%d`, `%m/%d/%Y`, ...). The whole input must
/// match. As with `chrono`, `%B` also reads an abbreviated month name ("Mar 7, 2026").
pub fn parse_date(input: &str, format: &str) -> Result<Date, jiff::Error> {
    Date::strptime(format, input).or_else(|e| match abbreviated_months(format) {
        Some(f) => Date::strptime(&f, input).map_err(|_| e),
        None => Err(e),
    })
}

/// Parse a date and time with a strftime-style format. The whole input must match. `%B` also
/// reads an abbreviated month name.
pub fn parse_datetime(input: &str, format: &str) -> Result<DateTime, jiff::Error> {
    DateTime::strptime(format, input).or_else(|e| match abbreviated_months(format) {
        Some(f) => DateTime::strptime(&f, input).map_err(|_| e),
        None => Err(e),
    })
}

/// `format` with `%B` (full month name) read as `%b` (abbreviated), when it has one.
fn abbreviated_months(format: &str) -> Option<String> {
    format.contains("%B").then(|| format.replace("%B", "%b"))
}

/// Parse a time of day with a strftime-style format. The whole input must match.
pub fn parse_time(input: &str, format: &str) -> Result<Time, jiff::Error> {
    Time::strptime(format, input)
}

/// Parse an ISO 8601 date (`2026-11-18`).
pub fn parse_iso_date(input: &str) -> Result<Date, jiff::Error> {
    input.parse()
}

/// A number of 24-hour days.
pub const fn days(n: i64) -> SignedDuration {
    SignedDuration::from_hours(n * 24)
}

/// A number of weeks of 24-hour days.
pub const fn weeks(n: i64) -> SignedDuration {
    SignedDuration::from_hours(n * 24 * 7)
}

/// A number of hours.
pub const fn hours(n: i64) -> SignedDuration {
    SignedDuration::from_hours(n)
}

/// A number of minutes.
pub const fn minutes(n: i64) -> SignedDuration {
    SignedDuration::from_mins(n)
}

/// `date` plus `n` days (negative goes back), or `None` past the supported range.
pub fn add_days(date: Date, n: i64) -> Option<Date> {
    date.checked_add(Span::new().try_days(n).ok()?).ok()
}

/// `date` plus `n` months (negative goes back), clamped to the last day of the month; `None`
/// past the supported range.
pub fn add_months(date: Date, n: i64) -> Option<Date> {
    date.checked_add(Span::new().try_months(n).ok()?).ok()
}

/// Whole days from `earlier` to `later` (negative when `later` is before `earlier`).
pub fn days_between(earlier: Date, later: Date) -> i64 {
    later.duration_since(earlier).num_days()
}

/// Whole minutes from `earlier` to `later` (negative when `later` is before `earlier`).
pub fn minutes_between(earlier: DateTime, later: DateTime) -> i64 {
    later.duration_since(earlier).num_minutes()
}

/// The `chrono::Duration` accessors the code base uses, for [`SignedDuration`]. Each truncates
/// toward zero, as `chrono` did.
pub trait DurationExt {
    /// Whole 24-hour days.
    fn num_days(&self) -> i64;
    /// Whole weeks.
    fn num_weeks(&self) -> i64;
    /// Whole hours.
    fn num_hours(&self) -> i64;
    /// Whole minutes.
    fn num_minutes(&self) -> i64;
    /// Whole seconds.
    fn num_seconds(&self) -> i64;
}

impl DurationExt for SignedDuration {
    fn num_days(&self) -> i64 {
        self.as_hours() / 24
    }
    fn num_weeks(&self) -> i64 {
        self.as_hours() / (24 * 7)
    }
    fn num_hours(&self) -> i64 {
        self.as_hours()
    }
    fn num_minutes(&self) -> i64 {
        self.as_mins()
    }
    fn num_seconds(&self) -> i64 {
        self.as_secs()
    }
}

/// The exact length of a [`Span`] from subtracting two civil dates or date-times (`a - b`),
/// counting a day as 24 hours. Such spans never hold years or months; one that does is a
/// programming error.
fn civil_span_duration(s: &Span) -> SignedDuration {
    debug_assert!(
        s.get_years() == 0 && s.get_months() == 0,
        "calendar span with years or months: {s}"
    );
    let days = i64::from(s.get_weeks()) * 7 + i64::from(s.get_days());
    let secs =
        days * 86_400 + i64::from(s.get_hours()) * 3_600 + s.get_minutes() * 60 + s.get_seconds();
    let nanos =
        s.get_milliseconds() * 1_000_000 + s.get_microseconds() * 1_000 + s.get_nanoseconds();
    SignedDuration::from_secs(secs) + SignedDuration::from_nanos(nanos)
}

impl DurationExt for Span {
    fn num_days(&self) -> i64 {
        civil_span_duration(self).num_days()
    }
    fn num_weeks(&self) -> i64 {
        civil_span_duration(self).num_weeks()
    }
    fn num_hours(&self) -> i64 {
        civil_span_duration(self).num_hours()
    }
    fn num_minutes(&self) -> i64 {
        civil_span_duration(self).num_minutes()
    }
    fn num_seconds(&self) -> i64 {
        civil_span_duration(self).num_seconds()
    }
}

/// Monday-based weekday numbers, as `chrono::Weekday` offered them.
pub trait WeekdayExt {
    /// Monday = 0 ... Sunday = 6.
    fn num_days_from_monday(self) -> u32;
    /// Sunday = 0 ... Saturday = 6.
    fn num_days_from_sunday(self) -> u32;
    /// Monday = 1 ... Sunday = 7.
    fn number_from_monday(self) -> u32;
    /// Three-letter English name (`Mon`), as `chrono` displayed a weekday.
    fn short_name(self) -> &'static str;
}

impl WeekdayExt for Weekday {
    fn num_days_from_monday(self) -> u32 {
        self.to_monday_zero_offset() as u32
    }
    fn num_days_from_sunday(self) -> u32 {
        self.to_sunday_zero_offset() as u32
    }
    fn number_from_monday(self) -> u32 {
        self.to_monday_one_offset() as u32
    }
    fn short_name(self) -> &'static str {
        match self {
            Weekday::Monday => "Mon",
            Weekday::Tuesday => "Tue",
            Weekday::Wednesday => "Wed",
            Weekday::Thursday => "Thu",
            Weekday::Friday => "Fri",
            Weekday::Saturday => "Sat",
            Weekday::Sunday => "Sun",
        }
    }
}

/// A whole number of months, for [`DateExt::checked_add_months`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Months(u32);

impl Months {
    /// `n` months.
    pub const fn new(n: u32) -> Self {
        Self(n)
    }
}

/// A whole number of days, for [`DateExt::checked_add_days`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Days(u64);

impl Days {
    /// `n` days.
    pub const fn new(n: u64) -> Self {
        Self(n)
    }
}

/// An ISO 8601 week (year and week number 1–53).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IsoWeek {
    year: i32,
    week: u32,
}

impl IsoWeek {
    /// The ISO week-numbering year.
    pub fn year(&self) -> i32 {
        self.year
    }
    /// The week number, 1 to 53.
    pub fn week(&self) -> u32 {
        self.week
    }
}

/// Date operations under the names the code base used with `chrono`. All are checked: `None`
/// means the result would be past the supported range (years -9999 to 9999) or does not exist.
pub trait DateExt: Sized {
    /// Day of the year, 1 to 366.
    fn ordinal(&self) -> u32;
    /// Month, 0 to 11.
    fn month0(&self) -> u32;
    /// The same date on another day of the month.
    fn with_day(&self, day: u32) -> Option<Self>;
    /// The same month and day in another year.
    fn with_year(&self, year: i32) -> Option<Self>;
    /// Plus whole months, clamped to the end of the month.
    fn checked_add_months(&self, months: Months) -> Option<Self>;
    /// Minus whole months, clamped to the end of the month.
    fn checked_sub_months(&self, months: Months) -> Option<Self>;
    /// Plus whole days.
    fn checked_add_days(&self, days: Days) -> Option<Self>;
    /// Minus whole days.
    fn checked_sub_days(&self, days: Days) -> Option<Self>;
    /// Plus a duration (whole days count; any remainder is dropped, as in `chrono`).
    fn checked_add_signed(&self, d: SignedDuration) -> Option<Self>;
    /// The next day.
    fn succ_opt(&self) -> Option<Self>;
    /// The previous day.
    fn pred_opt(&self) -> Option<Self>;
    /// This date at a time of day.
    fn and_hms_opt(&self, hour: u32, minute: u32, second: u32) -> Option<DateTime>;
    /// This date at a time of day.
    fn and_time(&self, t: Time) -> DateTime;
    /// The exact time from `other` to `self`.
    fn signed_duration_since(&self, other: Self) -> SignedDuration;
    /// The ISO week.
    fn iso_week(&self) -> IsoWeek;
    /// Format with a strftime-style format.
    fn format<'f>(&self, fmt: &'f str) -> impl core::fmt::Display + 'f;
}

impl DateExt for Date {
    fn ordinal(&self) -> u32 {
        self.day_of_year() as u32
    }
    fn month0(&self) -> u32 {
        self.month() as u32 - 1
    }
    fn with_day(&self, day: u32) -> Option<Self> {
        ymd(i32::from(self.year()), self.month() as u32, day)
    }
    fn with_year(&self, year: i32) -> Option<Self> {
        ymd(year, self.month() as u32, self.day() as u32)
    }
    fn checked_add_months(&self, months: Months) -> Option<Self> {
        add_months(*self, i64::from(months.0))
    }
    fn checked_sub_months(&self, months: Months) -> Option<Self> {
        add_months(*self, -i64::from(months.0))
    }
    fn checked_add_days(&self, days: Days) -> Option<Self> {
        add_days(*self, i64::try_from(days.0).ok()?)
    }
    fn checked_sub_days(&self, days: Days) -> Option<Self> {
        add_days(*self, -i64::try_from(days.0).ok()?)
    }
    fn checked_add_signed(&self, d: SignedDuration) -> Option<Self> {
        add_days(*self, d.num_days())
    }
    fn succ_opt(&self) -> Option<Self> {
        self.tomorrow().ok()
    }
    fn pred_opt(&self) -> Option<Self> {
        self.yesterday().ok()
    }
    fn and_hms_opt(&self, hour: u32, minute: u32, second: u32) -> Option<DateTime> {
        and_hms(*self, hour, minute, second)
    }
    fn and_time(&self, t: Time) -> DateTime {
        self.to_datetime(t)
    }
    fn signed_duration_since(&self, other: Self) -> SignedDuration {
        self.duration_since(other)
    }
    fn iso_week(&self) -> IsoWeek {
        let w = self.iso_week_date();
        IsoWeek {
            year: i32::from(w.year()),
            week: w.week() as u32,
        }
    }
    fn format<'f>(&self, fmt: &'f str) -> impl core::fmt::Display + 'f {
        self.strftime(fmt)
    }
}

/// Date-time operations under the names the code base used with `chrono`.
pub trait DateTimeExt: Sized {
    /// The exact time from `other` to `self`.
    fn signed_duration_since(&self, other: Self) -> SignedDuration;
    /// Plus a duration; `None` past the supported range.
    fn checked_add_signed(&self, d: SignedDuration) -> Option<Self>;
    /// This wall-clock time read as UTC.
    fn and_utc(&self) -> Timestamp;
    /// Format with a strftime-style format.
    fn format<'f>(&self, fmt: &'f str) -> impl core::fmt::Display + 'f;
}

impl DateTimeExt for DateTime {
    fn signed_duration_since(&self, other: Self) -> SignedDuration {
        self.duration_since(other)
    }
    fn checked_add_signed(&self, d: SignedDuration) -> Option<Self> {
        self.checked_add(d).ok()
    }
    fn and_utc(&self) -> Timestamp {
        self.to_zoned_datetime(jiff::tz::TimeZone::UTC)
            .expect("every civil date-time exists in UTC")
            .timestamp()
    }
    fn format<'f>(&self, fmt: &'f str) -> impl core::fmt::Display + 'f {
        self.strftime(fmt)
    }
}

/// Time-of-day formatting under the name the code base used with `chrono`.
pub trait TimeExt {
    /// Format with a strftime-style format.
    fn format<'f>(&self, fmt: &'f str) -> impl core::fmt::Display + 'f;
}

impl TimeExt for Time {
    fn format<'f>(&self, fmt: &'f str) -> impl core::fmt::Display + 'f {
        self.strftime(fmt)
    }
}

/// UTC instant operations under the names the code base used with `chrono::DateTime<Utc>`.
pub trait TimestampExt {
    /// RFC 3339 exactly as `chrono` wrote it: `+00:00` (not `Z`) and 0, 3, 6, or 9 fractional
    /// digits. The HTTP cache stores and compares these strings, so the form must not change.
    fn to_rfc3339(&self) -> String;
    /// RFC 3339 to the whole second with `Z` (`2026-11-18T14:30:00Z`).
    fn to_rfc3339_secs(&self) -> String;
    /// The UTC calendar date.
    fn date_naive(&self) -> Date;
    /// The exact time from `other` to `self`.
    fn signed_duration_since(&self, other: Timestamp) -> SignedDuration;
    /// The calendar date in the server's local time zone.
    fn local_date(&self) -> Date;
    /// Format in UTC with a strftime-style format.
    fn format<'f>(&self, fmt: &'f str) -> impl core::fmt::Display + 'f;
}

impl TimestampExt for Timestamp {
    fn to_rfc3339(&self) -> String {
        let base = self.strftime("%Y-%m-%dT%H:%M:%S");
        let ns = self.subsec_nanosecond();
        if ns == 0 {
            format!("{base}+00:00")
        } else if ns % 1_000_000 == 0 {
            format!("{base}.{:03}+00:00", ns / 1_000_000)
        } else if ns % 1_000 == 0 {
            format!("{base}.{:06}+00:00", ns / 1_000)
        } else {
            format!("{base}.{ns:09}+00:00")
        }
    }
    fn to_rfc3339_secs(&self) -> String {
        self.strftime("%Y-%m-%dT%H:%M:%SZ").to_string()
    }
    fn date_naive(&self) -> Date {
        self.to_zoned_datetime(jiff::tz::TimeZone::UTC).date()
    }
    fn signed_duration_since(&self, other: Timestamp) -> SignedDuration {
        self.duration_since(other)
    }
    fn local_date(&self) -> Date {
        self.to_zoned_datetime(jiff::tz::TimeZone::system()).date()
    }
    fn format<'f>(&self, fmt: &'f str) -> impl core::fmt::Display + 'f {
        self.strftime(fmt)
    }
}

/// Parse an RFC 3339 instant with any offset (`2026-07-15T18:00:00-04:00`, `...Z`).
pub fn parse_rfc3339(input: &str) -> Result<Timestamp, jiff::Error> {
    input.parse()
}

/// Parse an RFC 2822 instant (`Tue, 18 Nov 2026 14:30:00 -0500`), as in RSS feeds.
pub fn parse_rfc2822(input: &str) -> Result<Timestamp, jiff::Error> {
    jiff::fmt::rfc2822::parse(input).map(|z| z.timestamp())
}

macro_rules! string_schema {
    ($name:ident, $format:literal, $doc:literal) => {
        #[doc = $doc]
        pub struct $name;

        impl schemars::JsonSchema for $name {
            fn inline_schema() -> bool {
                true
            }
            fn schema_name() -> Cow<'static, str> {
                Cow::Borrowed($format)
            }
            fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
                schemars::json_schema!({
                    "type": "string",
                    "format": $format
                })
            }
        }
    };
}

string_schema!(
    DateSchema,
    "date",
    "JSON Schema for a [`Date`] (`2026-11-18`)."
);
string_schema!(
    DateTimeSchema,
    "partial-date-time",
    "JSON Schema for a [`DateTime`] (`2026-11-18T09:30:00`)."
);
string_schema!(
    TimeSchema,
    "partial-time",
    "JSON Schema for a [`Time`] (`09:30:00`)."
);
string_schema!(
    TimestampSchema,
    "date-time",
    "JSON Schema for a [`Timestamp`] (`2026-11-18T14:30:00Z`)."
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_arithmetic_clamps_like_chrono() {
        let jan31 = ymd(2025, 1, 31).unwrap();
        assert_eq!(add_months(jan31, 1), ymd(2025, 2, 28));
        assert_eq!(add_months(ymd(2024, 1, 31).unwrap(), 1), ymd(2024, 2, 29));
        assert_eq!(add_months(ymd(2024, 3, 31).unwrap(), -1), ymd(2024, 2, 29));
    }

    #[test]
    fn day_arithmetic_and_differences() {
        let d = ymd(2026, 3, 7).unwrap();
        assert_eq!(d + days(2), ymd(2026, 3, 9).unwrap());
        assert_eq!(d - weeks(1), ymd(2026, 2, 28).unwrap());
        assert_eq!(days_between(d, ymd(2026, 3, 9).unwrap()), 2);
        assert_eq!(days_between(ymd(2026, 3, 9).unwrap(), d), -2);
        assert_eq!((ymd(2026, 3, 9).unwrap().duration_since(d)).num_days(), 2);
        let a = and_hms(d, 23, 30, 0).unwrap();
        assert_eq!(
            a + hours(1),
            and_hms(ymd(2026, 3, 8).unwrap(), 0, 30, 0).unwrap()
        );
        assert_eq!(minutes_between(a, a + minutes(95)), 95);
        // `a - b` on dates and date-times gives a span; its length matches chrono's.
        assert_eq!((ymd(2026, 3, 9).unwrap() - d).num_days(), 2);
        assert_eq!((d - ymd(2026, 3, 9).unwrap()).num_days(), -2);
        assert_eq!(((a + minutes(1_501)) - a).num_minutes(), 1_501);
        assert_eq!(((a + minutes(1_501)) - a).num_days(), 1);
        assert_eq!((a - (a + hours(30))).num_days(), -1);
        // Truncation toward zero, as chrono's num_days did.
        assert_eq!(hours(-30).num_days(), -1);
    }

    #[test]
    fn parsing_accepts_what_the_formats_say() {
        assert_eq!(parse_date("2026-11-18", "%Y-%m-%d").ok(), ymd(2026, 11, 18));
        assert_eq!(parse_date("11/18/2026", "%m/%d/%Y").ok(), ymd(2026, 11, 18));
        // Unpadded month and day, which chrono also read.
        assert_eq!(parse_date("1/5/2026", "%m/%d/%Y").ok(), ymd(2026, 1, 5));
        assert_eq!(parse_date("2026-1-5", "%Y-%m-%d").ok(), ymd(2026, 1, 5));
        assert_eq!(parse_date("2026-11-18x", "%Y-%m-%d").ok(), None);
        assert_eq!(parse_date("2026-02-30", "%Y-%m-%d").ok(), None);
        assert_eq!(parse_date("1 March 2026", "%d %B %Y").ok(), ymd(2026, 3, 1));
        // chrono's %B also read abbreviations (%b reads only abbreviations in both).
        assert_eq!(parse_date("7 Mar 2026", "%d %B %Y").ok(), ymd(2026, 3, 7));
        assert_eq!(parse_date("Sep 7, 2026", "%B %d, %Y").ok(), ymd(2026, 9, 7));
        assert_eq!(
            parse_date("Saturday Mar 7 2026", "%A %B %d %Y").ok(),
            ymd(2026, 3, 7)
        );
        assert_eq!(parse_date("7 March 2026", "%d %b %Y").ok(), None);
        assert_eq!(parse_date("7 Sept 2026", "%d %B %Y").ok(), None);
        // Two-digit years: 69-99 are 1900s, 00-68 are 2000s (POSIX).
        assert_eq!(parse_date("3/7/68", "%m/%d/%y").ok(), ymd(2068, 3, 7));
        assert_eq!(parse_date("3/7/69", "%m/%d/%y").ok(), ymd(1969, 3, 7));
        assert_eq!(parse_iso_date("2026-11-18").ok(), ymd(2026, 11, 18));
        assert_eq!(
            parse_datetime("2026-11-18 09:05", "%Y-%m-%d %H:%M").ok(),
            and_hms(ymd(2026, 11, 18).unwrap(), 9, 5, 0)
        );
        assert_eq!(parse_time("9:05", "%H:%M").ok(), hms(9, 5, 0));
    }

    #[test]
    fn display_and_formats_match_what_chrono_printed() {
        let d = ymd(2026, 3, 7).unwrap();
        assert_eq!(d.to_string(), "2026-03-07");
        assert_eq!(d.strftime("%B %-d, %Y").to_string(), "March 7, 2026");
        assert_eq!(d.strftime("%a %Y-%m-%d").to_string(), "Sat 2026-03-07");
        let t = and_hms(d, 14, 5, 0).unwrap();
        assert_eq!(t.strftime("%-I:%M %p").to_string(), "2:05 PM");
        assert_eq!(serde_json::to_string(&d).unwrap(), "\"2026-03-07\"");
        assert_eq!(
            serde_json::to_string(&t).unwrap(),
            "\"2026-03-07T14:05:00\""
        );
        assert_eq!(d.weekday().short_name(), "Sat");
        assert_eq!(d.weekday().num_days_from_monday(), 5);
    }

    #[test]
    fn rfc3339_text_matches_chrono() {
        let t = parse_rfc3339("2026-07-15T18:00:00-04:00").unwrap();
        assert_eq!(t.to_rfc3339(), "2026-07-15T22:00:00+00:00");
        assert_eq!(t.to_rfc3339_secs(), "2026-07-15T22:00:00Z");
        let ms = Timestamp::from_millisecond(1_784_152_800_250).unwrap();
        assert_eq!(ms.to_rfc3339(), "2026-07-15T22:00:00.250+00:00");
        let ns = Timestamp::new(1_784_152_800, 123_456_789).unwrap();
        assert_eq!(ns.to_rfc3339(), "2026-07-15T22:00:00.123456789+00:00");
        assert_eq!(parse_rfc3339(&ns.to_rfc3339()).unwrap(), ns);
        assert_eq!(t.date_naive(), ymd(2026, 7, 15).unwrap());
        assert_eq!(parse_rfc2822("Wed, 15 Jul 2026 18:00:00 -0400").unwrap(), t);
    }

    #[test]
    fn chrono_named_date_methods() {
        let d = ymd(2025, 1, 31).unwrap();
        assert_eq!(d.checked_add_months(Months::new(1)), ymd(2025, 2, 28));
        assert_eq!(d.checked_sub_months(Months::new(2)), ymd(2024, 11, 30));
        assert_eq!(d.checked_add_days(Days::new(1)), ymd(2025, 2, 1));
        assert_eq!(d.succ_opt(), ymd(2025, 2, 1));
        assert_eq!(d.pred_opt(), ymd(2025, 1, 30));
        assert_eq!(d.ordinal(), 31);
        assert_eq!(d.month0(), 0);
        assert_eq!(d.with_day(1), ymd(2025, 1, 1));
        assert_eq!(ymd(2024, 2, 29).unwrap().with_year(2025), None);
        assert_eq!(ymd(2026, 12, 28).unwrap().iso_week().week(), 53);
        assert_eq!(d.format("%m/%d/%Y").to_string(), "01/31/2025");
    }

    #[test]
    fn schemas_match_chrono_ones() {
        let s = schemars::schema_for!(Option<DateSchema>);
        let v = serde_json::to_value(&s).unwrap();
        assert!(v.to_string().contains("\"format\":\"date\""), "{v}");
    }
}
