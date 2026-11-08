//! EQ-05 `screen_health_related_social_needs`: scores the CMS Accountable
//! Health Communities (AHC) Health-Related Social Needs screening tool (10
//! core and 16 supplemental items) by domain and maps positive domains to
//! Massachusetts referral categories. With a local file of many screenings it
//! writes per-person results to `/data/output` and returns suppressed counts.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::core::practice::{self, CitedNote, CitedText, ContentSource};
use crate::md;

/// Content id.
pub const AHC_CONTENT: &str = "ahc-hrsn-screening";

#[derive(Debug, Clone, Deserialize)]
struct DomainInfo {
    id: String,
    label: String,
    core: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq)]
pub struct ItemOption {
    pub key: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub positive: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
struct Item {
    id: String,
    number: String,
    domain: String,
    topic: String,
    item_source: String,
    reuse: String,
    wording: Option<String>,
    multi: bool,
    #[serde(default)]
    min_age: Option<u32>,
    options: Vec<ItemOption>,
}

#[derive(Debug, Clone, Deserialize)]
struct ActivityRule {
    under_age: u32,
    youth_max_age: u32,
    youth_min_minutes_per_day: u32,
    adult_min_minutes_per_week: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct Rules {
    safety_threshold: i64,
    phq2_threshold: i64,
    physical_activity: ActivityRule,
    notes: Vec<CitedText>,
}

#[derive(Debug, Clone, Deserialize)]
struct AhcContent {
    meta: ContentMeta,
    sources: BTreeMap<String, ContentSource>,
    domains: Vec<DomainInfo>,
    items: Vec<Item>,
    rules: Rules,
    referrals: BTreeMap<String, Vec<CitedText>>,
}

static CONTENT: LazyLock<AhcContent> = LazyLock::new(|| {
    practice::load(AHC_CONTENT).expect("AHC HRSN content matches the Rust types")
});

fn content() -> &'static AhcContent {
    &CONTENT
}

/// Arguments for `screen_health_related_social_needs`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct HrsnArgs {
    /// One person's answers, keyed by item id (living_situation, housing_problems,
    /// food_worry, food_didnt_last, transportation, utilities, physically_hurt,
    /// insult, threaten, scream, financial_strain, employment, daily_help, lonely,
    /// other_language, school_training, exercise_days, exercise_minutes, alcohol,
    /// tobacco, prescription_misuse, illegal_drugs, phq2_interest, phq2_down,
    /// stress, cognition, errands) or question number ("1"-"26", "23a", "23b").
    /// Values are option keys or labels (e.g. "sometimes_true", "Often true"),
    /// numbers for items 7-10, 17, 18, 23a/b, or a list for item 2.
    pub answers: BTreeMap<String, serde_json::Value>,
    /// The person's age in years (sets the physical activity threshold and the
    /// age limits for items 25-26).
    pub age_years: Option<u32>,
    /// A CSV/XLSX file in /data with one screening per row (columns named by
    /// item id or q1-q26, plus optional Age). Per-person results go to
    /// /data/output; only suppressed counts are returned.
    pub path: Option<String>,
    /// Sheet name for an XLSX file.
    pub sheet: Option<String>,
    /// Optional category column for counts by group (site, month, program;
    /// at most 20 values; identifier columns are refused).
    pub group_by: Option<String>,
    /// Return the items, answer keys, and reuse terms instead of scoring.
    pub list_items: bool,
}

/// Screen result for a domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DomainStatus {
    /// An answer CMS marks as a possible unmet need (or a score at the threshold).
    Positive,
    Negative,
    /// Not enough answers to decide.
    Incomplete,
    /// Not asked (supplemental item skipped) or not applicable at this age.
    NotScreened,
}

impl DomainStatus {
    fn label(self) -> &'static str {
        match self {
            DomainStatus::Positive => "positive",
            DomainStatus::Negative => "negative",
            DomainStatus::Incomplete => "incomplete",
            DomainStatus::NotScreened => "not screened",
        }
    }
}

/// Result for one domain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DomainResult {
    pub domain: String,
    pub label: String,
    /// Core (10-item) domain or supplemental.
    pub core: bool,
    pub status: DomainStatus,
    /// Why (scores and which items were positive, by item number).
    pub reason: String,
    /// Referral categories for positive domains, with Massachusetts resources.
    pub referrals: Vec<CitedNote>,
}

/// An item as listed by `list_items`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ItemInfo {
    pub id: String,
    pub number: String,
    pub domain: String,
    pub topic: String,
    /// Exact wording, only where reuse is clearly free.
    pub wording: Option<String>,
    pub item_source: String,
    pub reuse: String,
    pub options: Vec<ItemOption>,
}

