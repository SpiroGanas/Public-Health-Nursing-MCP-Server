//! Open Meeting Law notice calculator: the latest time a meeting notice may be posted so that
//! at least 48 hours pass before the meeting, excluding Saturdays, Sundays, and legal holidays
//! (M.G.L. c. 30A, s. 20(b); 940 CMR 29.03(1)(a)).
//!
//! Method (this project's reading; the Attorney General gives no worked example): a Saturday,
//! Sunday, or legal holiday is taken out of the count as a whole calendar day. Hours are counted
//! back from the meeting through the remaining days until 48 have been counted, so a Monday 6 pm
//! meeting needs posting by Thursday 6 pm. The legal holidays are the bundled
//! `ma_legal_holidays.yaml` list (2026-2028); Evacuation Day and Bunker Hill Day count only for
//! Suffolk county bodies.

use std::collections::BTreeMap;

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Weekday};
use phn_core::{PhnError, Result};

use super::workforce_content::{HolidayContent, HolidayRow};

/// First and last dates the bundled holiday list covers.
pub const FIRST_DAY: (i32, u32, u32) = (2026, 1, 1);
pub const LAST_DAY: (i32, u32, u32) = (2028, 12, 31);

const REQUIRED_MINUTES: i64 = 48 * 60;

/// Why a day is left out of the 48-hour count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Excluded {
    pub date: NaiveDate,
    pub reason: String,
}

/// The legal holiday calendar for one body (statewide, or with the Suffolk-only days).
#[derive(Debug, Clone)]
pub struct Calendar {
    holidays: BTreeMap<NaiveDate, String>,
}

fn d(t: (i32, u32, u32)) -> NaiveDate {
    NaiveDate::from_ymd_opt(t.0, t.1, t.2).expect("constant date")
}

pub fn first_day() -> NaiveDate {
    d(FIRST_DAY)
}

pub fn last_day() -> NaiveDate {
    d(LAST_DAY)
}

pub fn weekday_name(date: NaiveDate) -> &'static str {
    match date.weekday() {
        Weekday::Mon => "Monday",
        Weekday::Tue => "Tuesday",
        Weekday::Wed => "Wednesday",
        Weekday::Thu => "Thursday",
        Weekday::Fri => "Friday",
        Weekday::Sat => "Saturday",
        Weekday::Sun => "Sunday",
    }
}

impl Calendar {
    /// Build from the bundled list; `suffolk` adds the Suffolk-county-only holidays.
    pub fn load(suffolk: bool) -> Result<Self> {
        let c = HolidayContent::load()?;
        Self::from_rows(&c.holidays, suffolk)
    }

    pub fn from_rows(rows: &[HolidayRow], suffolk: bool) -> Result<Self> {
        let mut holidays = BTreeMap::new();
        for r in rows {
            if r.scope == "suffolk" && !suffolk {
                continue;
            }
            let date = NaiveDate::parse_from_str(&r.date, "%Y-%m-%d")
                .map_err(|e| PhnError::Internal(format!("holiday date {}: {e}", r.date)))?;
            holidays.insert(date, r.name.clone());
        }
        Ok(Self { holidays })
    }

    /// The holiday on a date, if any.
    pub fn holiday(&self, date: NaiveDate) -> Option<&str> {
        self.holidays.get(&date).map(String::as_str)
    }

    /// Why the day is excluded, or `None` for a counted day.
    pub fn exclusion(&self, date: NaiveDate) -> Option<String> {
        match date.weekday() {
            Weekday::Sat => Some("Saturday".into()),
            Weekday::Sun => Some("Sunday".into()),
            _ => self.holiday(date).map(|h| format!("legal holiday: {h}")),
        }
    }

    fn check_range(&self, date: NaiveDate) -> Result<()> {
        if date < first_day() || date > last_day() {
            return Err(PhnError::InvalidInput(format!(
                "the bundled legal holiday list covers 2026-01-01 to 2028-12-31, so a notice \
                 calculation that reaches {date} cannot be made; check the holidays yourself \
                 (M.G.L. c. 4, s. 7, clause Eighteenth)"
            )));
        }
        Ok(())
    }

