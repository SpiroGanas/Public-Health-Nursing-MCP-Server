//! Shared pieces of the CDC surveillance tools (CD-08, CD-09): MMWR week
//! arithmetic, bundled-snapshot fallbacks for Socrata queries, and helpers
//! for describing where data came from.

use chrono::{Datelike, Duration, NaiveDate};
use phn_core::SourceMode;
use phn_data::fetch::SnapshotFallback;
use phn_data::socrata::SocrataRows;

use crate::context::Context;

/// First day (a Sunday) of MMWR week 1 of `year`: the Sunday of the week that
/// contains January 4 (MMWR week 1 is the first week with at least four days
/// in the calendar year).
pub fn mmwr_year_start(year: i32) -> NaiveDate {
    let jan4 = NaiveDate::from_ymd_opt(year, 1, 4).expect("valid date");
    jan4 - Duration::days(i64::from(jan4.weekday().num_days_from_sunday()))
}

/// Number of MMWR weeks in `year` (52 or 53).
pub fn mmwr_weeks_in_year(year: i32) -> u32 {
    ((mmwr_year_start(year + 1) - mmwr_year_start(year)).num_days() / 7) as u32
}

/// The Saturday that ends MMWR week `week` of `year`.
pub fn mmwr_week_end(year: i32, week: u32) -> NaiveDate {
    mmwr_year_start(year) + Duration::days(7 * i64::from(week) - 1)
}

/// MMWR year and week containing `date`.
pub fn mmwr_week_of(date: NaiveDate) -> (i32, u32) {
    let mut year = date.year() + 1;
    while mmwr_year_start(year) > date {
        year -= 1;
    }
    let week = ((date - mmwr_year_start(year)).num_days() / 7 + 1) as u32;
    (year, week)
}

/// A snapshot fallback that reads one bundled snapshot file and turns it into
/// a Socrata-shaped JSON body with `transform` (which may filter rows).
pub fn snapshot_fallback<F>(
    ctx: &Context,
    id: &'static str,
    file: &'static str,
    transform: F,
) -> SnapshotFallback
where
    F: FnOnce(&str) -> Option<Vec<u8>> + Send + 'static,
{
    let store = ctx.snapshots.clone();
    Box::new(move || {
        let (text, entry, _) = store.file_text(id, file).ok()?;
        let body = transform(&text)?;
        Some((body, entry.retrieved))
    })
}

/// The bundled snapshot's vintage text (for caveats), e.g. "MMWR 2026 week 43".
pub fn snapshot_vintage(ctx: &Context, id: &str) -> Option<String> {
    ctx.snapshots.entry(id).map(|(e, _)| e.source_vintage)
}

/// A caveat describing a fallback, when one was used.
pub fn fallback_caveat(
    ctx: &Context,
    rows: &SocrataRows,
    what: &str,
    snapshot_id: &str,
) -> Option<String> {
    match rows.mode {
        SourceMode::Snapshot => Some(format!(
            "{what}: live data could not be reached, so the bundled snapshot was used ({}, \
             retrieved {}). It may be weeks out of date.",
            snapshot_vintage(ctx, snapshot_id).unwrap_or_else(|| "undated".into()),
            rows.as_of.chars().take(10).collect::<String>()
        )),
        SourceMode::Cache if rows.fallback_reason.is_some() => Some(format!(
            "{what}: live data could not be reached, so a cached copy from {} was used.",
            rows.as_of.chars().take(10).collect::<String>()
        )),
        _ => None,
    }
}

/// Mode label for a list of results (the least current one wins).
pub fn combined_mode(modes: &[SourceMode]) -> SourceMode {
    if modes.contains(&SourceMode::Snapshot) {
        SourceMode::Snapshot
    } else if modes.contains(&SourceMode::Cache) {
        SourceMode::Cache
    } else {
        SourceMode::Live
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    /// Reference values from CDC's MMWR week calendars: 2026 week 1 begins
    /// Sunday 2026-01-04; 2025 week 1 begins 2024-12-29 and 2025 has 53
    /// weeks (NNDSS publishes a week 53 for 2025); 2021 week 1 begins
    /// 2021-01-03; 2020 had 53 weeks.
    #[test]
    fn mmwr_weeks_match_cdc_calendars() {
        assert_eq!(mmwr_year_start(2026), d("2026-01-04"));
        assert_eq!(mmwr_year_start(2025), d("2024-12-29"));
        assert_eq!(mmwr_year_start(2021), d("2021-01-03"));
        assert_eq!(mmwr_weeks_in_year(2025), 53);
        assert_eq!(mmwr_weeks_in_year(2020), 53);
        assert_eq!(mmwr_weeks_in_year(2024), 52);
        assert_eq!(mmwr_weeks_in_year(2026), 52);
        // NHSN and NNDSS both report MMWR 2026 week 43 as ending 2026-10-31.
        assert_eq!(mmwr_week_end(2026, 43), d("2026-10-31"));
        assert_eq!(mmwr_week_of(d("2026-10-31")), (2026, 43));
        assert_eq!(mmwr_week_of(d("2026-10-25")), (2026, 43));
        assert_eq!(mmwr_week_of(d("2026-01-03")), (2025, 53));
        assert_eq!(mmwr_week_of(d("2024-12-29")), (2025, 1));
    }
}