/// Counts for one domain in batch mode ("*" = suppressed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DomainCounts {
    pub domain: String,
    /// Group value, or "All".
    pub group: String,
    pub counts: BTreeMap<String, String>,
}

/// Batch summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HrsnBatch {
    pub file: String,
    pub screenings: usize,
    pub items_recognized: Vec<String>,
    pub unrecognized_answers: usize,
    pub counts: Vec<DomainCounts>,
    pub output_file: String,
    pub suppression_threshold: u32,
}

/// Result of `screen_health_related_social_needs`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HrsnResult {
    pub domains: Vec<DomainResult>,
    pub positive_domains: Vec<String>,
    pub items: Vec<ItemInfo>,
    pub batch: Option<HrsnBatch>,
    pub scoring_notes: Vec<CitedNote>,
}

/// A normalized answer.
#[derive(Debug, Clone, PartialEq)]
enum Ans {
    Keys(Vec<String>),
    Num(f64),
}

fn norm(s: &str) -> String {
    phn_core::text::normalize(s).replace([' ', '-'], "_")
}

fn find_item(key: &str) -> Option<&'static Item> {
    let c = content();
    let k = norm(key);
    let k = k.trim_start_matches("question_").trim_start_matches('q');
    c.items
        .iter()
        .find(|i| norm(&i.id) == norm(key) || i.number == k || norm(&i.number) == k)
}

/// Map a raw value to an answer for an item; `None` when it cannot be read.
fn parse_value(item: &Item, v: &serde_json::Value) -> Option<Ans> {
    use serde_json::Value;
    match v {
        Value::Null => None,
        Value::Bool(b) => parse_text(item, if *b { "yes" } else { "no" }),
        Value::Number(n) => {
            let x = n.as_f64()?;
            if item.options.is_empty() {
                return Some(Ans::Num(x));
            }
            item.options
                .iter()
                .find(|o| o.value == Some(x as i64) && (x.fract() == 0.0))
                .map(|o| Ans::Keys(vec![o.key.clone()]))
        }
        Value::String(s) => parse_text(item, s),
        Value::Array(a) => {
            let mut keys = Vec::new();
            for x in a {
                match parse_value(item, x)? {
                    Ans::Keys(k) => keys.extend(k),
                    Ans::Num(_) => return None,
                }
            }
            Some(Ans::Keys(keys))
        }
        Value::Object(_) => None,
    }
}

fn parse_text(item: &Item, s: &str) -> Option<Ans> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    if item.options.is_empty() {
        // Numeric items (exercise days and minutes): "150 or greater" -> 150.
        let digits: String = t
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        return digits.parse::<f64>().ok().map(Ans::Num);
    }
    let parts: Vec<&str> = if item.multi {
        t.split([';', ',', '|']).collect()
    } else {
        vec![t]
    };
    let mut keys = Vec::new();
    for p in parts {
        let n = norm(p);
        if n.is_empty() {
            continue;
        }
        let exact = item.options.iter().find(|o| {
            norm(&o.key) == n
                || norm(&o.label) == n
                || o.value.is_some_and(|v| n == v.to_string() || n.starts_with(&format!("{v}_")))
                || (n.len() >= 4 && norm(&o.label).starts_with(&n))
        });
        // Otherwise the option whose key words all appear in the answer, most words first
        // ("I do not have a steady place" -> not_steady rather than steady).
        let words: Vec<&str> = n.split('_').collect();
        let hit = exact.or_else(|| {
            item.options
                .iter()
                .filter(|o| {
                    let kw: Vec<&str> = o.key.split('_').collect();
                    kw.iter().all(|w| words.contains(w))
                })
                .max_by_key(|o| o.key.split('_').count())
        });
        match hit {
            Some(o) => keys.push(o.key.clone()),
            None if matches!(n.as_str(), "y" | "true" | "1") => keys.push("yes".into()),
            None if matches!(n.as_str(), "n" | "false" | "0") => keys.push("no".into()),
            None => return None,
        }
    }
    keys.retain(|k| item.options.iter().any(|o| &o.key == k));
    (!keys.is_empty()).then_some(Ans::Keys(keys))
}

fn option<'a>(item: &'a Item, key: &str) -> Option<&'a ItemOption> {
    item.options.iter().find(|o| o.key == key)
}

