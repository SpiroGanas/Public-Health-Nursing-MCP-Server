//! COMM-03 `create_social_media_campaign`: a posting calendar tied to health
//! observances, checks of draft posts (platform length, alt text, hashtags,
//! emoji, capitals, readability, jargon, and safe messaging for suicide
//! topics), and an XLSX calendar in /data/output.
//!
//! The assistant writes the post text; this tool plans the calendar and checks
//! the drafts. Platform limits and observance dates could not be verified
//! from this environment (see `content/comms/campaigns.yaml`), so both are
//! marked low confidence and can be overridden.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result};
use phn_docs::model::{Block, Document, TableBlock};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::content::{self, CAMPAIGNS, NamedSource};
use super::readability::{AnalyzeOptions, analyze, default_target_grade};
use super::safe_messaging;
use crate::context::Context;
use crate::md;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Platform {
    pub id: String,
    pub name: String,
    pub char_limit: usize,
    pub link_counts_as: Option<usize>,
    pub alt_text_limit: Option<usize>,
    pub hashtag_limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Observance {
    pub name: String,
    pub kind: String,
    pub month: u32,
    #[serde(default)]
    pub day: Option<u32>,
    #[serde(default)]
    pub nth: Option<u32>,
    #[serde(default)]
    pub weekday: Option<u32>,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct PostRule {
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct NamedText {
    pub name: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ChecklistItem {
    pub id: String,
    pub text: String,
    pub section: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CampaignContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, NamedSource>,
    pub platforms: Vec<Platform>,
    pub observances: Vec<Observance>,
    pub post_rules: Vec<PostRule>,
    pub cerc_principles: Vec<NamedText>,
    pub press_release_checklist: Vec<ChecklistItem>,
    pub media_advisory_elements: Vec<ChecklistItem>,
    pub spokesperson_questions: Vec<String>,
    pub spokesperson_tips: Vec<String>,
}

pub(crate) static CAMPAIGN: LazyLock<CampaignContent> = LazyLock::new(|| {
    content::load(CAMPAIGNS).expect("content/comms/campaigns.yaml matches its schema")
});

// ---------------------------------------------------------------- arguments and results

/// A draft post to check.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct PostDraft {
    /// "x", "facebook", "instagram", "linkedin", "bluesky", or "threads".
    pub platform: String,
    pub text: String,
    /// Alt text for the image, if the post has one.
    pub alt_text: Option<String>,
    /// True when the post has an image or video.
    pub has_image: bool,
    /// Planned date (YYYY-MM-DD).
    pub date: Option<String>,
}

/// Arguments for `create_social_media_campaign`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct SocialCampaignArgs {
    /// Campaign topic (e.g. "flu vaccine", "naloxone", "lead poisoning").
    pub topic: String,
    /// Platforms (default facebook, instagram, x).
    pub platforms: Vec<String>,
    /// First day of the campaign (default today).
    pub start_date: Option<String>,
    /// Campaign length in weeks (default 4, at most 26).
    pub weeks: Option<u32>,
    /// Posts per platform per week (default 2, at most 7).
    pub posts_per_week: Option<u32>,
    /// Draft posts to check.
    pub posts: Vec<PostDraft>,
    /// Your own observance calendar in /data (CSV/XLSX with columns name, start, end; dates
    /// YYYY-MM-DD) instead of the bundled list.
    pub observances_path: Option<String>,
    /// Override platform character limits, e.g. {"x": 280} (the bundled limits are unverified).
    pub char_limits: BTreeMap<String, usize>,
    /// Reading grade to aim for (default 8).
    pub target_grade: Option<f64>,
    /// Write the calendar and checks to /data/output ("xlsx").
    pub export: Option<String>,
}

/// An observance in the campaign window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ObservanceMatch {
    pub name: String,
    pub start: String,
    pub end: String,
    /// The observance's keywords match the topic.
    pub related_to_topic: bool,
    /// "bundled (unverified)" or the local file.
    pub source: String,
}

/// A planned posting slot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CalendarSlot {
    pub date: String,
    pub platform: String,
    /// A related observance on that date, if any.
    pub observance: Option<String>,
}

/// Checks of one draft post.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PostCheck {
    /// 1-based position in `posts`.
    pub post: usize,
    pub platform: String,
    /// Characters as the platform counts them (links counted as the platform's fixed length where known).
    pub characters: usize,
    pub limit: usize,
    pub within_limit: bool,
    pub reading_grade: f64,
    pub problems: Vec<String>,
}

