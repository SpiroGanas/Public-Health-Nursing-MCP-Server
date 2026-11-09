//! EQ-07 `find_community_resources`: searches a resource directory the nurse
//! maintains (CSV/XLSX in `/data`) and HRSA-funded health center sites
//! (data.hrsa.gov, live with cache and snapshot fallback) by town, service,
//! language, and hours.
//!
//! Directory columns (case-insensitive; only `name`, `services`, and `town`
//! are required): name, services (semicolon-separated), town, address, phone,
//! website, languages (semicolon-separated), hours ("Mon-Fri 9:00-17:00; Sat
//! 10:00-14:00" or "24/7"), eligibility, cost, last_verified (YYYY-MM-DD).
//! Files that look like client lists (names, dates of birth, record numbers)
//! are refused.

use chrono::{Datelike, NaiveDate, NaiveTime, Timelike, Weekday};
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_local::phi::Identifier;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::core::practice;
use crate::md;

/// Default directory location in /data.
pub const DEFAULT_DIRECTORY: &str = "resources/community_resources.csv";

/// Service categories and the words that select them.
const CATEGORIES: &[(&str, &[&str])] = &[
    (
        "food",
        &[
            "food",
            "pantry",
            "meals",
            "meal",
            "groceries",
            "snap",
            "wic",
            "nutrition",
            "hunger",
        ],
    ),
    (
        "housing",
        &[
            "housing",
            "shelter",
            "homeless",
            "homelessness",
            "rent",
            "eviction",
            "rental",
        ],
    ),
    (
        "utilities",
        &[
            "utilities",
            "utility",
            "fuel",
            "heating",
            "heat",
            "electric",
            "energy",
            "heap",
        ],
    ),
    (
        "transportation",
        &["transportation", "rides", "ride", "transit", "bus", "van"],
    ),
    (
        "primary_care",
        &[
            "primary care",
            "health center",
            "clinic",
            "medical",
            "doctor",
            "fqhc",
            "health care",
        ],
    ),
    ("dental", &["dental", "dentist", "oral health", "teeth"]),
    (
        "behavioral_health",
        &[
            "behavioral health",
            "mental health",
            "counseling",
            "therapy",
            "psychiatric",
        ],
    ),
    (
        "substance_use",
        &[
            "substance use",
            "recovery",
            "addiction",
            "opioid",
            "detox",
            "harm reduction",
            "naloxone",
        ],
    ),
    (
        "safety",
        &[
            "domestic violence",
            "safety",
            "abuse",
            "sexual assault",
            "dv",
        ],
    ),
    (
        "legal_immigration",
        &["legal", "immigration", "lawyer", "attorney", "asylum"],
    ),
    (
        "employment",
        &["employment", "job", "jobs", "work", "career", "masshire"],
    ),
    (
        "education",
        &[
            "education",
            "esol",
            "esl",
            "english classes",
            "ged",
            "training",
            "literacy",
        ],
    ),
    (
        "child_family",
        &[
            "child care",
            "childcare",
            "parenting",
            "family",
            "early intervention",
            "home visiting",
        ],
    ),
    (
        "older_adults",
        &[
            "older adults",
            "elder",
            "seniors",
            "council on aging",
            "aging",
        ],
    ),
    (
        "disability",
        &[
            "disability",
            "disabilities",
            "accessible",
            "independent living",
        ],
    ),
];

/// Categories that HRSA health center sites can satisfy.
const HEALTH_CATEGORIES: &[&str] = &[
    "primary_care",
    "dental",
    "behavioral_health",
    "substance_use",
];

/// Arguments for `find_community_resources`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ResourceSearchArgs {
    /// Massachusetts city or town (default: the configured jurisdiction).
    pub town: Option<String>,
    /// Service wanted: a category (food, housing, utilities, transportation,
    /// primary_care, dental, behavioral_health, substance_use, safety,
    /// legal_immigration, employment, education, child_family, older_adults,
    /// disability) or any words ("fuel assistance").
    pub service: Option<String>,
    /// Language the resource must offer (directory entries only).
    pub language: Option<String>,
    /// Day it must be open: monday ... sunday, or "today".
    pub open_on: Option<String>,
    /// Time it must be open on that day, HH:MM (24-hour).
    pub open_at: Option<String>,
    /// Include resources elsewhere in the same county (default true).
    pub include_county: Option<bool>,
    /// Include HRSA-funded health center sites (default true).
    pub include_health_centers: Option<bool>,
    /// Directory file in /data (default resources/community_resources.csv when it exists).
    pub directory_path: Option<String>,
    /// Sheet name for an XLSX directory.
    pub sheet: Option<String>,
    /// Most results to return (default 15, at most 50).
    pub max_results: Option<usize>,
}