/// Score one person's answers (by item id).
fn score(answers: &BTreeMap<String, Ans>, age: Option<u32>) -> Vec<DomainResult> {
    let c = content();
    let mut out = Vec::new();
    for d in &c.domains {
        let items: Vec<&Item> = c.items.iter().filter(|i| i.domain == d.id).collect();
        let (status, reason) = match d.id.as_str() {
            "safety" => score_sum(&items, answers, c.rules.safety_threshold, "HITS score"),
            "mental_health" => {
                let phq: Vec<&Item> = items
                    .iter()
                    .copied()
                    .filter(|i| i.id.starts_with("phq2"))
                    .collect();
                let (s1, r1) = score_sum(&phq, answers, c.rules.phq2_threshold, "PHQ-2 score");
                let stress: Vec<&Item> =
                    items.iter().copied().filter(|i| i.id == "stress").collect();
                let (s2, r2) = score_options(&stress, answers, age);
                combine(&[(s1, r1), (s2, r2)])
            }
            "physical_activity" => score_activity(answers, age, &c.rules.physical_activity),
            _ => score_options(&items, answers, age),
        };
        let referrals = if status == DomainStatus::Positive {
            c.referrals
                .get(&d.id)
                .map(|v| practice::notes(v, &c.sources))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        out.push(DomainResult {
            domain: d.id.clone(),
            label: d.label.clone(),
            core: d.core,
            status,
            reason,
            referrals,
        });
    }
    out
}

fn combine(parts: &[(DomainStatus, String)]) -> (DomainStatus, String) {
    let status = if parts.iter().any(|(s, _)| *s == DomainStatus::Positive) {
        DomainStatus::Positive
    } else if parts.iter().any(|(s, _)| *s == DomainStatus::Incomplete) {
        DomainStatus::Incomplete
    } else if parts.iter().any(|(s, _)| *s == DomainStatus::Negative) {
        DomainStatus::Negative
    } else {
        DomainStatus::NotScreened
    };
    let reason = parts
        .iter()
        .map(|(_, r)| r.as_str())
        .filter(|r| !r.is_empty())
        .collect::<Vec<_>>()
        .join("; ");
    (status, reason)
}

fn score_options(
    items: &[&Item],
    answers: &BTreeMap<String, Ans>,
    age: Option<u32>,
) -> (DomainStatus, String) {
    let mut positive = Vec::new();
    let mut answered = 0;
    let mut applicable = 0;
    for i in items {
        if let (Some(min), Some(a)) = (i.min_age, age)
            && a < min
        {
            continue;
        }
        applicable += 1;
        let Some(Ans::Keys(keys)) = answers.get(&i.id) else {
            continue;
        };
        answered += 1;
        if keys
            .iter()
            .any(|k| option(i, k).and_then(|o| o.positive) == Some(true))
        {
            positive.push(format!("item {}", i.number));
        }
    }
    if applicable == 0 {
        return (DomainStatus::NotScreened, "not applicable at this age".into());
    }
    if !positive.is_empty() {
        (
            DomainStatus::Positive,
            format!("possible need on {}", positive.join(", ")),
        )
    } else if answered == 0 {
        (DomainStatus::NotScreened, String::new())
    } else if answered < applicable {
        (
            DomainStatus::Incomplete,
            format!("{answered} of {applicable} item(s) answered, none positive"),
        )
    } else {
        (DomainStatus::Negative, "no possible-need answers".into())
    }
}

fn score_sum(
    items: &[&Item],
    answers: &BTreeMap<String, Ans>,
    threshold: i64,
    name: &str,
) -> (DomainStatus, String) {
    let mut sum = 0i64;
    let mut answered = 0;
    for i in items {
        if let Some(Ans::Keys(keys)) = answers.get(&i.id)
            && let Some(v) = keys.first().and_then(|k| option(i, k)).and_then(|o| o.value)
        {
            sum += v;
            answered += 1;
        }
    }
    if answered == 0 {
        return (DomainStatus::NotScreened, String::new());
    }
    // The largest score the unanswered items could add.
    let max_left: i64 = items
        .iter()
        .filter(|i| !answers.contains_key(&i.id))
        .map(|i| i.options.iter().filter_map(|o| o.value).max().unwrap_or(0))
        .sum();
    if sum >= threshold {
        (
            DomainStatus::Positive,
            format!("{name} {sum} (threshold {threshold})"),
        )
    } else if answered < items.len() && sum + max_left >= threshold {
        (
            DomainStatus::Incomplete,
            format!(
                "{name} {sum} from {answered} of {} items (threshold {threshold})",
                items.len()
            ),
        )
    } else {
        (
            DomainStatus::Negative,
            format!("{name} {sum} (threshold {threshold})"),
        )
    }
}

fn score_activity(
    answers: &BTreeMap<String, Ans>,
    age: Option<u32>,
    r: &ActivityRule,
) -> (DomainStatus, String) {
    let num = |id: &str| match answers.get(id) {
        Some(Ans::Num(x)) => Some(*x),
        _ => None,
    };
    let (Some(days), Some(mins)) = (num("exercise_days"), num("exercise_minutes")) else {
        return if answers.contains_key("exercise_days") || answers.contains_key("exercise_minutes")
        {
            (
                DomainStatus::Incomplete,
                "both days and minutes are needed".into(),
            )
        } else {
            (DomainStatus::NotScreened, String::new())
        };
    };
    let weekly = days.clamp(0.0, 7.0) * mins.max(0.0);
    match age {
        None => (
            DomainStatus::Incomplete,
            format!("{weekly} minutes a week; give age_years to apply the age threshold"),
        ),
        Some(a) if a < r.under_age => (
            DomainStatus::NotScreened,
            format!("cannot be determined under age {}", r.under_age),
        ),
        Some(a) if a <= r.youth_max_age => {
            let daily = weekly / 7.0;
            if daily < f64::from(r.youth_min_minutes_per_day) {
                (
                    DomainStatus::Positive,
                    format!(
                        "{weekly} minutes a week = {:.0} a day on average (under {} for ages \
                         {}-{})",
                        daily, r.youth_min_minutes_per_day, r.under_age, r.youth_max_age
                    ),
                )
            } else {
                (
                    DomainStatus::Negative,
                    format!("{:.0} minutes a day on average", daily),
                )
            }
        }
        Some(_) => {
            if weekly < f64::from(r.adult_min_minutes_per_week) {
                (
                    DomainStatus::Positive,
                    format!(
                        "{weekly} minutes a week (under {} for adults)",
                        r.adult_min_minutes_per_week
                    ),
                )
            } else {
                (
                    DomainStatus::Negative,
                    format!("{weekly} minutes a week"),
                )
            }
        }
    }
}

fn item_infos() -> Vec<ItemInfo> {
    content()
        .items
        .iter()
        .map(|i| ItemInfo {
            id: i.id.clone(),
            number: i.number.clone(),
            domain: i.domain.clone(),
            topic: i.topic.clone(),
            wording: i.wording.clone(),
            item_source: i.item_source.clone(),
            reuse: i.reuse.clone(),
            options: i.options.clone(),
        })
        .collect()
}

fn base_sources(c: &AhcContent, results: &[DomainResult]) -> Vec<Source> {
    let mut keys: Vec<&str> = vec!["cms-ahc-tool"];
    for d in results.iter().filter(|d| d.status == DomainStatus::Positive) {
        if let Some(v) = c.referrals.get(&d.domain) {
            keys.extend(v.iter().map(|n| n.source.as_str()));
        }
    }
    practice::sources_for(&c.sources, keys)
}

fn base_caveats(c: &AhcContent) -> Vec<String> {
    let mut v: Vec<String> = practice::review_caveat(&c.meta).into_iter().collect();
    v.push(
        "A positive domain means the person might have an unmet health-related social need \
         (CMS); it is a prompt for a conversation and referral, not a diagnosis."
            .into(),
    );
    v.push(
        "Item wording is shown only for items whose reuse is clearly free; use the CMS form \
         (linked in the sources) for the others. CMS's permission from the original authors \
         covers the AHC Model only, and some items (for example HITS, PRAPARE) have their own \
         terms."
            .into(),
    );
    v.push(
        "Resource details (phone numbers, eligibility) change; confirm them before referring. \
         find_community_resources searches local resources and health centers."
            .into(),
    );
    v
}

/// Score the AHC HRSN screening tool.
pub fn screen_health_related_social_needs(
    ctx: &Context,
    args: HrsnArgs,
) -> Result<Envelope<HrsnResult>> {
    let c = content();
    if let Some(path) = args.path.as_deref().filter(|p| !p.trim().is_empty()) {
        return batch(ctx, path, args.sheet.as_deref(), args.group_by.as_deref());
    }
    let scoring_notes = practice::notes(&c.rules.notes, &c.sources);
    if args.list_items || args.answers.is_empty() {
        let items = item_infos();
        let mut s = String::from(
            "**CMS AHC Health-Related Social Needs Screening Tool: items and answer keys**\n\n",
        );
        let rows: Vec<Vec<String>> = items
            .iter()
            .map(|i| {
                vec![
                    i.number.clone(),
                    i.id.clone(),
                    i.domain.replace('_', " "),
                    i.topic.clone(),
                    if i.options.is_empty() {
                        "number".into()
                    } else {
                        i.options
                            .iter()
                            .map(|o| o.key.clone())
                            .collect::<Vec<_>>()
                            .join(", ")
                    },
                ]
            })
            .collect();
        s.push_str(&md::table(&["#", "Item id", "Domain", "Topic", "Answer keys"], &rows));
        s.push_str(
            "\nItems 1-10 are the core screen; 11-26 are supplemental. Give answers by item id \
             (or question number) to score them.\n",
        );
        return Ok(Envelope::new(
            s,
            HrsnResult {
                domains: Vec::new(),
                positive_domains: Vec::new(),
                items,
                batch: None,
                scoring_notes,
            },
        )
        .sources(practice::sources_for(&c.sources, ["cms-ahc-tool"]))
        .caveats(base_caveats(c))
        .privacy(Privacy::none()));
    }

    // ---- One person
    let mut parsed: BTreeMap<String, Ans> = BTreeMap::new();
    for (k, v) in &args.answers {
        let item = find_item(k).ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "'{k}' is not an AHC item; use an item id (living_situation, food_worry, ...) or \
                 a question number 1-26 (23a, 23b). Call with list_items for the keys."
            ))
        })?;
        let a = parse_value(item, v).ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "the answer for item {} ({}) is not one of: {}",
                item.number,
                item.id,
                if item.options.is_empty() {
                    "a number".to_string()
                } else {
                    item.options
                        .iter()
                        .map(|o| o.key.clone())
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            ))
        })?;
        parsed.insert(item.id.clone(), a);
    }
    let results = score(&parsed, args.age_years);
    let positive: Vec<String> = results
        .iter()
        .filter(|d| d.status == DomainStatus::Positive)
        .map(|d| d.domain.clone())
        .collect();
    let mut s = String::from("**AHC health-related social needs screen**\n\n");
    let rows: Vec<Vec<String>> = results
        .iter()
        .filter(|d| d.status != DomainStatus::NotScreened)
        .map(|d| {
            vec![
                d.label.clone(),
                if d.core { "core" } else { "supplemental" }.into(),
                d.status.label().to_string(),
                d.reason.clone(),
            ]
        })
        .collect();
    s.push_str(&md::table(&["Domain", "", "Result", "Why"], &rows));
    if positive.is_empty() {
        s.push_str("\nNo positive domains among the answers given.\n");
    } else {
        s.push_str("\n**Referral categories for positive domains**\n");
        for d in results.iter().filter(|d| d.status == DomainStatus::Positive) {
            s.push_str(&format!("\n_{}_\n", d.label));
            for r in &d.referrals {
                s.push_str(&format!("- {} ({})\n", r.text, r.citation));
            }
        }
    }
    let core_missing: Vec<&str> = results
        .iter()
        .filter(|d| d.core && d.status == DomainStatus::NotScreened)
        .map(|d| d.label.as_str())
        .collect();
    let mut caveats = base_caveats(c);
    if !core_missing.is_empty() {
        caveats.push(format!(
            "Core domains not screened: {}.",
            core_missing.join("; ")
        ));
    }
    if positive.iter().any(|d| d == "safety") {
        caveats.insert(
            0,
            "SAFETY: the interpersonal safety screen is positive. Follow your agency's protocol \
             (private setting, immediate danger, safety planning, mandated reporting) before \
             anything else, and do not leave results where others can see them."
                .into(),
        );
    }
    let sources = base_sources(c, &results);
    Ok(Envelope::new(
        s,
        HrsnResult {
            domains: results,
            positive_domains: positive,
            items: Vec::new(),
            batch: None,
            scoring_notes,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        notes: vec![
            "Only answer codes were used; do not enter the person's name or identifiers.".into(),
        ],
        ..Privacy::none()
    })
    .decision_support())
}