/// Result of `create_social_media_campaign`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SocialCampaignResult {
    pub topic: String,
    pub start: String,
    pub end: String,
    pub platforms: Vec<String>,
    pub observances: Vec<ObservanceMatch>,
    pub calendar: Vec<CalendarSlot>,
    pub post_checks: Vec<PostCheck>,
    /// Platform limits used, with whether they were overridden.
    pub limits: BTreeMap<String, usize>,
    pub accessibility_rules: Vec<String>,
    pub export_file: Option<String>,
}

// ---------------------------------------------------------------- dates

fn weekday(n: u32) -> Weekday {
    match n {
        1 => Weekday::Mon,
        2 => Weekday::Tue,
        3 => Weekday::Wed,
        4 => Weekday::Thu,
        5 => Weekday::Fri,
        6 => Weekday::Sat,
        _ => Weekday::Sun,
    }
}

fn last_day(year: i32, month: u32) -> NaiveDate {
    let (y, m) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(y, m, 1).expect("valid date") - Duration::days(1)
}

/// Start and end dates of an observance in a year.
pub(crate) fn observance_dates(o: &Observance, year: i32) -> Option<(NaiveDate, NaiveDate)> {
    let first = NaiveDate::from_ymd_opt(year, o.month, 1)?;
    match o.kind.as_str() {
        "month" => Some((first, last_day(year, o.month))),
        "day" => {
            let d = NaiveDate::from_ymd_opt(year, o.month, o.day?)?;
            Some((d, d))
        }
        // A full week runs Sunday to Saturday.
        "first_full_week" => {
            let off = (7 - first.weekday().num_days_from_sunday()) % 7;
            let s = first + Duration::days(i64::from(off));
            Some((s, s + Duration::days(6)))
        }
        "last_full_week" => {
            let last = last_day(year, o.month);
            let off = (last.weekday().num_days_from_sunday() + 1) % 7;
            let e = last - Duration::days(i64::from(off));
            Some((e - Duration::days(6), e))
        }
        "nth_weekday" => {
            let wd = weekday(o.weekday?);
            let d = NaiveDate::from_weekday_of_month_opt(year, o.month, wd, o.nth? as u8)?;
            Some((d, d))
        }
        _ => None,
    }
}

// ---------------------------------------------------------------- checks

static HASHTAG: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"#([\p{L}\p{N}_]+)").expect("valid regex"));
static URL: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?i)\b(?:https?://|www\.)\S+").expect("valid regex"));

fn is_emoji(c: char) -> bool {
    matches!(c as u32, 0x1F300..=0x1FAFF | 0x2600..=0x27BF | 0x1F000..=0x1F2FF)
}

fn platform_by_id(id: &str) -> Option<&'static Platform> {
    let k = id.trim().to_lowercase();
    let k = match k.as_str() {
        "twitter" => "x",
        "fb" => "facebook",
        "ig" => "instagram",
        other => other,
    }
    .to_string();
    CAMPAIGN.platforms.iter().find(|p| p.id == k)
}