/// One resource.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ResourceHit {
    /// "directory" (your file) or "hrsa" (HRSA health center site).
    pub source: String,
    pub name: String,
    pub organization: Option<String>,
    pub services: Vec<String>,
    pub town: String,
    pub county: Option<String>,
    pub address: Option<String>,
    pub phone: Option<String>,
    pub website: Option<String>,
    pub languages: Vec<String>,
    pub hours: Option<String>,
    /// Whether it is open at the requested day/time (null = unknown).
    pub open_match: Option<bool>,
    pub eligibility: Option<String>,
    pub cost: Option<String>,
    pub last_verified: Option<String>,
    /// Last verified more than 12 months ago (or never).
    pub needs_reverification: bool,
    /// "same town" or "same county".
    pub proximity: String,
}

/// Result of `find_community_resources`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ResourceSearchResult {
    pub town: Option<String>,
    pub county: Option<String>,
    pub service_categories: Vec<String>,
    pub results: Vec<ResourceHit>,
    pub directory_file: Option<String>,
    pub directory_entries: usize,
    pub health_center_sites_searched: usize,
    pub health_center_data_mode: Option<String>,
    /// The directory schema, for building or fixing the file.
    pub directory_schema: Vec<String>,
}

fn categories_for(q: &str) -> Vec<&'static str> {
    let p = practice::padded_words(q);
    let n = phn_core::text::normalize(q).replace(' ', "_");
    CATEGORIES
        .iter()
        .filter(|(id, words)| {
            *id == n || words.iter().any(|w| p.contains(&practice::padded_words(w)))
        })
        .map(|(id, _)| *id)
        .collect()
}

fn parse_day(s: &str) -> Option<Weekday> {
    let k: String = s.trim().to_ascii_lowercase().chars().take(3).collect();
    Some(match k.as_str() {
        "mon" => Weekday::Mon,
        "tue" => Weekday::Tue,
        "wed" => Weekday::Wed,
        "thu" => Weekday::Thu,
        "fri" => Weekday::Fri,
        "sat" => Weekday::Sat,
        "sun" => Weekday::Sun,
        _ => return None,
    })
}

fn parse_clock(s: &str, default_pm: bool) -> Option<u32> {
    let t = s.trim().to_ascii_lowercase().replace(' ', "");
    let (body, pm, am) = if let Some(b) = t.strip_suffix("pm") {
        (b.to_string(), true, false)
    } else if let Some(b) = t.strip_suffix("am") {
        (b.to_string(), false, true)
    } else {
        (t.clone(), false, false)
    };
    let (h, m) = match body.split_once(':') {
        Some((h, m)) => (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?),
        None => (body.parse::<u32>().ok()?, 0),
    };
    if h > 24 || m > 59 {
        return None;
    }
    let h = if pm && h < 12 {
        h + 12
    } else if am && h == 12 {
        0
    } else if !pm && !am && default_pm && h < 8 {
        h + 12
    } else {
        h
    };
    Some(h * 60 + m)
}

/// Parsed opening hours: (days, open minute, close minute).
type Spans = Vec<(Vec<Weekday>, u32, u32)>;