fn batch(
    ctx: &Context,
    path: &str,
    sheet: Option<&str>,
    group_by: Option<&str>,
) -> Result<Envelope<HrsnResult>> {
    let c = content();
    let lt = crate::localdata::load(ctx, path, sheet)?;
    let t = &lt.table;
    // Map columns to items.
    let mut item_cols: Vec<(usize, &Item)> = Vec::new();
    for (i, h) in t.headers.iter().enumerate() {
        if let Some(item) = find_item(h)
            && !item_cols.iter().any(|(_, it)| it.id == item.id)
        {
            item_cols.push((i, item));
        }
    }
    if item_cols.is_empty() {
        return Err(PhnError::InvalidInput(
            "no AHC item columns were recognized; name columns by item id (living_situation, \
             food_worry, ...) or q1-q26 (q23a, q23b). Call with list_items for the ids."
                .into(),
        ));
    }
    let age_c = practice::find_col(t, &["age", "age_years", "age_yrs"]);
    let group_c = match group_by {
        Some(g) => Some(lt.column(g)?),
        None => None,
    };
    let groups: Option<Vec<Option<String>>> = match group_c {
        Some(gc) => Some(lt.categorical(gc)?),
        None => None,
    };
    let mut inputs: Vec<usize> = item_cols.iter().map(|(i, _)| *i).collect();
    inputs.extend(age_c);
    inputs.extend(group_c);
    let id_cols = practice::identifier_columns(t, &lt.findings, &inputs);
    let mut unrecognized = 0usize;
    let mut per_person: Vec<Vec<String>> = Vec::new();
    // (group, domain) -> status -> count
    let mut counts: BTreeMap<(String, String), BTreeMap<DomainStatus, u64>> = BTreeMap::new();
    let mut n = 0usize;
    for (ri, row) in t.rows.iter().enumerate() {
        if row.iter().all(|x| x.trim().is_empty()) {
            continue;
        }
        n += 1;
        let mut answers = BTreeMap::new();
        for (ci, item) in &item_cols {
            let v = row.get(*ci).map(String::as_str).unwrap_or("").trim();
            if v.is_empty() {
                continue;
            }
            match parse_text(item, v) {
                Some(a) => {
                    answers.insert(item.id.clone(), a);
                }
                None => unrecognized += 1,
            }
        }
        let age = age_c
            .and_then(|i| row.get(i))
            .and_then(|s| s.trim().parse::<f64>().ok())
            .map(|a| a.max(0.0) as u32);
        let results = score(&answers, age);
        let group = groups
            .as_ref()
            .and_then(|g| g[ri].clone())
            .unwrap_or_else(|| "(blank)".into());
        let mut line = vec![(ri + 2).to_string()];
        line.extend(id_cols.iter().map(|&i| row.get(i).cloned().unwrap_or_default()));
        for d in &results {
            line.push(d.status.label().to_string());
            for g in [group.clone(), "All".to_string()] {
                if g != "All" && groups.is_none() {
                    continue;
                }
                *counts
                    .entry((g, d.domain.clone()))
                    .or_default()
                    .entry(d.status)
                    .or_default() += 1;
            }
        }
        line.push(
            results
                .iter()
                .filter(|d| d.status == DomainStatus::Positive)
                .map(|d| d.label.clone())
                .collect::<Vec<_>>()
                .join("; "),
        );
        per_person.push(line);
    }
    // Output file.
    let mut headers = vec!["Row".to_string()];
    headers.extend(id_cols.iter().map(|&i| t.headers[i].clone()));
    headers.extend(c.domains.iter().map(|d| d.label.clone()));
    headers.push("Positive domains".into());
    let h: Vec<&str> = headers.iter().map(String::as_str).collect();
    let mut referral_rows = Vec::new();
    for d in &c.domains {
        if let Some(v) = c.referrals.get(&d.id) {
            for r in v {
                referral_rows.push(vec![
                    d.label.clone(),
                    r.text.clone(),
                    practice::cite(&c.sources, &r.source, &r.section),
                ]);
            }
        }
    }
    let doc = practice::document(
        ctx,
        "Health-related social needs screening results",
        vec![
            practice::para(
                "CMS AHC HRSN Screening Tool results by person. A positive domain means the \
                 person might have an unmet need. Decision support only; confirm with the person.",
            ),
            practice::table("Results", &h, per_person),
            practice::table(
                "Referral categories",
                &["Domain", "Resource", "Source"],
                referral_rows,
            ),
        ],
    );
    let written = practice::write_document(ctx, &doc, "hrsn_screening_results", &["xlsx"])?;
    let out = written.first().cloned().unwrap_or_default();

    // Suppressed counts per group and domain.
    let threshold = ctx.suppression_threshold();
    let statuses = [
        DomainStatus::Positive,
        DomainStatus::Negative,
        DomainStatus::Incomplete,
        DomainStatus::NotScreened,
    ];
    let mut count_rows = Vec::new();
    let mut any_suppressed = false;
    let mut group_names: Vec<String> = counts.keys().map(|(g, _)| g.clone()).collect();
    group_names.sort();
    group_names.dedup();
    group_names.retain(|g| g != "All");
    group_names.insert(0, "All".into());
    for g in &group_names {
        for d in &c.domains {
            let Some(m) = counts.get(&(g.clone(), d.id.clone())) else {
                continue;
            };
            let raw: Vec<(String, u64)> = statuses
                .iter()
                .map(|s| (s.label().to_string(), *m.get(s).unwrap_or(&0)))
                .collect();
            let (shown, any) = practice::suppressed_counts(&raw, threshold);
            any_suppressed |= any;
            count_rows.push(DomainCounts {
                domain: d.id.clone(),
                group: g.clone(),
                counts: shown.into_iter().collect(),
            });
        }
    }
    let mut s = format!(
        "**AHC health-related social needs screening — {n} screening(s) in `{}`**\n\nPer-person \
         results were written to `{out}` (not shown here).\n\n",
        lt.label
    );
    let rows: Vec<Vec<String>> = count_rows
        .iter()
        .filter(|r| r.group == "All")
        .map(|r| {
            let label = c
                .domains
                .iter()
                .find(|d| d.id == r.domain)
                .map(|d| d.label.clone())
                .unwrap_or_default();
            vec![
                label,
                r.counts.get("positive").cloned().unwrap_or_default(),
                r.counts.get("negative").cloned().unwrap_or_default(),
                r.counts.get("incomplete").cloned().unwrap_or_default(),
                r.counts.get("not screened").cloned().unwrap_or_default(),
            ]
        })
        .collect();
    s.push_str(&md::table(
        &["Domain", "Positive", "Negative", "Incomplete", "Not screened"],
        &rows,
    ));
    if group_names.len() > 1 {
        s.push_str(&format!(
            "\nCounts by `{}` are in the structured data ({} groups).\n",
            group_by.unwrap_or_default(),
            group_names.len() - 1
        ));
    }
    if any_suppressed {
        s.push_str(&format!(
            "\n`*` = suppressed (counts from 1 to {} and cells that would reveal them).\n",
            threshold - 1
        ));
    }
    let mut caveats = base_caveats(c);
    if unrecognized > 0 {
        caveats.push(format!(
            "{unrecognized} answer cell(s) could not be matched to an answer option and were \
             treated as unanswered (values not shown)."
        ));
    }
    let recognized: Vec<String> = item_cols.iter().map(|(_, i)| i.id.clone()).collect();
    Ok(Envelope::new(
        s,
        HrsnResult {
            domains: Vec::new(),
            positive_domains: Vec::new(),
            items: Vec::new(),
            batch: Some(HrsnBatch {
                file: lt.label.clone(),
                screenings: n,
                items_recognized: recognized,
                unrecognized_answers: unrecognized,
                counts: count_rows,
                output_file: out.clone(),
                suppression_threshold: threshold,
            }),
            scoring_notes: practice::notes(&c.rules.notes, &c.sources),
        },
    )
    .source(lt.source())
    .sources(practice::sources_for(&c.sources, ["cms-ahc-tool"]))
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        suppression_applied: true,
        suppression_threshold: Some(threshold),
        identifiers_detected: lt.identifier_categories(),
        files_written: written,
        notes: vec![
            "Only counts by domain are returned; per-person results are in the output file."
                .into(),
        ],
    })
    .decision_support())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx(with_data: bool) -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        if with_data {
            std::fs::create_dir_all(dir.path().join("data")).unwrap();
        }
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    fn run(answers: serde_json::Value, age: Option<u32>) -> Envelope<HrsnResult> {
        let (_d, c) = ctx(false);
        let answers: BTreeMap<String, serde_json::Value> =
            serde_json::from_value(answers).unwrap();
        screen_health_related_social_needs(
            &c,
            HrsnArgs {
                answers,
                age_years: age,
                ..HrsnArgs::default()
            },
        )
        .unwrap()
    }

    fn status(env: &Envelope<HrsnResult>, d: &str) -> DomainStatus {
        env.data
            .domains
            .iter()
            .find(|x| x.domain == d)
            .unwrap()
            .status
    }

    #[test]
    fn core_domains_follow_the_cms_underlines() {
        let env = run(
            json!({
                "1": "I have a place to live today, but I am worried about losing it",
                "housing_problems": ["mold", "pests"],
                "food_worry": "Sometimes true",
                "food_didnt_last": "never_true",
                "transportation": "no",
                "utilities": "already shut off",
                "7": 1, "8": 2, "9": 1, "10": 2
            }),
            Some(40),
        );
        assert_eq!(status(&env, "living_situation"), DomainStatus::Positive);
        assert_eq!(status(&env, "food"), DomainStatus::Positive);
        assert_eq!(status(&env, "transportation"), DomainStatus::Negative);
        assert_eq!(status(&env, "utilities"), DomainStatus::Positive);
        assert_eq!(status(&env, "safety"), DomainStatus::Negative);
        let food = env.data.domains.iter().find(|d| d.domain == "food").unwrap();
        assert!(food.referrals.iter().any(|r| r.text.contains("SNAP")));
        assert!(env.notice.is_some());
    }

    #[test]
    fn safety_threshold_is_11_and_partial_answers_are_incomplete() {
        let env = run(json!({"7": 3, "8": 3, "9": 2, "10": 3}), None);
        assert_eq!(status(&env, "safety"), DomainStatus::Positive);
        assert!(env.caveats[0].starts_with("SAFETY"));
        let env = run(json!({"7": 2, "8": 2, "9": 2, "10": 2}), None);
        assert_eq!(status(&env, "safety"), DomainStatus::Negative);
        let env = run(json!({"7": "sometimes", "8": "rarely"}), None);
        assert_eq!(status(&env, "safety"), DomainStatus::Incomplete);
    }

    #[test]
    fn phq2_stress_and_physical_activity() {
        let env = run(
            json!({"23a": 1, "23b": 2, "stress": "not at all",
                   "exercise_days": 3, "exercise_minutes": "40"}),
            Some(45),
        );
        assert_eq!(status(&env, "mental_health"), DomainStatus::Positive);
        // 3 x 40 = 120 minutes a week < 150.
        assert_eq!(status(&env, "physical_activity"), DomainStatus::Positive);
        let env = run(json!({"exercise_days": 7, "exercise_minutes": 60}), Some(12));
        assert_eq!(status(&env, "physical_activity"), DomainStatus::Negative);
        let env = run(json!({"exercise_days": 7, "exercise_minutes": 50}), Some(12));
        assert_eq!(status(&env, "physical_activity"), DomainStatus::Positive);
        let env = run(json!({"errands": "yes"}), Some(10));
        assert_eq!(status(&env, "disabilities"), DomainStatus::NotScreened);
    }

    #[test]
    fn bad_answers_are_tool_errors() {
        let (_d, c) = ctx(false);
        let mut answers = BTreeMap::new();
        answers.insert("food_worry".to_string(), json!("maybe"));
        let e = screen_health_related_social_needs(
            &c,
            HrsnArgs {
                answers,
                ..HrsnArgs::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("often_true"));
        let env = screen_health_related_social_needs(&c, HrsnArgs::default()).unwrap();
        assert_eq!(env.data.items.len(), 27);
        assert!(env.data.items.iter().any(|i| i.wording.is_some()));
        assert!(
            env.data
                .items
                .iter()
                .filter(|i| i.item_source.contains("HITS"))
                .all(|i| i.wording.is_none())
        );
    }

    #[test]
    fn batch_returns_suppressed_counts_only() {
        let (dir, c) = ctx(true);
        let mut csv = String::from("Client Name,MRN,Age,Site,q1,q3,q4,q5,q6\n");
        for i in 0..12 {
            csv.push_str(&format!(
                "Quentin Zorbeck{i},MRN77{i:03},{},{},{},{},never true,{},no\n",
                30 + i,
                if i % 2 == 0 { "Clinic 1" } else { "Clinic 2" },
                if i < 6 { "steady" } else { "not steady" },
                if i < 3 { "often true" } else { "never true" },
                if i == 0 { "yes" } else { "no" },
            ));
        }
        std::fs::write(dir.path().join("data/hrsn.csv"), csv).unwrap();
        let env = screen_health_related_social_needs(
            &c,
            HrsnArgs {
                path: Some("hrsn.csv".into()),
                group_by: Some("Site".into()),
                ..HrsnArgs::default()
            },
        )
        .unwrap();
        let b = env.data.batch.as_ref().unwrap();
        assert_eq!(b.screenings, 12);
        let food = b
            .counts
            .iter()
            .find(|r| r.group == "All" && r.domain == "food")
            .unwrap();
        // 3 positive -> suppressed.
        assert_eq!(food.counts["positive"], "*");
        let json = serde_json::to_string(&env).unwrap();
        assert!(!json.contains("Zorbeck") && !json.contains("MRN77"));
        assert!(env.privacy.suppression_applied);
        assert_eq!(env.privacy.files_written.len(), 1);
    }
}