/// Check one post.
fn check_post(
    i: usize,
    p: &PostDraft,
    limits: &BTreeMap<String, usize>,
    target: f64,
) -> Result<PostCheck> {
    let plat = platform_by_id(&p.platform).ok_or_else(|| {
        PhnError::InvalidInput(format!(
            "post {i}: platform '{}' is not recognized; use {}",
            p.platform,
            CAMPAIGN
                .platforms
                .iter()
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    })?;
    let limit = limits.get(&plat.id).copied().unwrap_or(plat.char_limit);
    let mut chars = p.text.chars().count();
    if let Some(n) = plat.link_counts_as {
        for m in URL.find_iter(&p.text) {
            chars = chars - m.as_str().chars().count() + n;
        }
    }
    let mut problems = Vec::new();
    if chars > limit {
        problems.push(format!(
            "{} characters; {} allows {limit}",
            chars, plat.name
        ));
    }
    let tags: Vec<&str> = HASHTAG
        .captures_iter(&p.text)
        .map(|c| c.get(1).map_or("", |m| m.as_str()))
        .collect();
    for t in &tags {
        let letters: Vec<char> = t.chars().filter(|c| c.is_alphabetic()).collect();
        if letters.len() > 7 && letters.iter().all(|c| c.is_lowercase()) {
            problems.push(format!(
                "hashtag #{t}: capitalize each word (CamelCase) for screen readers"
            ));
        }
    }
    if let Some(n) = plat.hashtag_limit
        && tags.len() > n
    {
        problems.push(format!("{} hashtags; {} allows {n}", tags.len(), plat.name));
    }
    if tags.len() > 3 {
        problems.push(format!("{} hashtags: use few, at the end", tags.len()));
    }
    let emoji = p.text.chars().filter(|c| is_emoji(*c)).count();
    if emoji > 3 {
        problems.push(format!(
            "{emoji} emoji: screen readers read each one aloud; use few"
        ));
    }
    let caps: Vec<&str> = p
        .text
        .split_whitespace()
        .filter(|w| {
            let l: Vec<char> = w.chars().filter(|c| c.is_alphabetic()).collect();
            l.len() >= 5 && l.iter().all(|c| c.is_uppercase())
        })
        .collect();
    if !caps.is_empty() {
        problems.push(format!("{} word(s) in all capitals", caps.len()));
    }
    match p
        .alt_text
        .as_deref()
        .map(str::trim)
        .filter(|a| !a.is_empty())
    {
        None if p.has_image => problems.push("the image has no alt text (WCAG 1.1.1)".into()),
        Some(a) => {
            let al = a.to_lowercase();
            if al.starts_with("image of")
                || al.starts_with("picture of")
                || al.starts_with("photo of")
            {
                problems.push(
                    "alt text starts with 'image of' or similar; describe the content directly"
                        .into(),
                );
            }
            if let Some(n) = plat.alt_text_limit
                && a.chars().count() > n
            {
                problems.push(format!(
                    "alt text is {} characters; {} allows {n}",
                    a.chars().count(),
                    plat.name
                ));
            }
        }
        None => {}
    }
    // Readability on the text without hashtags and links.
    let plain = URL
        .replace_all(&HASHTAG.replace_all(&p.text, ""), "")
        .to_string();
    let a = analyze(
        &plain,
        &AnalyzeOptions {
            target_grade: target,
            max_flags: 5,
            redact_excerpts: false,
        },
    );
    if a.counts.words >= 12 && !a.grade.meets_target {
        problems.push(format!(
            "reading grade {:.1} is above the target {target}",
            a.grade.median_grade
        ));
    }
    let jargon: Vec<String> = a
        .flags
        .jargon
        .iter()
        .filter(|j| !j.check_context)
        .map(|j| j.term.clone())
        .take(3)
        .collect();
    if !jargon.is_empty() {
        problems.push(format!("jargon: {}", jargon.join(", ")));
    }
    let lower = p.text.to_lowercase();
    if lower.contains("suicid")
        || lower.contains("took their own life")
        || lower.contains("took his own life")
        || lower.contains("took her own life")
    {
        for f in safe_messaging::check(&p.text).findings {
            problems.push(format!("safe messaging: {} {}", f.message, f.fix));
        }
    }
    Ok(PostCheck {
        post: i,
        platform: plat.id.clone(),
        characters: chars,
        limit,
        within_limit: chars <= limit,
        reading_grade: a.grade.median_grade,
        problems,
    })
}

fn parse_date(s: &str, what: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
        .map_err(|_| PhnError::InvalidInput(format!("{what} '{s}' is not a date (YYYY-MM-DD)")))
}

/// Plan a social media campaign and check draft posts.
pub fn create_social_media_campaign(
    ctx: &Context,
    args: SocialCampaignArgs,
) -> Result<Envelope<SocialCampaignResult>> {
    let c = &*CAMPAIGN;
    let topic = args.topic.trim().to_string();
    if topic.is_empty() {
        return Err(PhnError::InvalidInput("give the campaign topic".into()));
    }
    let start = match args.start_date.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(s) => parse_date(s, "start_date")?,
        None => chrono::Local::now().date_naive(),
    };
    let weeks = args.weeks.unwrap_or(4).clamp(1, 26);
    let per_week = args.posts_per_week.unwrap_or(2).clamp(1, 7);
    let end = start + Duration::days(i64::from(weeks) * 7 - 1);
    let target = args.target_grade.unwrap_or_else(default_target_grade);
    let mut platforms: Vec<&Platform> = Vec::new();
    let names = if args.platforms.is_empty() {
        vec!["facebook".to_string(), "instagram".into(), "x".into()]
    } else {
        args.platforms.clone()
    };
    for p in &names {
        let pl = platform_by_id(p).ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "platform '{p}' is not recognized; use {}",
                c.platforms
                    .iter()
                    .map(|p| p.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?;
        if !platforms.iter().any(|x| x.id == pl.id) {
            platforms.push(pl);
        }
    }
    let mut limits: BTreeMap<String, usize> = c
        .platforms
        .iter()
        .map(|p| (p.id.clone(), p.char_limit))
        .collect();
    for (k, v) in &args.char_limits {
        let pl = platform_by_id(k).ok_or_else(|| {
            PhnError::InvalidInput(format!("char_limits: platform '{k}' is not recognized"))
        })?;
        limits.insert(pl.id.clone(), *v);
    }

    // ---- observances in the window
    let topic_l = topic.to_lowercase();
    let mut sources = Vec::new();
    let mut observances: Vec<ObservanceMatch> = Vec::new();
    if let Some(path) = args
        .observances_path
        .as_deref()
        .filter(|p| !p.trim().is_empty())
    {
        let (t, label) = crate::core::files::load_table(ctx, path, None)?;
        let col = |n: &str| {
            t.headers
                .iter()
                .position(|h| h.trim().eq_ignore_ascii_case(n))
        };
        let (Some(nc), Some(sc)) = (col("name"), col("start")) else {
            return Err(PhnError::InvalidInput(format!(
                "{label} needs columns name and start (and optionally end)"
            )));
        };
        let ec = col("end");
        for r in &t.rows {
            let name = r.get(nc).cloned().unwrap_or_default();
            let Ok(s) = parse_date(r.get(sc).map(String::as_str).unwrap_or_default(), "start")
            else {
                continue;
            };
            let e = ec
                .and_then(|i| r.get(i))
                .and_then(|v| parse_date(v, "end").ok())
                .unwrap_or(s);
            if e < start || s > end {
                continue;
            }
            observances.push(ObservanceMatch {
                related_to_topic: name
                    .to_lowercase()
                    .split_whitespace()
                    .any(|w| w.len() > 3 && topic_l.contains(w)),
                name,
                start: s.to_string(),
                end: e.to_string(),
                source: label.clone(),
            });
        }
        sources.push(phn_core::Source::local_file(&label));
    } else {
        for o in &c.observances {
            for y in [start.year(), end.year()] {
                if let Some((s, e)) = observance_dates(o, y)
                    && e >= start
                    && s <= end
                    && !observances
                        .iter()
                        .any(|x| x.name == o.name && x.start == s.to_string())
                {
                    observances.push(ObservanceMatch {
                        name: o.name.clone(),
                        start: s.to_string(),
                        end: e.to_string(),
                        related_to_topic: o.keywords.iter().any(|k| topic_l.contains(k.as_str()))
                            || topic_l
                                .split_whitespace()
                                .any(|w| w.len() > 3 && o.keywords.iter().any(|k| k.contains(w))),
                        source: "bundled list (unverified; confirm with the sponsor)".into(),
                    });
                }
            }
        }
    }
    observances.sort_by(|a, b| {
        b.related_to_topic
            .cmp(&a.related_to_topic)
            .then(a.start.cmp(&b.start))
    });

    // ---- calendar: spread posts across the week (Tue/Thu first), on related observance days when possible
    let order = [
        Weekday::Tue,
        Weekday::Thu,
        Weekday::Mon,
        Weekday::Wed,
        Weekday::Fri,
        Weekday::Sat,
        Weekday::Sun,
    ];
    let days: Vec<Weekday> = order.iter().take(per_week as usize).copied().collect();
    let mut calendar = Vec::new();
    let mut d = start;
    while d <= end {
        if days.contains(&d.weekday()) {
            let obs = observances
                .iter()
                .filter(|o| o.related_to_topic)
                .find(|o| {
                    o.start.as_str() <= d.to_string().as_str()
                        && d.to_string().as_str() <= o.end.as_str()
                })
                .map(|o| o.name.clone());
            for p in &platforms {
                calendar.push(CalendarSlot {
                    date: d.to_string(),
                    platform: p.id.clone(),
                    observance: obs.clone(),
                });
            }
        }
        d += Duration::days(1);
    }
    // Add single-day related observances that fall outside the regular posting days.
    for o in observances
        .iter()
        .filter(|o| o.related_to_topic && o.start == o.end)
    {
        if !calendar.iter().any(|s| s.date == o.start) {
            for p in &platforms {
                calendar.push(CalendarSlot {
                    date: o.start.clone(),
                    platform: p.id.clone(),
                    observance: Some(o.name.clone()),
                });
            }
        }
    }
    calendar.sort_by(|a, b| a.date.cmp(&b.date).then(a.platform.cmp(&b.platform)));

    // ---- post checks
    let post_checks: Vec<PostCheck> = args
        .posts
        .iter()
        .enumerate()
        .map(|(i, p)| check_post(i + 1, p, &limits, target))
        .collect::<Result<_>>()?;

    // ---- export
    let mut privacy = Privacy::none();
    let mut export_file = None;
    if let Some(fmt) = args
        .export
        .as_deref()
        .map(|f| f.trim().to_lowercase())
        .filter(|f| !f.is_empty() && f != "none")
    {
        if fmt != "xlsx" {
            return Err(PhnError::InvalidInput("export must be \"xlsx\"".into()));
        }
        let mut rows: Vec<Vec<String>> = calendar
            .iter()
            .map(|s| {
                vec![
                    s.date.clone(),
                    s.platform.clone(),
                    s.observance.clone().unwrap_or_default(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                ]
            })
            .collect();
        for (i, p) in args.posts.iter().enumerate() {
            let chk = &post_checks[i];
            let row = vec![
                p.date.clone().unwrap_or_default(),
                chk.platform.clone(),
                String::new(),
                p.text.clone(),
                p.alt_text.clone().unwrap_or_default(),
                format!("{}/{}", chk.characters, chk.limit),
                chk.problems.join("; "),
            ];
            match rows
                .iter_mut()
                .find(|r| r[0] == row[0] && r[1] == row[1] && r[3].is_empty())
            {
                Some(r) => {
                    r[3] = row[3].clone();
                    r[4] = row[4].clone();
                    r[5] = row[5].clone();
                    r[6] = row[6].clone();
                }
                None => rows.push(row),
            }
        }
        rows.sort_by(|a, b| a[0].cmp(&b[0]).then(a[1].cmp(&b[1])));
        let doc = Document {
            title: format!("Social media calendar: {topic}"),
            subtitle: Some(format!("{start} to {end}")),
            language: "en-US".into(),
            author: None,
            letterhead: ctx.settings().letterhead,
            logo: None,
            blocks: vec![
                Block::Table(TableBlock {
                    caption: Some("Calendar".into()),
                    headers: [
                        "Date",
                        "Platform",
                        "Observance",
                        "Post text",
                        "Alt text",
                        "Characters / limit",
                        "Issues",
                    ]
                    .map(String::from)
                    .to_vec(),
                    rows,
                }),
                Block::Table(TableBlock {
                    caption: Some("Observances".into()),
                    headers: ["Observance", "Start", "End", "Related to topic", "Source"]
                        .map(String::from)
                        .to_vec(),
                    rows: observances
                        .iter()
                        .map(|o| {
                            vec![
                                o.name.clone(),
                                o.start.clone(),
                                o.end.clone(),
                                if o.related_to_topic {
                                    "yes".into()
                                } else {
                                    "no".into()
                                },
                                o.source.clone(),
                            ]
                        })
                        .collect(),
                }),
            ],
        };
        let bytes = phn_docs::xlsx::to_xlsx(&doc)?;
        let sb = ctx.sandbox()?;
        let path = sb.new_output_file("social_media_calendar.xlsx")?;
        std::fs::write(&path, bytes)?;
        let disp = sb.display_path(&path);
        privacy.files_written.push(disp.clone());
        export_file = Some(disp);
    }

    // ---- summary
    let mut summary = format!(
        "**Social media campaign: {topic}** ({start} to {end}; {} post(s) a week on {})\n\n",
        per_week,
        platforms
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let related: Vec<&ObservanceMatch> =
        observances.iter().filter(|o| o.related_to_topic).collect();
    if related.is_empty() {
        summary.push_str("No related observance falls in this window.");
    } else {
        summary.push_str(&format!(
            "Related observances: {}.",
            related
                .iter()
                .map(|o| if o.start == o.end {
                    format!("{} ({})", o.name, o.start)
                } else {
                    format!("{} ({} to {})", o.name, o.start, o.end)
                })
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    summary.push_str(&format!(" {} posting slot(s) planned.\n\n", calendar.len()));
    summary.push_str(&format!(
        "**Limits used** (unverified; override with char_limits): {}\n\n",
        platforms
            .iter()
            .map(|p| format!(
                "{} {} characters",
                p.name,
                limits.get(&p.id).copied().unwrap_or(p.char_limit)
            ))
            .collect::<Vec<_>>()
            .join("; ")
    ));
    if !post_checks.is_empty() {
        let rows: Vec<Vec<String>> = post_checks
            .iter()
            .map(|p| {
                vec![
                    p.post.to_string(),
                    p.platform.clone(),
                    format!("{}/{}", p.characters, p.limit),
                    format!("{:.1}", p.reading_grade),
                    if p.problems.is_empty() {
                        "none".into()
                    } else {
                        p.problems.join("; ")
                    },
                ]
            })
            .collect();
        summary.push_str(&md::table(
            &["Post", "Platform", "Characters", "Grade", "Problems"],
            &rows,
        ));
        summary.push('\n');
    }
    summary.push_str("**Accessibility for every post:** ");
    summary.push_str(
        &c.post_rules
            .iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>()
            .join(" "),
    );
    summary.push('\n');
    if let Some(f) = &export_file {
        summary.push_str(&format!("\nCalendar written to `{f}`.\n"));
    }

    sources.extend(content::cite(&c.sources, &["wcag"], &c.meta.prepared));
    let mut caveats = vec![
        "Platform character, hashtag, and alt-text limits could not be checked against the platforms' own help pages from this environment; confirm them, and override with char_limits if they differ.".to_string(),
        "The bundled observance dates could not be verified (ODPHP's National Health Observances pages were not available); confirm each with its sponsor, or give your own calendar with observances_path.".to_string(),
    ];
    if post_checks.iter().any(|p| !p.problems.is_empty()) {
        caveats.push("Fix the listed problems and run the check again before scheduling.".into());
    }
    caveats.extend(content::review_caveats(&[CAMPAIGNS]));
    Ok(Envelope::new(
        summary,
        SocialCampaignResult {
            topic,
            start: start.to_string(),
            end: end.to_string(),
            platforms: platforms.iter().map(|p| p.id.clone()).collect(),
            observances,
            calendar,
            post_checks,
            limits: platforms
                .iter()
                .map(|p| {
                    (
                        p.id.clone(),
                        limits.get(&p.id).copied().unwrap_or(p.char_limit),
                    )
                })
                .collect(),
            accessibility_rules: c.post_rules.iter().map(|r| r.text.clone()).collect(),
            export_file,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(privacy))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn observance_rules_compute_dates() {
        let find = |n: &str| CAMPAIGN.observances.iter().find(|o| o.name == n).unwrap();
        // Third Thursday of November 2026 is November 19.
        assert_eq!(
            observance_dates(find("Great American Smokeout"), 2026),
            Some((d("2026-11-19"), d("2026-11-19")))
        );
        // October 2026: last full Sunday-Saturday week is October 25-31.
        assert_eq!(
            observance_dates(find("National Lead Poisoning Prevention Week"), 2026),
            Some((d("2026-10-25"), d("2026-10-31")))
        );
        // December 2026 begins on a Tuesday: first full week is December 6-12.
        assert_eq!(
            observance_dates(find("National Influenza Vaccination Week"), 2026),
            Some((d("2026-12-06"), d("2026-12-12")))
        );
        assert_eq!(
            observance_dates(find("American Heart Month"), 2028)
                .unwrap()
                .1,
            d("2028-02-29")
        );
    }

    #[test]
    fn post_checks_find_length_hashtag_alt_text_and_caps_problems() {
        let limits: BTreeMap<String, usize> = CAMPAIGN
            .platforms
            .iter()
            .map(|p| (p.id.clone(), p.char_limit))
            .collect();
        let long = "Get your flu shot. ".repeat(20);
        let p = PostDraft {
            platform: "x".into(),
            text: format!("{long} #flushotssavelives FREE"),
            has_image: true,
            ..Default::default()
        };
        let c = check_post(1, &p, &limits, 8.0).unwrap();
        assert!(!c.within_limit);
        let all = c.problems.join(" | ");
        assert!(all.contains("CamelCase"), "{all}");
        assert!(all.contains("no alt text"), "{all}");
        let ok = PostDraft {
            platform: "facebook".into(),
            text: "Flu shots are free at the town hall on Tuesday. #FluShots".into(),
            has_image: true,
            alt_text: Some("A nurse gives a flu shot to a smiling older man.".into()),
            ..Default::default()
        };
        let c = check_post(2, &ok, &limits, 8.0).unwrap();
        assert!(c.problems.is_empty(), "{:?}", c.problems);
        // Links count as 23 characters on X.
        let link = PostDraft {
            platform: "x".into(),
            text: format!("Learn more: https://www.mass.gov/{}", "a".repeat(200)),
            ..Default::default()
        };
        assert_eq!(
            check_post(3, &link, &limits, 8.0).unwrap().characters,
            "Learn more: ".len() + 23
        );
    }
}