fn parse_hours(s: &str) -> Option<Spans> {
    let all = [
        Weekday::Mon,
        Weekday::Tue,
        Weekday::Wed,
        Weekday::Thu,
        Weekday::Fri,
        Weekday::Sat,
        Weekday::Sun,
    ];
    let t = s.trim().to_ascii_lowercase();
    if t.contains("24/7") || t.contains("24 hours") {
        return Some(vec![(all.to_vec(), 0, 24 * 60)]);
    }
    let mut out = Vec::new();
    for part in t.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        // Split the day spec from the time range at the first digit.
        let i = part.find(|c: char| c.is_ascii_digit())?;
        let (days_s, time_s) = part.split_at(i);
        let days_s = days_s.trim().trim_end_matches(':');
        let mut days: Vec<Weekday> = Vec::new();
        if days_s.is_empty() || days_s.contains("daily") || days_s.contains("every day") {
            days = all.to_vec();
        } else if days_s.contains("weekdays") {
            days = all[..5].to_vec();
        } else if days_s.contains("weekend") {
            days = all[5..].to_vec();
        } else {
            for seg in days_s.split([',', '&', '/']) {
                let seg = seg.trim();
                if seg.is_empty() || seg == "and" {
                    continue;
                }
                match seg.split_once('-') {
                    Some((a, b)) => {
                        let (a, b) = (parse_day(a)?, parse_day(b)?);
                        let (ai, bi) = (a.num_days_from_monday(), b.num_days_from_monday());
                        let mut k = ai;
                        loop {
                            days.push(all[k as usize]);
                            if k == bi {
                                break;
                            }
                            k = (k + 1) % 7;
                        }
                    }
                    None => days.push(parse_day(seg)?),
                }
            }
        }
        let (a, b) = time_s.split_once(['-', '–'])?;
        let b_pm = b.contains("pm");
        let open = parse_clock(a, false)?;
        let close = parse_clock(b, !b_pm && !b.contains("am"))?;
        out.push((days, open, close));
    }
    (!out.is_empty()).then_some(out)
}

fn is_open(spans: &Spans, day: Weekday, minute: Option<u32>) -> bool {
    spans
        .iter()
        .any(|(days, a, b)| days.contains(&day) && minute.is_none_or(|m| m >= *a && m < *b))
}

/// Refuse files that look like client data (never echo values).
fn directory_guard(lt: &crate::localdata::LocalTable) -> Result<()> {
    let person_headers = [
        "first name",
        "last name",
        "dob",
        "date of birth",
        "birth date",
        "ssn",
        "mrn",
        "patient",
        "client",
        "student",
        "member id",
        "medical record",
    ];
    let bad_headers: Vec<&String> = lt
        .table
        .headers
        .iter()
        .filter(|h| {
            let n = phn_core::text::normalize(h);
            person_headers
                .iter()
                .any(|p| n == *p || n.starts_with(&format!("{p} ")))
        })
        .collect();
    let bad_kinds: Vec<&str> = lt
        .findings
        .iter()
        .filter(|f| {
            matches!(
                f.kind,
                Identifier::BirthDate
                    | Identifier::Ssn
                    | Identifier::MedicalRecordNumber
                    | Identifier::HealthPlanNumber
                    | Identifier::AccountNumber
            )
        })
        .map(|f| f.kind.label())
        .collect();
    if !bad_headers.is_empty() || !bad_kinds.is_empty() {
        return Err(PhnError::Privacy(format!(
            "'{}' looks like a list of people (columns {}{}), not a resource directory, so it was \
             not searched. A directory lists organizations: name, services, town, address, phone, \
             website, languages, hours, eligibility, cost, last_verified.",
            lt.label,
            bad_headers
                .iter()
                .map(|h| format!("'{h}'"))
                .collect::<Vec<_>>()
                .join(", "),
            if bad_kinds.is_empty() {
                String::new()
            } else {
                format!("; detected {}", bad_kinds.join(", "))
            }
        )));
    }
    Ok(())
}

fn split_list(s: &str) -> Vec<String> {
    s.split([';', '|'])
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect()
}

fn schema_lines() -> Vec<String> {
    vec![
        "name (required): organization or program name".into(),
        "services (required): semicolon-separated, e.g. 'food pantry; WIC'".into(),
        "town (required): Massachusetts city or town".into(),
        "address, phone, website".into(),
        "languages: semicolon-separated, e.g. 'English; Spanish; Portuguese'".into(),
        "hours: e.g. 'Mon-Fri 9:00-17:00; Sat 10:00-14:00' or '24/7'".into(),
        "eligibility, cost".into(),
        "last_verified: YYYY-MM-DD (entries older than 12 months are flagged)".into(),
        format!("default location: /data/{DEFAULT_DIRECTORY}"),
    ]
}