    /// The latest time the notice can be posted (counted from the meeting back over 48
    /// countable hours), and the excluded days passed on the way.
    pub fn posting_deadline(
        &self,
        meeting: NaiveDateTime,
    ) -> Result<(NaiveDateTime, Vec<Excluded>)> {
        self.check_range(meeting.date())?;
        let mut remaining = REQUIRED_MINUTES;
        let mut date = meeting.date();
        let mut end = minutes_of(meeting.time());
        let mut skipped = Vec::new();
        loop {
            self.check_range(date)?;
            let avail = match self.exclusion(date) {
                Some(reason) => {
                    skipped.push(Excluded { date, reason });
                    0
                }
                None => end,
            };
            if avail >= remaining {
                let at = end - remaining;
                let t = NaiveTime::from_hms_opt((at / 60) as u32, (at % 60) as u32, 0)
                    .expect("minute of day");
                return Ok((date.and_time(t), skipped));
            }
            remaining -= avail;
            date -= Duration::days(1);
            end = 1440;
        }
    }

    /// Countable minutes (not Saturday, Sunday, or a legal holiday) from `posted` to `meeting`.
    pub fn countable_minutes(&self, posted: NaiveDateTime, meeting: NaiveDateTime) -> Result<i64> {
        self.check_range(posted.date())?;
        self.check_range(meeting.date())?;
        if posted >= meeting {
            return Ok(0);
        }
        let mut total = 0;
        let mut day = posted.date();
        while day <= meeting.date() {
            if self.exclusion(day).is_none() {
                let from = if day == posted.date() {
                    minutes_of(posted.time())
                } else {
                    0
                };
                let to = if day == meeting.date() {
                    minutes_of(meeting.time())
                } else {
                    1440
                };
                total += (to - from).max(0);
            }
            day += Duration::days(1);
        }
        Ok(total)
    }
}

fn minutes_of(t: NaiveTime) -> i64 {
    i64::from(t.hour() * 60 + t.minute())
}

/// Parse "YYYY-MM-DD" (1900 to 2100).
pub fn parse_date(what: &str, s: &str) -> Result<NaiveDate> {
    let d = NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").map_err(|_| {
        PhnError::InvalidInput(format!(
            "{what} must be a date written YYYY-MM-DD, such as 2026-12-08"
        ))
    })?;
    if !(1900..=2100).contains(&d.year()) {
        return Err(PhnError::InvalidInput(format!(
            "{what} must be between 1900 and 2100"
        )));
    }
    Ok(d)
}

/// Parse a time written 18:00, 6:00 pm, or 6 pm.
pub fn parse_time(what: &str, s: &str) -> Result<NaiveTime> {
    let mut t = s.trim().to_ascii_lowercase().replace('.', "");
    // "7 pm" has no minutes: chrono needs them.
    if !t.contains(':')
        && let Some(pos) = t.find(|c: char| !c.is_ascii_digit())
        && pos > 0
    {
        t.insert_str(pos, ":00");
    }
    let formats = ["%H:%M", "%I:%M %p", "%I:%M%p", "%I %p", "%I%p"];
    for f in formats {
        if let Ok(v) = NaiveTime::parse_from_str(&t, f) {
            return Ok(v);
        }
    }
    Err(PhnError::InvalidInput(format!(
        "{what} must be a time such as 18:00 or 6:00 pm"
    )))
}

/// Parse "YYYY-MM-DD HH:MM" (or with a T).
pub fn parse_datetime(what: &str, s: &str) -> Result<NaiveDateTime> {
    let t = s.trim().replace('T', " ");
    let (date, time) = t.split_once(' ').ok_or_else(|| {
        PhnError::InvalidInput(format!(
            "{what} must be a date and time written like 2026-12-04 17:30"
        ))
    })?;
    Ok(parse_date(what, date)?.and_time(parse_time(what, time)?))
}