/// Search community resources.
pub async fn find_community_resources(
    ctx: &Context,
    args: ResourceSearchArgs,
) -> Result<Envelope<ResourceSearchResult>> {
    let today = chrono::Local::now().date_naive();
    let max = args.max_results.unwrap_or(15).clamp(1, 50);
    let town_q = args
        .town
        .clone()
        .filter(|t| !t.trim().is_empty())
        .or_else(|| ctx.settings().jurisdiction.town);
    let (town, county) = match &town_q {
        Some(t) => match ctx.ma.town(t) {
            Some(m) => (Some(m.name.clone()), Some(m.county.clone())),
            None => {
                let s: Vec<String> = ctx
                    .ma
                    .suggest_towns(t, 5)
                    .iter()
                    .map(|m| m.name.clone())
                    .collect();
                return Err(PhnError::InvalidInput(format!(
                    "'{t}' is not a Massachusetts city or town{}",
                    if s.is_empty() {
                        String::new()
                    } else {
                        format!("; did you mean {}?", s.join(", "))
                    }
                )));
            }
        },
        None => (None, None),
    };
    let cats: Vec<&'static str> = args
        .service
        .as_deref()
        .map(categories_for)
        .unwrap_or_default();
    let service_words = args
        .service
        .as_deref()
        .map(practice::padded_words)
        .unwrap_or_default();
    let day = match args.open_on.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(d) if d.eq_ignore_ascii_case("today") => Some(today.weekday()),
        Some(d) => Some(
            parse_day(d)
                .ok_or_else(|| PhnError::InvalidInput(format!("open_on '{d}' is not a weekday")))?,
        ),
    };
    let minute = match args.open_at.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(t) => Some(
            NaiveTime::parse_from_str(t, "%H:%M")
                .map(|x| x.hour() * 60 + x.minute())
                .map_err(|_| PhnError::InvalidInput("open_at must be HH:MM (24-hour)".into()))?,
        ),
    };
    if minute.is_some() && day.is_none() {
        return Err(PhnError::InvalidInput(
            "give open_on (a weekday or 'today') with open_at".into(),
        ));
    }
    let include_county = args.include_county.unwrap_or(true);
    let proximity = |t: &str, c: Option<&str>| -> Option<String> {
        match (&town, &county) {
            (None, _) => Some("any".into()),
            (Some(tw), _) if phn_core::text::normalize(tw) == phn_core::text::normalize(t) => {
                Some("same town".into())
            }
            (_, Some(cy)) if include_county && c.is_some_and(|c| c == cy) => {
                Some("same county".into())
            }
            _ => None,
        }
    };
    let mut caveats: Vec<String> = Vec::new();
    let mut sources: Vec<Source> = Vec::new();
    let mut results: Vec<(i32, ResourceHit)> = Vec::new();

    // ---- The nurse's directory
    let dir_path = match args
        .directory_path
        .as_deref()
        .filter(|p| !p.trim().is_empty())
    {
        Some(p) => Some(p.to_string()),
        None => ctx
            .sandbox()
            .ok()
            .and_then(|sb| sb.resolve_existing(DEFAULT_DIRECTORY).ok())
            .map(|_| DEFAULT_DIRECTORY.to_string()),
    };
    let mut directory_file = None;
    let mut directory_entries = 0;
    if let Some(p) = &dir_path {
        let lt = crate::localdata::load(ctx, p, args.sheet.as_deref())?;
        directory_guard(&lt)?;
        let t = &lt.table;
        let col = |names: &[&str]| practice::find_col(t, names);
        let (Some(name_c), Some(svc_c), Some(town_c)) = (
            col(&["name", "organization", "program", "resource", "agency"]),
            col(&["services", "service", "category", "categories"]),
            col(&["town", "city", "municipality"]),
        ) else {
            return Err(PhnError::InvalidInput(format!(
                "the directory needs name, services, and town columns. Expected columns: {}",
                schema_lines().join("; ")
            )));
        };
        let addr_c = col(&["address", "street_address"]);
        let phone_c = col(&["phone", "telephone", "phone_number"]);
        let web_c = col(&["website", "web", "url"]);
        let lang_c = col(&["languages", "language"]);
        let hours_c = col(&["hours", "open_hours", "hours_of_operation"]);
        let elig_c = col(&["eligibility", "who"]);
        let cost_c = col(&["cost", "fees", "fee"]);
        let ver_c = col(&["last_verified", "verified", "date_verified"]);
        directory_file = Some(lt.label.clone());
        sources.push(lt.source());
        let mut unparsed_hours = 0;
        for row in &t.rows {
            let cell = |i: Option<usize>| {
                i.and_then(|i| row.get(i))
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            };
            let Some(name) = cell(Some(name_c)) else {
                continue;
            };
            directory_entries += 1;
            let rtown = cell(Some(town_c)).unwrap_or_default();
            let rcounty = ctx.ma.town(&rtown).map(|m| m.county.clone());
            let Some(prox) = proximity(&rtown, rcounty.as_deref()) else {
                continue;
            };
            let services = split_list(&cell(Some(svc_c)).unwrap_or_default());
            let svc_text = practice::padded_words(&format!("{name} {}", services.join(" ")));
            let mut score = if prox == "same town" { 20 } else { 10 };
            if args.service.is_some() {
                let cat_hit = cats.iter().any(|c| {
                    CATEGORIES
                        .iter()
                        .find(|(id, _)| id == c)
                        .is_some_and(|(_, words)| {
                            words
                                .iter()
                                .any(|w| svc_text.contains(&practice::padded_words(w)))
                        })
                });
                let word_hit = service_words
                    .split_whitespace()
                    .filter(|w| w.len() >= 3)
                    .any(|w| svc_text.contains(&format!(" {w} ")));
                if !cat_hit && !word_hit {
                    continue;
                }
                score += if cat_hit { 5 } else { 3 };
            }
            let languages = split_list(&cell(lang_c).unwrap_or_default());
            if let Some(l) = args.language.as_deref() {
                let ln = phn_core::text::normalize(l);
                if !languages
                    .iter()
                    .any(|x| phn_core::text::normalize(x).starts_with(&ln))
                {
                    continue;
                }
            }
            let hours = cell(hours_c);
            let spans = hours.as_deref().and_then(parse_hours);
            if hours.is_some() && spans.is_none() {
                unparsed_hours += 1;
            }
            let open_match = match (day, &spans) {
                (None, _) => None,
                (Some(d), Some(sp)) => Some(is_open(sp, d, minute)),
                (Some(_), None) => None,
            };
            if open_match == Some(false) {
                continue;
            }
            let verified = cell(ver_c);
            let stale = verified
                .as_deref()
                .and_then(phn_local::phi::parse_date)
                .is_none_or(|d: NaiveDate| (today - d).num_days() > 365);
            results.push((
                score,
                ResourceHit {
                    source: "directory".into(),
                    name,
                    organization: None,
                    services,
                    town: rtown,
                    county: rcounty,
                    address: cell(addr_c),
                    phone: cell(phone_c),
                    website: cell(web_c),
                    languages,
                    hours,
                    open_match,
                    eligibility: cell(elig_c),
                    cost: cell(cost_c),
                    last_verified: verified,
                    needs_reverification: stale,
                    proximity: prox,
                },
            ));
        }
        if unparsed_hours > 0 {
            caveats.push(format!(
                "{unparsed_hours} directory entr(ies) have hours the tool could not read; use \
                 'Mon-Fri 9:00-17:00; Sat 10:00-14:00' or '24/7'."
            ));
        }
    } else {
        caveats.push(format!(
            "No resource directory was found at /data/{DEFAULT_DIRECTORY}. Create one (columns: \
             name, services, town, address, phone, website, languages, hours, eligibility, cost, \
             last_verified) to search local food, housing, and other resources; only HRSA health \
             centers were searched."
        ));
    }

    // ---- HRSA health centers
    let want_hc = args.include_health_centers.unwrap_or(true)
        && (args.service.is_none() || cats.iter().any(|c| HEALTH_CATEGORIES.contains(c)));
    let mut hc_count = 0;
    let mut hc_mode = None;
    if want_hc {
        match phn_data::hrsa::ma_sites(&ctx.http, ctx.cache.as_ref(), &ctx.snapshots).await {
            Ok(s) => {
                hc_count = s.sites.len();
                hc_mode = Some(s.mode.label().to_string());
                if let Some(r) = &s.fallback_reason {
                    caveats.push(format!("HRSA health centers: {r}."));
                }
                let mut src = ctx
                    .snapshots
                    .entry(phn_data::hrsa::SNAPSHOT_ID)
                    .map(|(e, _)| e.to_source())
                    .unwrap_or_else(|| Source::computed("HRSA health center sites", "HRSA"));
                src.mode = s.mode;
                src.url = Some(phn_data::hrsa::HRSA_SITES_URL.into());
                src.retrieved = Some(s.as_of.chars().take(10).collect());
                sources.push(src);
                let dental = cats.contains(&"dental");
                for site in s.sites {
                    let Some(prox) = proximity(&site.city, Some(&site.county)) else {
                        continue;
                    };
                    if args.language.is_some() || day.is_some() {
                        // HRSA does not publish languages or daily hours.
                        continue;
                    }
                    let mut score = if prox == "same town" { 19 } else { 9 };
                    let lname = site.site_name.to_lowercase();
                    if dental && lname.contains("dental") {
                        score += 4;
                    }
                    if dental && !lname.contains("dental") {
                        score -= 2;
                    }
                    if site.setting == "School" && !service_words.contains(" school ") {
                        score -= 3;
                    }
                    let mut services = vec!["health center (FQHC)".to_string()];
                    if site.setting != "Unknown" && site.setting != "All Other Clinic Types" {
                        services.push(format!("setting: {}", site.setting));
                    }
                    if site.location_type != "Permanent" {
                        services.push(site.location_type.to_lowercase());
                    }
                    results.push((
                        score,
                        ResourceHit {
                            source: "hrsa".into(),
                            name: site.site_name,
                            organization: Some(site.health_center),
                            services,
                            town: site.city,
                            county: Some(site.county),
                            address: Some(site.address),
                            phone: (!site.phone.is_empty()).then_some(site.phone),
                            website: (!site.website.is_empty()).then_some(site.website),
                            languages: Vec::new(),
                            hours: Some(format!(
                                "{} hours/week{}",
                                site.hours_per_week.trim_end_matches(".00"),
                                if site.schedule != "Unknown" {
                                    format!(", {}", site.schedule.to_lowercase())
                                } else {
                                    String::new()
                                }
                            )),
                            open_match: None,
                            eligibility: Some(
                                "Health centers serve everyone, with sliding-fee discounts based \
                                 on income"
                                    .into(),
                            ),
                            cost: None,
                            last_verified: None,
                            needs_reverification: false,
                            proximity: prox,
                        },
                    ));
                }
                if args.language.is_some() || day.is_some() {
                    caveats.push(
                        "HRSA's site data have no languages or daily hours, so health centers \
                         were left out of a search by language or opening time."
                            .into(),
                    );
                }
            }
            Err(e) => caveats.push(format!("HRSA health center data were not available ({e}).")),
        }
    }

    results.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.name.cmp(&b.1.name)));
    let total = results.len();
    let results: Vec<ResourceHit> = results.into_iter().take(max).map(|(_, r)| r).collect();

    // ---- Markdown
    let mut s = format!(
        "**Community resources{}{}**\n\n",
        town.as_deref()
            .map(|t| format!(" near {t}"))
            .unwrap_or_default(),
        args.service
            .as_deref()
            .map(|v| format!(" — {v}"))
            .unwrap_or_default()
    );
    if results.is_empty() {
        s.push_str("No matching resources.\n");
    } else {
        let rows: Vec<Vec<String>> = results
            .iter()
            .map(|r| {
                vec![
                    r.name.clone(),
                    r.services.join("; "),
                    format!("{} ({})", r.town, r.proximity),
                    r.phone.clone().unwrap_or_default(),
                    r.languages.join(", "),
                    r.hours.clone().unwrap_or_default(),
                    if r.source == "hrsa" {
                        "HRSA".into()
                    } else if r.needs_reverification {
                        "your directory (re-verify)".into()
                    } else {
                        "your directory".into()
                    },
                ]
            })
            .collect();
        s.push_str(&md::table(
            &[
                "Resource",
                "Services",
                "Town",
                "Phone",
                "Languages",
                "Hours",
                "Source",
            ],
            &rows,
        ));
        if total > results.len() {
            s.push_str(&format!(
                "\nShowing {} of {total} matches; narrow by town, service, language, or hours.\n",
                results.len()
            ));
        }
    }
    caveats.push(
        "Confirm hours, eligibility, and services before referring; directory entries are only \
         as current as their last_verified date."
            .into(),
    );
    if results
        .iter()
        .any(|r| r.needs_reverification && r.source == "directory")
    {
        caveats.push(
            "Some directory entries were last verified more than 12 months ago (or have no date)."
                .into(),
        );
    }
    Ok(Envelope::new(
        s,
        ResourceSearchResult {
            town,
            county,
            service_categories: cats.iter().map(|c| c.to_string()).collect(),
            results,
            directory_file,
            directory_entries,
            health_center_sites_searched: hc_count,
            health_center_data_mode: hc_mode,
            directory_schema: schema_lines(),
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        notes: vec![
            "Only organization listings are returned; files that look like client lists are \
             refused."
                .into(),
        ],
        ..Privacy::none()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIR: &str = "name,services,town,address,phone,languages,hours,last_verified\n\
        Example Pantry,food pantry,Amherst,1 Main,413-555-0190,English; Spanish,Tue 15:00-18:00; Sat 9:00-12:00,2026-09-01\n\
        Example Rides,transportation,Hadley,2 Main,413-555-0191,English,Mon-Fri 8am-4pm,2024-01-01\n\
        Far Away Meals,food,Boston,3 Main,617-555-0192,English,24/7,2026-09-01\n";

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data/resources")).unwrap();
        std::fs::write(
            dir.path().join("data/resources/community_resources.csv"),
            DIR,
        )
        .unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    fn run(c: &Context, a: ResourceSearchArgs) -> Result<Envelope<ResourceSearchResult>> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(find_community_resources(c, a))
    }

    #[test]
    fn hours_parse() {
        let sp = parse_hours("Mon-Fri 8am-4pm").unwrap();
        assert!(is_open(&sp, Weekday::Wed, Some(9 * 60)));
        assert!(!is_open(&sp, Weekday::Sat, None));
        assert!(!is_open(&sp, Weekday::Mon, Some(16 * 60 + 30)));
        let sp = parse_hours("Tue 15:00-18:00; Sat 9:00-12:00").unwrap();
        assert!(is_open(&sp, Weekday::Sat, Some(10 * 60)));
        assert!(is_open(
            &parse_hours("24/7").unwrap(),
            Weekday::Sun,
            Some(3 * 60)
        ));
        let sp = parse_hours("Mon, Wed 9-5").unwrap();
        assert!(is_open(&sp, Weekday::Wed, Some(16 * 60)));
        assert!(parse_hours("by appointment").is_none());
    }

    #[test]
    fn searches_directory_by_town_service_language_and_hours() {
        let (_d, c) = ctx();
        let env = run(
            &c,
            ResourceSearchArgs {
                town: Some("Amherst".into()),
                service: Some("food".into()),
                language: Some("Spanish".into()),
                open_on: Some("saturday".into()),
                open_at: Some("10:00".into()),
                ..ResourceSearchArgs::default()
            },
        )
        .unwrap();
        let r = &env.data.results;
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, "Example Pantry");
        assert_eq!(r[0].open_match, Some(true));
        // Boston is outside Hampshire County.
        let env = run(
            &c,
            ResourceSearchArgs {
                town: Some("Amherst".into()),
                service: Some("transportation".into()),
                ..ResourceSearchArgs::default()
            },
        )
        .unwrap();
        let r = &env.data.results;
        assert_eq!(r[0].name, "Example Rides");
        assert_eq!(r[0].proximity, "same county");
        assert!(r[0].needs_reverification);
        assert_eq!(env.data.health_center_sites_searched, 0);
    }

    #[test]
    fn health_centers_come_from_the_snapshot_offline() {
        let (_d, c) = ctx();
        let env = run(
            &c,
            ResourceSearchArgs {
                town: Some("Springfield".into()),
                service: Some("dental".into()),
                include_county: Some(false),
                ..ResourceSearchArgs::default()
            },
        )
        .unwrap();
        assert!(env.data.health_center_sites_searched > 200);
        assert_eq!(
            env.data.health_center_data_mode.as_deref(),
            Some("snapshot")
        );
        assert!(env.data.results.iter().all(|r| r.town == "Springfield"));
        assert!(env.data.results.iter().any(|r| r.source == "hrsa"));
    }

    #[test]
    fn client_lists_are_refused() {
        let (dir, c) = ctx();
        std::fs::write(
            dir.path().join("data/people.csv"),
            "First Name,Last Name,DOB,town,services\nZed,Quimby,2001-01-01,Amherst,food\n",
        )
        .unwrap();
        let e = run(
            &c,
            ResourceSearchArgs {
                directory_path: Some("people.csv".into()),
                ..ResourceSearchArgs::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("looks like a list of people"));
        assert!(!e.to_string().contains("Quimby"));
        let e = run(
            &c,
            ResourceSearchArgs {
                town: Some("Nowheresville".into()),
                ..ResourceSearchArgs::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("not a Massachusetts"));
    }
}