/// "Thursday, November 12, 2026 at 6:00 PM".
pub fn show(dt: NaiveDateTime) -> String {
    format!(
        "{}, {} at {}",
        weekday_name(dt.date()),
        dt.date().format("%B %-d, %Y"),
        dt.time().format("%-I:%M %p")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
    }

    fn deadline(cal: &Calendar, meeting: &str) -> String {
        cal.posting_deadline(dt(meeting))
            .unwrap()
            .0
            .format("%Y-%m-%d %H:%M")
            .to_string()
    }

    // Each golden below was worked by hand from the statute's rule: count countable hours back
    // from the meeting, skipping Saturdays, Sundays, and legal holidays as whole days.

    #[test]
    fn monday_evening_needs_thursday_evening() {
        // Mon 2026-11-16 18:00: Monday 18 h + Friday 24 h = 42 h; 6 h more is Thursday 18:00.
        let cal = Calendar::load(false).unwrap();
        assert_eq!(deadline(&cal, "2026-11-16 18:00"), "2026-11-12 18:00");
    }

    #[test]
    fn tuesday_evening_needs_friday_evening() {
        // Tue 18 h + Mon 24 h = 42 h; Sunday and Saturday skipped; 6 h more is Friday 18:00.
        let cal = Calendar::load(false).unwrap();
        assert_eq!(deadline(&cal, "2026-11-17 18:00"), "2026-11-13 18:00");
    }

    #[test]
    fn midweek_meeting_needs_two_days_back() {
        // Wed 2026-11-18 09:00: no excluded day in the way, so Monday 09:00.
        let cal = Calendar::load(false).unwrap();
        assert_eq!(deadline(&cal, "2026-11-18 09:00"), "2026-11-16 09:00");
    }

    #[test]
    fn veterans_day_is_skipped() {
        // Fri 2026-11-13 10:00: Fri 10 h + Thu 24 h = 34 h; Wed Nov 11 (Veterans Day) skipped;
        // 14 h more from Tuesday's end is Tuesday 10:00.
        let cal = Calendar::load(false).unwrap();
        assert_eq!(deadline(&cal, "2026-11-13 10:00"), "2026-11-10 10:00");
    }

    #[test]
    fn thanksgiving_weekend() {
        // Mon 2026-11-30 09:00: 9 h + Fri Nov 27 24 h = 33 h; Sat, Sun skipped; Thu Nov 26
        // (Thanksgiving) skipped; 15 h more from Wednesday's end is Wednesday 09:00.
        let cal = Calendar::load(false).unwrap();
        assert_eq!(deadline(&cal, "2026-11-30 09:00"), "2026-11-25 09:00");
    }

    #[test]
    fn christmas_friday_2026() {
        // Mon 2026-12-28 12:00: 12 h; Sun, Sat, Fri Dec 25 (Christmas) skipped; Thu 24 h = 36 h;
        // 12 h more from Wednesday's end is Wednesday 12:00.
        let cal = Calendar::load(false).unwrap();
        assert_eq!(deadline(&cal, "2026-12-28 12:00"), "2026-12-23 12:00");
    }

    #[test]
    fn independence_day_on_sunday_moves_to_monday_2027() {
        // Tue 2027-07-06 17:00: 17 h; Mon Jul 5 (observed), Sun Jul 4, Sat Jul 3 skipped;
        // Fri Jul 2 supplies 24 h (41 h in all); the last 7 h come from the end of Thursday
        // Jul 1, which is Thursday 17:00.
        let cal = Calendar::load(false).unwrap();
        assert_eq!(deadline(&cal, "2027-07-06 17:00"), "2027-07-01 17:00");
    }

    #[test]
    fn juneteenth_on_monday_2028() {
        // Tue 2028-06-20 08:00: 8 h; Mon Jun 19 (Juneteenth), Sun, Sat skipped; Fri 24 h = 32 h;
        // 16 h more from Thursday's end is Thursday 08:00.
        let cal = Calendar::load(false).unwrap();
        assert_eq!(deadline(&cal, "2028-06-20 08:00"), "2028-06-15 08:00");
    }

    #[test]
    fn saturday_holidays_change_nothing() {
        // Veterans Day 2028 is a Saturday and is not moved; Mon 2028-11-13 18:00 needs Thursday.
        let cal = Calendar::load(false).unwrap();
        assert_eq!(deadline(&cal, "2028-11-13 18:00"), "2028-11-09 18:00");
    }

    #[test]
    fn suffolk_county_counts_evacuation_day() {
        // Wed 2026-03-18 12:00. Statewide: 12 h + Tue Mar 17 24 h = 36 h; Monday 12:00.
        // Suffolk: Mar 17 is skipped; 12 h + Monday 24 h = 36 h; Sun, Sat skipped; Friday 12:00.
        let statewide = Calendar::load(false).unwrap();
        let suffolk = Calendar::load(true).unwrap();
        assert_eq!(deadline(&statewide, "2026-03-18 12:00"), "2026-03-16 12:00");
        assert_eq!(deadline(&suffolk, "2026-03-18 12:00"), "2026-03-13 12:00");
    }

    #[test]
    fn a_sunday_meeting_counts_back_from_the_previous_friday() {
        // Sun 2026-11-15: Sunday and Saturday skipped; Friday 24 h and Thursday 24 h give 48 h
        // exactly, so Thursday 00:00.
        let cal = Calendar::load(false).unwrap();
        assert_eq!(deadline(&cal, "2026-11-15 10:00"), "2026-11-12 00:00");
    }

    #[test]
    fn exactly_48_countable_hours_is_enough_and_one_minute_less_is_not() {
        let cal = Calendar::load(false).unwrap();
        let meeting = dt("2026-11-16 18:00");
        assert_eq!(
            cal.countable_minutes(dt("2026-11-12 18:00"), meeting)
                .unwrap(),
            2880
        );
        assert_eq!(
            cal.countable_minutes(dt("2026-11-12 18:01"), meeting)
                .unwrap(),
            2879
        );
        // Posted on Friday evening for a Monday meeting is only 24 countable hours.
        assert_eq!(
            cal.countable_minutes(dt("2026-11-13 18:00"), meeting)
                .unwrap(),
            1440
        );
    }

    #[test]
    fn deadline_and_countable_minutes_agree_over_a_year() {
        let cal = Calendar::load(true).unwrap();
        let mut day = NaiveDate::from_ymd_opt(2027, 1, 5).unwrap();
        while day < NaiveDate::from_ymd_opt(2027, 12, 30).unwrap() {
            for t in [(0, 0), (6, 30), (12, 0), (23, 59)] {
                let m = day.and_hms_opt(t.0, t.1, 0).unwrap();
                let (dl, _) = cal.posting_deadline(m).unwrap();
                assert_eq!(cal.countable_minutes(dl, m).unwrap(), 2880, "{m}");
                // One minute later is not enough.
                let later = dl + Duration::minutes(1);
                assert_eq!(cal.countable_minutes(later, m).unwrap(), 2879, "{m}");
            }
            day += Duration::days(1);
        }
    }

    #[test]
    fn dates_outside_the_list_are_refused() {
        let cal = Calendar::load(false).unwrap();
        assert!(cal.posting_deadline(dt("2029-01-02 10:00")).is_err());
        // Needs 2025 holidays to count back from the first days of January 2026.
        assert!(cal.posting_deadline(dt("2026-01-02 10:00")).is_err());
        assert!(cal.posting_deadline(dt("2026-01-06 10:00")).is_ok());
    }

    #[test]
    fn rule_reproduces_the_bundled_list_and_the_trial_court_dates() {
        // The bundled list was generated by xtask/scripts/build_workforce_holidays.py (Python's
        // calendar module); here the rule is applied again in Rust and compared.
        let c = HolidayContent::load().unwrap();
        for r in &c.holidays {
            let date = NaiveDate::parse_from_str(&r.date, "%Y-%m-%d").unwrap();
            assert_eq!(weekday_name(date), r.weekday, "{}", r.date);
        }
        let mut want: Vec<(NaiveDate, String)> = Vec::new();
        for y in 2026..=2028 {
            let nth = |m: u32, wd: Weekday, n: u32| {
                let mut count = 0;
                for day in 1..=31 {
                    if let Some(x) = NaiveDate::from_ymd_opt(y, m, day)
                        && x.weekday() == wd
                    {
                        count += 1;
                        if count == n {
                            return x;
                        }
                    }
                }
                unreachable!()
            };
            let last_monday_may = (1..=31)
                .rev()
                .filter_map(|day| NaiveDate::from_ymd_opt(y, 5, day))
                .find(|x| x.weekday() == Weekday::Mon)
                .unwrap();
            let sunday_rule = |m: u32, day: u32| {
                let x = NaiveDate::from_ymd_opt(y, m, day).unwrap();
                if x.weekday() == Weekday::Sun {
                    x + Duration::days(1)
                } else {
                    x
                }
            };
            for (m, day, name) in [
                (1, 1, "New Year's Day"),
                (6, 19, "Juneteenth"),
                (7, 4, "Independence Day"),
                (11, 11, "Veterans Day"),
                (12, 25, "Christmas Day"),
                (3, 17, "Evacuation Day"),
                (6, 17, "Bunker Hill Day"),
            ] {
                want.push((sunday_rule(m, day), name.to_string()));
            }
            want.push((nth(1, Weekday::Mon, 3), "MLK Day".into()));
            want.push((nth(2, Weekday::Mon, 3), "Washington's Birthday".into()));
            want.push((nth(4, Weekday::Mon, 3), "Patriots' Day".into()));
            want.push((last_monday_may, "Memorial Day".into()));
            want.push((nth(9, Weekday::Mon, 1), "Labor Day".into()));
            want.push((nth(10, Weekday::Mon, 2), "Columbus Day".into()));
            want.push((nth(11, Weekday::Thu, 4), "Thanksgiving Day".into()));
        }
        want.sort();
        let mut have: Vec<(NaiveDate, String)> = c
            .holidays
            .iter()
            .map(|r| {
                (
                    NaiveDate::parse_from_str(&r.date, "%Y-%m-%d").unwrap(),
                    r.name.clone(),
                )
            })
            .collect();
        have.sort();
        assert_eq!(have, want);
        // Trial Court closing dates published for 2026 and 2027 (mass.gov, updated 2026-03-20),
        // weekdays only: spot-check the ones the rule moves or skips.
        let cal = Calendar::load(false).unwrap();
        for (date, name) in [
            ("2026-06-19", "Juneteenth"),
            ("2026-11-26", "Thanksgiving Day"),
            ("2026-12-25", "Christmas Day"),
            ("2027-07-05", "Independence Day"),
            ("2027-01-18", "MLK Day"),
            ("2027-05-31", "Memorial Day"),
        ] {
            assert_eq!(
                cal.holiday(NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap()),
                Some(name)
            );
        }
        // Not a holiday: a Saturday Independence Day (2026) is not moved to Friday.
        assert_eq!(
            cal.holiday(NaiveDate::from_ymd_opt(2026, 7, 3).unwrap()),
            None
        );
    }

    #[test]
    fn times_and_dates_parse() {
        assert_eq!(minutes_of(parse_time("t", "6:00 pm").unwrap()), 18 * 60);
        assert_eq!(minutes_of(parse_time("t", "18:30").unwrap()), 18 * 60 + 30);
        assert_eq!(minutes_of(parse_time("t", "7 PM").unwrap()), 19 * 60);
        assert!(parse_time("t", "25:00").is_err());
        assert!(parse_date("d", "2026-13-01").is_err());
        assert!(parse_date("d", "1850-01-01").is_err());
        assert_eq!(
            show(dt("2026-11-16 18:00")),
            "Monday, November 16, 2026 at 6:00 PM"
        );
    }
}
