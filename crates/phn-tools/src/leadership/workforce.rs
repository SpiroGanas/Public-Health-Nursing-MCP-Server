//! LEAD-06 `assess_workforce_competencies`: self-assessments against the domains and tiers of the
//! Council on Linkages Core Competencies for Public Health Professionals (2021) or the
//! Community/Public Health Nursing Competencies (2018), a team summary with a small-group rule
//! (no result for fewer than 3 people; names never appear), a team training plan, and a tracker
//! for the Massachusetts local public health workforce standards (credentials by position).
//!
//! Staff ratings and credentials come from the nurse's own CSV/XLSX files through `localdata`
//! (staff are identified by a code, never a name; name columns are refused). Individual rows go
//! only to the workbook in /data/output; the response carries team figures. The competency
//! statements of both frameworks were not readable from the build network, so ratings are by
//! domain and tier; the nurse may type her own competency labels. Decision support only.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Months, NaiveDate};
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_data::strategy::{self, PerformanceStandards, WorkforcePosition};
use phn_local::phi::{self, Identifier};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::finance_content::{has_word, padded};
use super::workforce_content::{self as wc, CompetencyContent, Framework};
use super::workforce_output;
use crate::context::Context;
use crate::localdata::{self, LocalTable};
use crate::md;

const MAX_STAFF: usize = 500;

// ---------------------------------------------------------------- arguments

/// Which competency framework the ratings use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FrameworkKind {
    /// Council on Linkages Core Competencies for Public Health Professionals, 2021 (default).
    #[default]
    CoreCompetencies2021,
    /// Quad Council Coalition Community/Public Health Nursing Competencies, 2018.
    Cphn2018,
}

impl FrameworkKind {
    fn id(self) -> &'static str {
        match self {
            FrameworkKind::CoreCompetencies2021 => "core_competencies_2021",
            FrameworkKind::Cphn2018 => "cphn_2018",
        }
    }
}

/// Arguments for `assess_workforce_competencies`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct WorkforceArgs {
    /// core_competencies_2021 (default) or cphn_2018.
    pub framework: Option<FrameworkKind>,
    /// CSV or XLSX in /data with one row per staff member, domain, and (optionally) competency:
    /// a staff code, a tier (1, 2, or 3), a domain, and a rating (1 None, 2 Aware, 3 Knowledgeable,
    /// 4 Proficient; blank or 0 = not rated). Staff are identified by a code, not a name: a column
    /// of names is refused. Leave out to write a blank self-assessment workbook and guide.
    pub ratings_path: Option<String>,
    /// Sheet name for an XLSX ratings file (default: the first sheet).
    pub ratings_sheet: Option<String>,
    /// Staff code column (default: Staff Code, Staff ID, Staff).
    pub staff_column: Option<String>,
    /// Tier column (default: Tier).
    pub tier_column: Option<String>,
    /// Domain column: a domain number 1-8, or its name (default: Domain).
    pub domain_column: Option<String>,
    /// Rating column (default: Rating).
    pub rating_column: Option<String>,
    /// Your target rating for every domain, 1 to 4 (default 3, a tool default with no published source).
    pub target_rating: Option<f64>,
    /// CSV or XLSX in /data to track credentials: a staff code, position, hire date, and one row
    /// per credential completed (credential name and completed date). Names are refused.
    pub credentials_path: Option<String>,
    /// Sheet name for an XLSX credentials file.
    pub credentials_sheet: Option<String>,
    /// Staff code column in the credentials file (default: Staff Code, Staff ID, Staff).
    pub cred_staff_column: Option<String>,
    /// Position column (default: Position, Title).
    pub cred_position_column: Option<String>,
    /// Hire date column (default: Hire Date).
    pub cred_hire_date_column: Option<String>,
    /// Credential name column (default: Credential, Training).
    pub cred_credential_column: Option<String>,
    /// Completed date column (default: Completed, Completed Date).
    pub cred_completed_column: Option<String>,
    /// Date to measure deadlines from, YYYY-MM-DD (default today).
    pub as_of_date: Option<String>,
    /// Days before a deadline to call it "due soon" (default 90).
    pub due_soon_days: Option<u32>,
    /// Write the workbook (live formulas) and the DOCX plan to /data/output (default true).
    pub write_documents: Option<bool>,
}

// ---------------------------------------------------------------- results

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScaleLevel {
    pub value: u32,
    pub label: String,
    pub meaning: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DomainResult {
    pub domain_number: u32,
    pub domain_id: String,
    pub domain: String,
    /// Staff who rated the domain (null when suppressed).
    pub raters: Option<u32>,
    pub mean_rating: Option<f64>,
    pub gap_to_target: Option<f64>,
    /// Staff whose domain rating is below the target (null when suppressed or between 1 and 2).
    pub staff_below_target: Option<u32>,
    pub percent_below_target: Option<f64>,
    pub suppressed: bool,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TierResult {
    pub tier: u32,
    pub label: String,
    pub staff: Option<u32>,
    pub mean_rating: Option<f64>,
    pub suppressed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TeamResults {
    pub framework: String,
    /// Number of staff with ratings, or "fewer than 3".
    pub team_size: String,
    pub small_group_rule_applied: bool,
    pub overall_mean_rating: Option<f64>,
    pub overall_basis: String,
    pub domains: Vec<DomainResult>,
    pub tiers: Vec<TierResult>,
    pub suppression_notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanRow {
    pub rank: u32,
    pub domain: String,
    /// High (gap of 1 or more), Medium (0.5 to under 1), Low (under 0.5), or Maintain.
    pub priority: String,
    pub gap_to_target: f64,
    pub suggested_steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Requirement {
    pub label: String,
    /// `required at hire`, `required after hire`, or `recommended`.
    pub kind: String,
    pub deadline_months_from_hire: Option<u32>,
    /// DPH's wording.
    pub dph_wording: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PositionStandard {
    pub position_id: String,
    pub position: String,
    pub requirements: Vec<Requirement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CredentialSummary {
    pub position: String,
    pub requirement: String,
    pub kind: String,
    /// Staff in the position with a hire date (null when any count below is suppressed).
    pub staff: Option<u32>,
    pub complete: Option<u32>,
    /// Overdue or due soon.
    pub needs_attention: Option<u32>,
    pub upcoming: Option<u32>,
    pub suppressed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CredentialResults {
    pub as_of: String,
    pub due_soon_days: u32,
    pub summaries: Vec<CredentialSummary>,
    /// Staff whose position did not match a DPH position (count only; "fewer than 3" when 1 or 2).
    pub staff_position_not_matched: String,
    /// Staff with no usable hire date (count only).
    pub staff_hire_date_missing: String,
    /// Credential entries not matched to a DPH requirement (count; see the workbook).
    pub entries_not_matched: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NotVerifiedOut {
    pub item: String,
    pub detail: String,
}

/// Result of `assess_workforce_competencies`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WorkforceResult {
    /// `template` (no files read) or `results`.
    pub mode: String,
    pub framework: String,
    pub domains: Vec<String>,
    pub tiers: Vec<String>,
    pub rating_scale: Vec<ScaleLevel>,
    pub target_rating: f64,
    pub minimum_group_size: u32,
    pub team: Option<TeamResults>,
    pub training_plan: Vec<PlanRow>,
    pub credentials: Option<CredentialResults>,
    /// DPH's workforce standards by position (public content from the bundled snapshot).
    pub position_standards: Vec<PositionStandard>,
    pub workbook_file: Option<String>,
    pub plan_file: Option<String>,
    pub not_verified: Vec<NotVerifiedOut>,
}

// ---------------------------------------------------------------- rows

/// One staff member's mean rating in one domain.
#[derive(Debug, Clone)]
pub(crate) struct StaffDomain {
    pub staff: String,
    pub tier: Option<u32>,
    pub domain: usize,
    pub rating: f64,
}

/// One credential line for one staff member (tracker row).
#[derive(Debug, Clone)]
pub(crate) struct CredRow {
    pub staff: String,
    pub position: String,
    pub hire: NaiveDate,
    pub requirement: String,
    pub kind: String,
    pub months: Option<u32>,
    pub completed: Option<NaiveDate>,
    pub status: String,
}

// ---------------------------------------------------------------- reading files

fn pick(lt: &LocalTable, explicit: &Option<String>, aliases: &[&str], what: &str) -> Result<usize> {
    if let Some(n) = explicit.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        return lt
            .table
            .column_index(n)
            .ok_or_else(|| PhnError::InvalidInput(format!("'{}' has no column named '{n}'", lt.label)));
    }
    for a in aliases {
        let want = padded(a);
        for (i, h) in lt.table.headers.iter().enumerate() {
            if padded(h) == want {
                return Ok(i);
            }
        }
    }
    Err(PhnError::InvalidInput(format!(
        "no {what} column found; name one with the matching *_column argument. Columns in the file: {}",
        lt.table
            .headers
            .iter()
            .filter(|h| !lt.findings.iter().any(|f| &f.column == *h && f.kind == Identifier::Name))
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    )))
}

/// A column that may be refused for holding identifiers (other than the date kinds).
fn safe_col(lt: &LocalTable, idx: usize) -> Result<usize> {
    lt.column(&lt.table.headers[idx].clone())
}

/// The staff code column: codes are allowed ("ID numbers or codes"), names and contacts are not.
fn staff_col(lt: &LocalTable, explicit: &Option<String>) -> Result<usize> {
    let idx = pick(
        lt,
        explicit,
        &["staff code", "staff id", "staff", "employee code", "employee id", "code", "id"],
        "staff code",
    )?;
    if let Some(f) = lt
        .findings
        .iter()
        .find(|f| f.index == idx && f.kind != Identifier::OtherIdNumber)
    {
        return Err(PhnError::Privacy(format!(
            "column '{}' appears to contain {}; identify staff by a code (such as S01), not a name. Names stay on your computer: keep a roster of codes and names in your own file",
            f.column,
            f.kind.label()
        )));
    }
    for v in lt.table.column(idx) {
        let t = v.trim();
        if t.is_empty() {
            continue;
        }
        if t.chars().count() > 20 {
            return Err(PhnError::Privacy(format!(
                "column '{}' has values longer than a staff code; use short codes such as S01",
                lt.table.headers[idx]
            )));
        }
        if t.split(|c: char| !c.is_alphabetic())
            .any(|w| w.chars().count() > 2 && (phi::is_first_name(w) || phi::is_surname(w)))
            || !phi::scan_text(t).is_empty() && t.chars().any(char::is_whitespace)
        {
            return Err(PhnError::Privacy(format!(
                "column '{}' looks like it holds names; identify staff by a code (such as S01)",
                lt.table.headers[idx]
            )));
        }
    }
    Ok(idx)
}

fn find_domain(f: &Framework, cell: &str) -> Option<usize> {
    let t = cell.trim();
    if let Ok(n) = t.parse::<usize>() {
        return (1..=f.domains.len()).contains(&n).then(|| n - 1);
    }
    let up = t.to_ascii_uppercase();
    if let Some(i) = f.domains.iter().position(|d| d.id.eq_ignore_ascii_case(t)) {
        return Some(i);
    }
    if let Some(n) = up.strip_prefix('D').and_then(|r| r.trim().parse::<usize>().ok()) {
        return (1..=f.domains.len()).contains(&n).then(|| n - 1);
    }
    let p = padded(t);
    if let Some(i) = f.domains.iter().position(|d| padded(&d.name) == p) {
        return Some(i);
    }
    // Aliases as whole words; the longest alias wins, a tie between domains is no match.
    let mut best: Option<(usize, usize, bool)> = None;
    for (i, d) in f.domains.iter().enumerate() {
        for a in &d.aliases {
            if has_word(t, a) {
                let len = padded(a).len();
                match best {
                    Some((_, l, _)) if l > len => {}
                    Some((j, l, _)) if l == len && j != i => best = Some((j, l, true)),
                    _ => best = Some((i, len, false)),
                }
            }
        }
    }
    best.filter(|(_, _, tie)| !tie).map(|(i, _, _)| i)
}

fn parse_tier(cell: &str) -> Option<u32> {
    let p = padded(cell);
    let digits: String = p.chars().filter(char::is_ascii_digit).collect();
    match digits.as_str() {
        "1" | "2" | "3" => digits.parse().ok(),
        _ => None,
    }
}

fn parse_rating(c: &CompetencyContent, cell: &str) -> std::result::Result<Option<f64>, ()> {
    let t = cell.trim();
    if t.is_empty() || phi::is_missing(t) || t.eq_ignore_ascii_case("n/a") || t.eq_ignore_ascii_case("na") {
        return Ok(None);
    }
    if let Ok(x) = t.trim_end_matches('.').parse::<f64>() {
        if x == 0.0 {
            return Ok(None);
        }
        if (1.0..=4.0).contains(&x) && x.fract() == 0.0 {
            return Ok(Some(x));
        }
        return Err(());
    }
    let p = padded(t);
    for l in &c.rating_scale {
        if p.contains(&format!(" {} ", l.label.to_lowercase())) {
            return Ok(Some(f64::from(l.value)));
        }
    }
    Err(())
}

struct Ratings {
    rows: Vec<StaffDomain>,
    lt_source: Source,
    ident: Vec<String>,
    notes: Vec<String>,
}

fn read_ratings(
    ctx: &Context,
    c: &CompetencyContent,
    f: &Framework,
    args: &WorkforceArgs,
    path: &str,
) -> Result<Ratings> {
    let lt = localdata::load(ctx, path, args.ratings_sheet.as_deref())?;
    if lt.table.truncated {
        return Err(PhnError::InvalidInput("the ratings file has more rows than this tool reads".into()));
    }
    let si = staff_col(&lt, &args.staff_column)?;
    let di = safe_col(&lt, pick(&lt, &args.domain_column, &["domain", "competency domain"], "domain")?)?;
    let ri = safe_col(&lt, pick(&lt, &args.rating_column, &["rating", "score", "level", "self rating"], "rating")?)?;
    let ti = match pick(&lt, &args.tier_column, &["tier"], "tier") {
        Ok(i) => Some(safe_col(&lt, i)?),
        Err(_) if args.tier_column.is_none() => None,
        Err(e) => return Err(e),
    };
    if [Some(si), Some(di), Some(ri), ti].iter().flatten().collect::<BTreeSet<_>>().len()
        != [Some(si), Some(di), Some(ri), ti].iter().flatten().count()
    {
        return Err(PhnError::InvalidInput(
            "the staff, domain, rating, and tier columns must be different columns".into(),
        ));
    }
    let mut acc: BTreeMap<(String, usize), Vec<f64>> = BTreeMap::new();
    let mut tiers: BTreeMap<String, u32> = BTreeMap::new();
    let (mut bad_domain, mut bad_rating, mut bad_tier, mut blank) = (0usize, 0usize, 0usize, 0usize);
    for r in &lt.table.rows {
        let staff = r.get(si).map_or("", |s| s.trim()).to_string();
        if staff.is_empty() {
            blank += 1;
            continue;
        }
        let rating = match parse_rating(c, r.get(ri).map_or("", String::as_str)) {
            Ok(Some(x)) => x,
            Ok(None) => continue,
            Err(()) => {
                bad_rating += 1;
                continue;
            }
        };
        let Some(domain) = find_domain(f, r.get(di).map_or("", String::as_str)) else {
            bad_domain += 1;
            continue;
        };
        if let Some(ti) = ti {
            let cell = r.get(ti).map_or("", String::as_str);
            if !cell.trim().is_empty() {
                match parse_tier(cell) {
                    Some(t) => {
                        if let Some(prev) = tiers.insert(staff.clone(), t)
                            && prev != t
                        {
                            return Err(PhnError::InvalidInput(
                                "a staff code appears with two different tiers; each staff member has one tier".into(),
                            ));
                        }
                    }
                    None => bad_tier += 1,
                }
            }
        }
        acc.entry((staff, domain)).or_default().push(rating);
    }
    if bad_rating > 0 {
        return Err(PhnError::InvalidInput(format!(
            "{bad_rating} rating cell(s) are not 1 to 4 or a level name (None, Aware, Knowledgeable, Proficient); blank or 0 means not rated"
        )));
    }
    if bad_domain > 0 {
        return Err(PhnError::InvalidInput(format!(
            "{bad_domain} row(s) have a domain that matches none of the {} domains of the {}; use the domain number (1 to {}) or its name",
            f.domains.len(),
            f.short_label,
            f.domains.len()
        )));
    }
    if bad_tier > 0 {
        return Err(PhnError::InvalidInput(format!(
            "{bad_tier} row(s) have a tier that is not 1, 2, or 3"
        )));
    }
    let staff: BTreeSet<&String> = acc.keys().map(|(s, _)| s).collect();
    if staff.is_empty() {
        return Err(PhnError::InvalidInput("no row had a staff code, a domain, and a rating".into()));
    }
    if staff.len() > MAX_STAFF {
        return Err(PhnError::InvalidInput(format!("more than {MAX_STAFF} staff codes")));
    }
    let rows: Vec<StaffDomain> = acc
        .into_iter()
        .map(|((s, d), v)| StaffDomain {
            tier: tiers.get(&s).copied(),
            staff: s,
            domain: d,
            rating: v.iter().sum::<f64>() / v.len() as f64,
        })
        .collect();
    let mut notes = vec![
        "Only the staff code, tier, domain, and rating columns were read; no other column in the file was read or returned.".to_string(),
    ];
    if blank > 0 {
        notes.push(format!("{blank} row(s) without a staff code were skipped."));
    }
    Ok(Ratings {
        rows,
        lt_source: lt.source(),
        ident: lt.identifier_categories(),
        notes,
    })
}

// ---------------------------------------------------------------- team results

fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

fn aggregate(
    c: &CompetencyContent,
    f: &Framework,
    rows: &[StaffDomain],
    target: f64,
    min_group: u32,
) -> TeamResults {
    let staff: BTreeSet<&str> = rows.iter().map(|r| r.staff.as_str()).collect();
    let n = staff.len() as u32;
    let mut notes = Vec::new();
    let mut domains = Vec::new();
    let small_team = n < min_group;
    for (i, d) in f.domains.iter().enumerate() {
        let vals: Vec<f64> = rows.iter().filter(|r| r.domain == i).map(|r| r.rating).collect();
        let raters = vals.len() as u32;
        let hide = small_team || raters < min_group;
        let mut res = DomainResult {
            domain_number: d.number,
            domain_id: d.id.clone(),
            domain: d.name.clone(),
            raters: None,
            mean_rating: None,
            gap_to_target: None,
            staff_below_target: None,
            percent_below_target: None,
            suppressed: hide,
            note: String::new(),
        };
        if raters == 0 {
            res.note = "no ratings".into();
            res.suppressed = false;
        } else if hide {
            res.note = if small_team {
                format!("fewer than {min_group} staff on the team: no result by domain")
            } else {
                format!("fewer than {min_group} staff rated this domain")
            };
        } else {
            let m = mean(&vals);
            let below = vals.iter().filter(|v| **v < target).count() as u32;
            res.raters = Some(raters);
            res.mean_rating = Some(r2(m));
            res.gap_to_target = Some(r2(target - m));
            if below == 0 || below >= min_group {
                res.staff_below_target = Some(below);
                res.percent_below_target = Some(r2(f64::from(below) / f64::from(raters) * 100.0));
            } else {
                res.note = format!("fewer than {min_group} staff below target (count not shown)");
            }
        }
        domains.push(res);
    }
    // Tier groups of at least `min_group` staff.
    let mut tiers_out = Vec::new();
    let mut tiers_hidden = false;
    for t in &c.tiers {
        let in_tier: BTreeSet<&str> = rows
            .iter()
            .filter(|r| r.tier == Some(t.number))
            .map(|r| r.staff.as_str())
            .collect();
        if in_tier.is_empty() {
            continue;
        }
        let ok = !small_team && in_tier.len() as u32 >= min_group;
        let vals: Vec<f64> = rows.iter().filter(|r| r.tier == Some(t.number)).map(|r| r.rating).collect();
        tiers_hidden |= !ok;
        tiers_out.push(TierResult {
            tier: t.number,
            label: t.label.clone(),
            staff: ok.then_some(in_tier.len() as u32),
            mean_rating: ok.then(|| r2(mean(&vals))),
            suppressed: !ok,
        });
    }
    if tiers_hidden {
        notes.push(format!(
            "A tier with fewer than {min_group} staff is not shown, so no one's rating can be read from it; those staff are included in the team figures."
        ));
    }
    // Overall.
    let shown: Vec<f64> = domains.iter().filter_map(|d| d.mean_rating).collect();
    let (overall, basis) = if small_team {
        if n >= 2 {
            let all: Vec<f64> = rows.iter().map(|r| r.rating).collect();
            (
                Some(r2(mean(&all))),
                format!("A team of fewer than {min_group}: one total only (all ratings combined); no result by domain, tier, or person."),
            )
        } else {
            (None, format!("A team of one cannot be summarized: fewer than {min_group} people. Use the individual self-assessment in your own files."))
        }
    } else if shown.is_empty() {
        (None, "No domain had enough raters.".to_string())
    } else {
        (
            Some(r2(mean(&shown))),
            if shown.len() == domains.len() {
                "Mean of the domain means.".to_string()
            } else {
                "Mean of the domain means shown (domains with too few raters are left out so they cannot be worked out from the total).".to_string()
            },
        )
    };
    if domains.iter().any(|d| d.suppressed && !small_team) {
        notes.push(format!(
            "Domains rated by fewer than {min_group} staff are not shown."
        ));
    }
    TeamResults {
        framework: f.label.clone(),
        team_size: if small_team {
            format!("fewer than {min_group}")
        } else {
            n.to_string()
        },
        small_group_rule_applied: small_team || domains.iter().any(|d| d.suppressed) || tiers_hidden,
        overall_mean_rating: overall,
        overall_basis: basis,
        domains,
        tiers: tiers_out,
        suppression_notes: notes,
    }
}

fn training_plan(c: &CompetencyContent, team: &TeamResults, target: f64) -> Vec<PlanRow> {
    let steps: Vec<String> = c.training_actions.iter().map(|a| a.text.clone()).collect();
    let mut rows: Vec<(f64, String)> = team
        .domains
        .iter()
        .filter_map(|d| d.gap_to_target.map(|g| (g, d.domain.clone())))
        .collect();
    rows.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
    let _ = target;
    rows.into_iter()
        .enumerate()
        .map(|(i, (gap, domain))| PlanRow {
            rank: i as u32 + 1,
            domain,
            priority: priority(gap).into(),
            gap_to_target: gap.max(0.0),
            suggested_steps: if gap > 0.0 { steps.clone() } else { Vec::new() },
        })
        .collect()
}

pub(crate) fn priority(gap: f64) -> &'static str {
    if gap >= 1.0 {
        "High"
    } else if gap >= 0.5 {
        "Medium"
    } else if gap > 0.0 {
        "Low"
    } else {
        "Maintain"
    }
}

// ---------------------------------------------------------------- credentials

/// Months in "within N months/years of hire".
fn deadline_months(text: &str) -> Option<u32> {
    let re = regex::Regex::new(r"(?i)within\s+(\d+)\s+(month|months|year|years)\b").ok()?;
    let c = re.captures(text)?;
    let n: u32 = c.get(1)?.as_str().parse().ok()?;
    Some(if c.get(2)?.as_str().to_ascii_lowercase().starts_with("year") {
        n * 12
    } else {
        n
    })
}

fn requirement_label(text: &str) -> String {
    let before = match regex::Regex::new(r"(?i)\s+within\s+\d+\s+(month|months|year|years)\b") {
        Ok(re) => re.split(text).next().unwrap_or(text).to_string(),
        Err(_) => text.to_string(),
    };
    let t = before.trim().trim_end_matches('*').trim().to_string();
    if t.chars().count() > 90 {
        let cut: String = t.chars().take(87).collect();
        format!("{cut}...")
    } else {
        t
    }
}

fn requirements_of(p: &WorkforcePosition, universal: &[wc::UniversalItem]) -> Vec<Requirement> {
    let mut out = Vec::new();
    for t in &p.required_at_hire {
        out.push(Requirement {
            label: requirement_label(t),
            kind: "required at hire".into(),
            deadline_months_from_hire: None,
            dph_wording: t.trim_end_matches('*').trim().to_string(),
        });
    }
    for t in &p.required_after_hire {
        out.push(Requirement {
            label: requirement_label(t),
            kind: "required after hire".into(),
            deadline_months_from_hire: deadline_months(t),
            dph_wording: t.trim_end_matches('*').trim().to_string(),
        });
    }
    for t in &p.recommended_after_hire {
        out.push(Requirement {
            label: requirement_label(t),
            kind: "recommended".into(),
            deadline_months_from_hire: deadline_months(t),
            dph_wording: t.trim_end_matches('*').trim().to_string(),
        });
    }
    for u in universal {
        out.push(Requirement {
            label: u.label.clone(),
            kind: u.kind.clone(),
            deadline_months_from_hire: Some(u.months_from_hire),
            dph_wording: u.text.clone(),
        });
    }
    out
}

/// Short keys used to match a credential entry to a requirement.
fn match_keys(label: &str) -> Vec<String> {
    let mut keys = Vec::new();
    for w in label.split(|c: char| !c.is_alphanumeric()) {
        if w.chars().count() >= 2 && w.chars().all(|c| c.is_uppercase() || c.is_ascii_digit()) {
            keys.push(w.to_lowercase());
        }
    }
    if keys.is_empty() {
        let stop = [
            "course", "training", "trained", "of", "the", "and", "or", "in", "a", "for", "to", "complete",
            "certified", "membership", "association", "specific", "relevant", "local", "public", "health",
        ];
        for w in padded(label).split_whitespace() {
            if w.chars().count() >= 4 && !stop.contains(&w) {
                keys.push(w.chars().take(5).collect());
                break;
            }
        }
    }
    keys
}

fn entry_matches(entry: &str, label: &str) -> bool {
    let p = padded(entry);
    if p.trim() == padded(label).trim() {
        return true;
    }
    let toks: Vec<String> = p.split_whitespace().map(str::to_string).collect();
    let keys = match_keys(label);
    !keys.is_empty()
        && keys.iter().any(|k| {
            toks.iter()
                .any(|t| if k.chars().count() <= 3 { t == k } else { t.starts_with(k.as_str()) })
        })
}

fn position_of(c: &CompetencyContent, text: &str) -> Option<String> {
    let mut best: Option<(usize, String, bool)> = None;
    for a in &c.credentials.position_aliases {
        for w in &a.words {
            if has_word(text, w) {
                let len = padded(w).len();
                match &best {
                    Some((l, _, _)) if *l > len => {}
                    Some((l, id, _)) if *l == len && *id != a.position_id => {
                        best = Some((len, id.clone(), true));
                    }
                    _ => best = Some((len, a.position_id.clone(), false)),
                }
            }
        }
    }
    best.filter(|b| !b.2).map(|b| b.1)
}

fn status_for(
    kind: &str,
    hire: NaiveDate,
    months: Option<u32>,
    completed: Option<NaiveDate>,
    as_of: NaiveDate,
    soon: u32,
) -> String {
    if let Some(done) = completed {
        if let Some(m) = months
            && let Some(due) = hire.checked_add_months(Months::new(m))
            && done > due
        {
            return "complete (late)".into();
        }
        return "complete".into();
    }
    match months {
        None => {
            if kind == "required at hire" {
                "confirm at hire".into()
            } else {
                "no deadline".into()
            }
        }
        Some(m) => match hire.checked_add_months(Months::new(m)) {
            None => "no deadline".into(),
            Some(due) if as_of > due => "overdue".into(),
            Some(due) if (due - as_of).num_days() <= i64::from(soon) => "due soon".into(),
            Some(_) => "upcoming".into(),
        },
    }
}

struct Creds {
    rows: Vec<CredRow>,
    summaries: Vec<CredentialSummary>,
    unmatched_position: usize,
    hire_missing: usize,
    entries_not_matched: u32,
    source: Source,
    ident: Vec<String>,
    notes: Vec<String>,
}

fn small(n: usize, min: u32) -> String {
    if n == 0 {
        "0".into()
    } else if (n as u32) < min {
        format!("fewer than {min}")
    } else {
        n.to_string()
    }
}

#[allow(clippy::too_many_lines)]
fn read_credentials(
    ctx: &Context,
    c: &CompetencyContent,
    perf: &PerformanceStandards,
    args: &WorkforceArgs,
    path: &str,
    as_of: NaiveDate,
    soon: u32,
    min_group: u32,
) -> Result<Creds> {
    let lt = localdata::load(ctx, path, args.credentials_sheet.as_deref())?;
    if lt.table.truncated {
        return Err(PhnError::InvalidInput("the credentials file has more rows than this tool reads".into()));
    }
    let si = staff_col(&lt, &args.cred_staff_column)?;
    let pi = safe_col(&lt, pick(&lt, &args.cred_position_column, &["position", "title", "job title"], "position")?)?;
    let hi = pick(&lt, &args.cred_hire_date_column, &["hire date", "date of hire", "start date"], "hire date")?;
    // A hire date is a date column, which `column` allows; confirm it is not refused.
    let hi = safe_col(&lt, hi)?;
    let ci = match pick(&lt, &args.cred_credential_column, &["credential", "training", "certification"], "credential") {
        Ok(i) => Some(safe_col(&lt, i)?),
        Err(_) if args.cred_credential_column.is_none() => None,
        Err(e) => return Err(e),
    };
    let di = match pick(&lt, &args.cred_completed_column, &["completed", "completed date", "date completed"], "completed date") {
        Ok(i) => Some(safe_col(&lt, i)?),
        Err(_) if args.cred_completed_column.is_none() => None,
        Err(e) => return Err(e),
    };
    // Staff: first position and hire date seen.
    let mut staff: BTreeMap<String, (String, Option<NaiveDate>)> = BTreeMap::new();
    let mut entries: Vec<(String, String, Option<NaiveDate>)> = Vec::new();
    let mut bad_dates = 0usize;
    for r in &lt.table.rows {
        let code = r.get(si).map_or("", |s| s.trim()).to_string();
        if code.is_empty() {
            continue;
        }
        let pos = r.get(pi).map_or("", |s| s.trim()).to_string();
        let hire_cell = r.get(hi).map_or("", |s| s.trim());
        let hire = if hire_cell.is_empty() {
            None
        } else {
            match phi::parse_date(hire_cell) {
                Some(d) => Some(d),
                None => {
                    bad_dates += 1;
                    None
                }
            }
        };
        let e = staff.entry(code.clone()).or_insert((pos.clone(), hire));
        if e.0.is_empty() {
            e.0 = pos;
        }
        if e.1.is_none() {
            e.1 = hire;
        }
        if let Some(ci) = ci {
            let cred = r.get(ci).map_or("", |s| s.trim());
            if !cred.is_empty() {
                let done_cell = di.and_then(|d| r.get(d)).map_or("", |s| s.trim());
                let done = if done_cell.is_empty() {
                    None
                } else {
                    match phi::parse_date(done_cell) {
                        Some(d) => Some(d),
                        None => {
                            bad_dates += 1;
                            None
                        }
                    }
                };
                entries.push((code, cred.to_string(), done));
            }
        }
    }
    if bad_dates > 0 {
        return Err(PhnError::InvalidInput(format!(
            "{bad_dates} date cell(s) could not be read as dates between 1900 and 2100; write them as YYYY-MM-DD"
        )));
    }
    if staff.is_empty() {
        return Err(PhnError::InvalidInput("no row had a staff code".into()));
    }
    if staff.len() > MAX_STAFF {
        return Err(PhnError::InvalidInput(format!("more than {MAX_STAFF} staff codes")));
    }

    let by_id: BTreeMap<&str, &WorkforcePosition> = perf
        .workforce_positions
        .iter()
        .map(|p| (p.id.as_str(), p))
        .collect();
    let mut rows: Vec<CredRow> = Vec::new();
    let mut used_entries: BTreeSet<usize> = BTreeSet::new();
    let (mut unmatched_position, mut hire_missing) = (0usize, 0usize);
    // (position id, requirement index) -> staff statuses.
    let mut groups: BTreeMap<(String, usize), Vec<String>> = BTreeMap::new();
    let mut req_cache: BTreeMap<String, Vec<Requirement>> = BTreeMap::new();
    for (code, (pos_text, hire)) in &staff {
        let Some(pid) = position_of(c, pos_text) else {
            unmatched_position += 1;
            continue;
        };
        let Some(p) = by_id.get(pid.as_str()) else {
            unmatched_position += 1;
            continue;
        };
        let Some(hire) = hire else {
            hire_missing += 1;
            continue;
        };
        let reqs = req_cache
            .entry(pid.clone())
            .or_insert_with(|| requirements_of(p, &c.credentials.universal_items))
            .clone();
        for (ri, rq) in reqs.iter().enumerate() {
            let mut completed: Option<NaiveDate> = None;
            if rq.kind != "required at hire" {
                for (k, (ecode, etext, edone)) in entries.iter().enumerate() {
                    if ecode == code && entry_matches(etext, &rq.label) {
                        used_entries.insert(k);
                        completed = match (completed, *edone) {
                            (Some(a), Some(b)) => Some(a.min(b)),
                            (a, b) => a.or(b),
                        };
                        if edone.is_none() && completed.is_none() {
                            // An entry with no completion date counts as complete on an unknown date.
                            completed = Some(as_of.min(*hire));
                        }
                    }
                }
            }
            // A completion dated after the as-of date is not yet complete.
            if completed.is_some_and(|d| d > as_of) {
                completed = None;
            }
            let status = status_for(&rq.kind, *hire, rq.deadline_months_from_hire, completed, as_of, soon);
            groups.entry((pid.clone(), ri)).or_default().push(status.clone());
            rows.push(CredRow {
                staff: code.clone(),
                position: p.position.clone(),
                hire: *hire,
                requirement: rq.label.clone(),
                kind: rq.kind.clone(),
                months: rq.deadline_months_from_hire,
                completed,
                status,
            });
        }
    }
    // Aggregates with the small-group rule.
    let mut summaries = Vec::new();
    for ((pid, ri), statuses) in &groups {
        let Some(reqs) = req_cache.get(pid) else { continue };
        let rq = &reqs[*ri];
        if rq.kind == "required at hire" {
            continue;
        }
        let Some(p) = by_id.get(pid.as_str()) else { continue };
        let total = statuses.len();
        let complete = statuses.iter().filter(|s| s.starts_with("complete")).count();
        let attention = statuses.iter().filter(|s| *s == "overdue" || *s == "due soon").count();
        let upcoming = statuses.iter().filter(|s| *s == "upcoming").count();
        let hide = (total as u32) < min_group
            || [complete, attention, upcoming]
                .iter()
                .any(|n| *n > 0 && (*n as u32) < min_group);
        let show = |n: usize| (!hide).then_some(n as u32);
        summaries.push(CredentialSummary {
            position: p.position.clone(),
            requirement: rq.label.clone(),
            kind: rq.kind.clone(),
            staff: if hide { None } else { Some(total as u32) },
            complete: show(complete),
            needs_attention: show(attention),
            upcoming: show(upcoming),
            suppressed: hide,
        });
    }
    let not_matched = entries.len().saturating_sub(used_entries.len()) as u32;
    let _ = small;
    Ok(Creds {
        rows,
        summaries,
        unmatched_position,
        hire_missing,
        entries_not_matched: not_matched,
        source: lt.source(),
        ident: lt.identifier_categories(),
        notes: vec![
            "Only the staff code, position, hire date, credential, and completed date columns were read; no other column was read or returned.".to_string(),
        ],
    })
}

// ---------------------------------------------------------------- the tool

pub fn assess_workforce_competencies(
    ctx: &Context,
    args: WorkforceArgs,
) -> Result<Envelope<WorkforceResult>> {
    let c = CompetencyContent::load()?;
    let kind = args.framework.unwrap_or_default();
    let f = c.framework(kind.id())?.clone();
    let min_group = c.defaults.minimum_group_size.value as u32;
    let target = args.target_rating.unwrap_or(c.defaults.target_rating.value);
    if !(target.is_finite() && (1.0..=4.0).contains(&target)) {
        return Err(PhnError::InvalidInput("target_rating must be between 1 and 4".into()));
    }
    let soon = args.due_soon_days.unwrap_or(c.defaults.due_soon_days.value as u32);
    if soon > 730 {
        return Err(PhnError::InvalidInput("due_soon_days must be 730 or less".into()));
    }
    let as_of = match args.as_of_date.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(s) => super::oml::parse_date("as_of_date", s)?,
        None => chrono::Utc::now().date_naive(),
    };
    let (perf, perf_entry) = strategy::load_performance(&ctx.snapshots)?;
    let mut sources: Vec<Source> = Vec::new();
    let mut ident: Vec<String> = Vec::new();
    let mut notes: Vec<String> = Vec::new();

    let ratings_path = args.ratings_path.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let cred_path = args.credentials_path.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let want_files = args.write_documents.unwrap_or(true);

    let ratings = match ratings_path {
        Some(p) => {
            let r = read_ratings(ctx, &c, &f, &args, p)?;
            sources.push(r.lt_source.clone());
            ident.extend(r.ident.clone());
            notes.extend(r.notes.clone());
            Some(r)
        }
        None => None,
    };
    let creds = match cred_path {
        Some(p) => {
            let r = read_credentials(ctx, &c, &perf, &args, p, as_of, soon, min_group)?;
            sources.push(r.source.clone());
            ident.extend(r.ident.clone());
            notes.extend(r.notes.clone());
            Some(r)
        }
        None => None,
    };
    ident.sort();
    ident.dedup();

    let team = ratings
        .as_ref()
        .map(|r| aggregate(&c, &f, &r.rows, target, min_group));
    let plan = team
        .as_ref()
        .map(|t| training_plan(&c, t, target))
        .unwrap_or_default();
    let standards: Vec<PositionStandard> = perf
        .workforce_positions
        .iter()
        .map(|p| PositionStandard {
            position_id: p.id.clone(),
            position: p.position.clone(),
            requirements: requirements_of(p, &c.credentials.universal_items),
        })
        .collect();
    let cred_results = creds.as_ref().map(|k| CredentialResults {
        as_of: as_of.to_string(),
        due_soon_days: soon,
        summaries: k.summaries.clone(),
        staff_position_not_matched: small(k.unmatched_position, min_group),
        staff_hire_date_missing: small(k.hire_missing, min_group),
        entries_not_matched: k.entries_not_matched,
    });

    let mut result = WorkforceResult {
        mode: if ratings.is_some() || creds.is_some() { "results" } else { "template" }.into(),
        framework: f.label.clone(),
        domains: f.domains.iter().map(|d| format!("{}. {}", d.number, d.name)).collect(),
        tiers: c.tiers.iter().map(|t| t.label.clone()).collect(),
        rating_scale: c
            .rating_scale
            .iter()
            .map(|l| ScaleLevel { value: l.value, label: l.label.clone(), meaning: l.meaning.clone() })
            .collect(),
        target_rating: target,
        minimum_group_size: min_group,
        team,
        training_plan: plan,
        credentials: cred_results,
        position_standards: standards,
        workbook_file: None,
        plan_file: None,
        not_verified: c
            .not_verified
            .iter()
            .map(|n| NotVerifiedOut { item: n.item.clone(), detail: n.detail.clone() })
            .collect(),
    };

    let mut files = Vec::new();
    if want_files {
        let inputs = workforce_output::OutputInputs {
            content: &c,
            framework: &f,
            result: &result,
            staff_domain: ratings.as_ref().map(|r| r.rows.as_slice()),
            cred_rows: creds.as_ref().map(|k| k.rows.as_slice()),
            as_of,
        };
        let (book, plan_file) = workforce_output::write_files(ctx, &inputs)?;
        files.push(book.clone());
        files.push(plan_file.clone());
        result.workbook_file = Some(book);
        result.plan_file = Some(plan_file);
    }

    let summary = summary_text(&result);
    let mut caveats = vec![
        "Ratings are self-assessments by domain and tier. The numbered competency statements of both frameworks could not be read from the build network, so none is bundled and none is checked; type your own competency labels from the framework you download if you want them in your file. The rating scale and tier descriptions are short paraphrases read in published articles (see the sources).".to_string(),
        format!("No result is shown for fewer than {min_group} people: a team of fewer than {min_group} gets one combined total at most, a domain rated by fewer than {min_group} staff is left out, and a tier with fewer than {min_group} staff is not shown. Staff are identified by a code, never a name, and individual ratings appear only in the workbook in your output folder."),
        format!("The target rating ({target}) is a tool default with no published source; set your own by domain or tier from your workforce development plan. Training steps are generic; no course or vendor is recommended or verified."),
        "Credential deadlines are read from DPH's wording (\"within N months or years of hire\") in the Performance Standards workbook snapshot and added to the hire date you give. Waivers, prior service, equivalents, and standards DPH changed after July 2024 are not modeled; credential entries are matched to DPH's list by keyword, so check the matches in the workbook.".into(),
        "Decision support for workforce planning, not a finding of compliance with state standards and not a personnel evaluation.".into(),
    ];
    caveats.extend(wc::caveats(&[wc::COMPETENCIES]));
    sources.extend(wc::cite(
        &["cdc_competencies", "fphp_2025", "nurse_educ_2025", "phn_2025", "amp_2024", "qcc_2020"],
        &c.sources,
    ));
    sources.push(perf_entry.to_source());
    sources.push(Source::computed(
        "Team means, small-group rule, and credential deadlines",
        "Calculated by this server",
    ));
    notes.push("Individual ratings and credential lines are written only to the workbook in the output folder; the response has team figures and counts.".into());
    let privacy = Privacy {
        row_level_data: false,
        suppression_applied: true,
        suppression_threshold: Some(min_group),
        identifiers_detected: ident,
        files_written: files,
        notes,
    };
    Ok(Envelope::new(summary, result)
        .decision_support()
        .sources(sources)
        .caveats(caveats)
        .privacy(privacy))
}

fn num(x: Option<f64>) -> String {
    x.map_or("-".into(), |v| md::num(v, 2))
}

fn summary_text(r: &WorkforceResult) -> String {
    let mut s = String::new();
    match &r.team {
        None if r.credentials.is_none() => {
            s.push_str(&format!(
                "**Blank self-assessment ({}).** The workbook has a ratings form (one row per staff code and domain), team results with live formulas that hide any result for fewer than {} people, a training plan sheet, and a credentials tracker. Fill it in, then give it back as ratings_path (and credentials_path).\n\n**Domains:** {}\n\n**Tiers:** {}\n",
                r.framework,
                r.minimum_group_size,
                r.domains.join("; "),
                r.tiers.join("; ")
            ));
        }
        Some(t) => {
            s.push_str(&format!(
                "**Team self-assessment ({}), {} staff, target {}.** {}\n\n",
                t.framework,
                t.team_size,
                r.target_rating,
                match t.overall_mean_rating {
                    Some(m) => format!("Overall {} on the 1 to 4 scale. {}", md::num(m, 2), t.overall_basis),
                    None => t.overall_basis.clone(),
                }
            ));
            let rows: Vec<Vec<String>> = t
                .domains
                .iter()
                .map(|d| {
                    vec![
                        d.domain.clone(),
                        d.raters.map_or("-".into(), |n| n.to_string()),
                        num(d.mean_rating),
                        num(d.gap_to_target),
                        d.staff_below_target.map_or("-".into(), |n| n.to_string()),
                        d.note.clone(),
                    ]
                })
                .collect();
            s.push_str(&md::table(&["Domain", "Raters", "Mean", "Gap to target", "Below target", "Note"], &rows));
            if !t.tiers.is_empty() {
                s.push_str("\n**By tier**\n\n");
                let rows: Vec<Vec<String>> = t
                    .tiers
                    .iter()
                    .map(|x| {
                        vec![
                            x.label.clone(),
                            x.staff.map_or("not shown".into(), |n| n.to_string()),
                            num(x.mean_rating),
                        ]
                    })
                    .collect();
                s.push_str(&md::table(&["Tier", "Staff", "Mean"], &rows));
            }
            for n in &t.suppression_notes {
                s.push_str(&format!("\n{n}\n"));
            }
            if !r.training_plan.is_empty() {
                s.push_str("\n**Training plan (largest gap first)**\n\n");
                let rows: Vec<Vec<String>> = r
                    .training_plan
                    .iter()
                    .map(|p| vec![p.rank.to_string(), p.domain.clone(), p.priority.clone(), md::num(p.gap_to_target, 2)])
                    .collect();
                s.push_str(&md::table(&["#", "Domain", "Priority", "Gap"], &rows));
            }
        }
        None => {}
    }
    if let Some(k) = &r.credentials {
        s.push_str(&format!(
            "\n**Massachusetts workforce standards (DPH) as of {}**\n\n",
            k.as_of
        ));
        if k.summaries.is_empty() {
            s.push_str("No credential line could be tracked (check positions and hire dates).\n");
        } else {
            let rows: Vec<Vec<String>> = k
                .summaries
                .iter()
                .map(|x| {
                    let f = |n: Option<u32>| n.map_or("fewer than 3 or hidden".into(), |v| v.to_string());
                    vec![
                        x.position.clone(),
                        x.requirement.clone(),
                        x.kind.clone(),
                        f(x.complete),
                        f(x.needs_attention),
                        f(x.upcoming),
                    ]
                })
                .collect();
            s.push_str(&md::table(&["Position", "Credential", "Kind", "Complete", "Overdue or due soon", "Upcoming"], &rows));
        }
        s.push_str(&format!(
            "\nStaff with a position not matched: {}; staff with no hire date: {}; credential entries not matched to a requirement: {}.\n",
            k.staff_position_not_matched, k.staff_hire_date_missing, k.entries_not_matched
        ));
    }
    if let (Some(b), Some(p)) = (&r.workbook_file, &r.plan_file) {
        s.push_str(&format!("\nWorkbook with live formulas: {b}. Training plan: {p}.\n"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content() -> (CompetencyContent, Framework) {
        let c = CompetencyContent::load().unwrap();
        let f = c.framework("core_competencies_2021").unwrap().clone();
        (c, f)
    }

    fn rows(spec: &[(&str, u32, usize, f64)]) -> Vec<StaffDomain> {
        spec.iter()
            .map(|(s, t, d, r)| StaffDomain {
                staff: (*s).into(),
                tier: Some(*t),
                domain: *d,
                rating: *r,
            })
            .collect()
    }

    #[test]
    fn domains_are_found_by_number_name_and_alias() {
        let (_, f) = content();
        assert_eq!(find_domain(&f, "1"), Some(0));
        assert_eq!(find_domain(&f, "D8"), Some(7));
        assert_eq!(find_domain(&f, "Communication Skills"), Some(2));
        assert_eq!(find_domain(&f, "health equity"), Some(3));
        assert_eq!(find_domain(&f, "9"), None);
        assert_eq!(find_domain(&f, "something else"), None);
    }

    #[test]
    fn ratings_read_as_numbers_or_level_names() {
        let (c, _) = content();
        assert_eq!(parse_rating(&c, "3").unwrap(), Some(3.0));
        assert_eq!(parse_rating(&c, "Aware").unwrap(), Some(2.0));
        assert_eq!(parse_rating(&c, "4 - Proficient").unwrap(), Some(4.0));
        assert_eq!(parse_rating(&c, "").unwrap(), None);
        assert_eq!(parse_rating(&c, "0").unwrap(), None);
        assert!(parse_rating(&c, "5").is_err());
        assert!(parse_rating(&c, "2.5").is_err());
    }

    #[test]
    fn a_team_of_two_gets_one_total_and_no_domain_result() {
        let (c, f) = content();
        let r = rows(&[("S1", 1, 0, 2.0), ("S2", 1, 0, 4.0), ("S1", 1, 1, 3.0)]);
        let t = aggregate(&c, &f, &r, 3.0, 3);
        assert_eq!(t.team_size, "fewer than 3");
        assert!(t.domains.iter().all(|d| d.mean_rating.is_none()));
        assert_eq!(t.overall_mean_rating, Some(3.0));
        assert!(t.tiers.iter().all(|x| x.suppressed));
        // A team of one gets nothing.
        let one = aggregate(&c, &f, &rows(&[("S1", 1, 0, 2.0)]), 3.0, 3);
        assert_eq!(one.overall_mean_rating, None);
    }

    #[test]
    fn domains_with_fewer_than_three_raters_are_left_out_of_the_overall() {
        let (c, f) = content();
        // Three staff rate domain 1; only two rate domain 2.
        let r = rows(&[
            ("S1", 1, 0, 2.0),
            ("S2", 1, 0, 3.0),
            ("S3", 2, 0, 4.0),
            ("S1", 1, 1, 1.0),
            ("S2", 1, 1, 1.0),
        ]);
        let t = aggregate(&c, &f, &r, 3.0, 3);
        assert_eq!(t.domains[0].mean_rating, Some(3.0));
        assert_eq!(t.domains[0].raters, Some(3));
        assert!(t.domains[1].suppressed);
        assert_eq!(t.domains[1].mean_rating, None);
        // The overall uses only the domain shown, so the hidden domain cannot be worked out.
        assert_eq!(t.overall_mean_rating, Some(3.0));
        // One person below target in domain 1 (the 2.0): fewer than 3, so the count is hidden.
        assert_eq!(t.domains[0].staff_below_target, None);
        // Tiers: tier 1 has two staff, tier 2 has one: neither is shown.
        assert!(t.tiers.iter().all(|x| x.suppressed));
    }

    #[test]
    fn means_and_gaps_match_hand_arithmetic() {
        let (c, f) = content();
        // Domain 1: 2, 3, 3, 4 -> mean 3.0; domain 3: 1, 2, 2 -> mean 1.6667 (gap 1.33, High).
        let r = rows(&[
            ("S1", 1, 0, 2.0),
            ("S2", 1, 0, 3.0),
            ("S3", 1, 0, 3.0),
            ("S4", 1, 0, 4.0),
            ("S1", 1, 2, 1.0),
            ("S2", 1, 2, 2.0),
            ("S3", 1, 2, 2.0),
        ]);
        let t = aggregate(&c, &f, &r, 3.0, 3);
        assert_eq!(t.domains[0].mean_rating, Some(3.0));
        assert_eq!(t.domains[0].gap_to_target, Some(0.0));
        assert_eq!(t.domains[2].mean_rating, Some(1.67));
        assert_eq!(t.domains[2].gap_to_target, Some(1.33));
        assert_eq!(t.domains[2].staff_below_target, Some(3));
        assert_eq!(t.domains[0].staff_below_target, None); // one below target (the 2.0)
        let plan = training_plan(&c, &t, 3.0);
        assert_eq!(plan[0].domain, "Communication Skills");
        assert_eq!(plan[0].priority, "High");
        assert_eq!(plan[1].priority, "Maintain");
        assert!(plan[1].suggested_steps.is_empty());
        assert_eq!(priority(0.5), "Medium");
        assert_eq!(priority(0.49), "Low");
    }

    #[test]
    fn deadlines_are_read_from_dph_wording() {
        assert_eq!(deadline_months("MAVEN-trained within 6 months of hire"), Some(6));
        assert_eq!(deadline_months("Foundations course within 1 year of hire"), Some(12));
        assert_eq!(deadline_months("RS within 6 years of hire"), Some(72));
        assert_eq!(deadline_months("Registered Nurse (RN), current MA license"), None);
        assert_eq!(requirement_label("MAVEN-trained within 6 months of hire"), "MAVEN-trained");
        assert_eq!(requirement_label("RS or equivalent within 1 year of hire*"), "RS or equivalent");
    }

    #[test]
    fn the_public_health_nurse_standard_has_two_deadlines() {
        let (c, _) = content();
        let (perf, _) = strategy::load_performance(&phn_data::SnapshotStore::bundled()).unwrap();
        let p = perf.workforce_positions.iter().find(|p| p.id == "WF-POS-04").unwrap();
        let reqs = requirements_of(p, &c.credentials.universal_items);
        let months: Vec<(String, Option<u32>)> = reqs
            .iter()
            .filter(|r| r.kind == "required after hire")
            .map(|r| (r.label.clone(), r.deadline_months_from_hire))
            .collect();
        assert_eq!(
            months,
            vec![
                ("MAVEN-trained".to_string(), Some(6)),
                ("Foundations course".to_string(), Some(12))
            ]
        );
    }

    #[test]
    fn status_follows_the_deadline() {
        let hire = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        let d = |m, day| NaiveDate::from_ymd_opt(2026, m, day).unwrap();
        // MAVEN within 6 months: due 2026-07-15.
        assert_eq!(status_for("required after hire", hire, Some(6), None, d(11, 12), 90), "overdue");
        assert_eq!(status_for("required after hire", hire, Some(6), None, d(5, 1), 90), "due soon");
        assert_eq!(status_for("required after hire", hire, Some(6), None, d(2, 1), 90), "upcoming");
        assert_eq!(status_for("required after hire", hire, Some(6), Some(d(7, 1)), d(11, 12), 90), "complete");
        assert_eq!(status_for("required after hire", hire, Some(6), Some(d(8, 1)), d(11, 12), 90), "complete (late)");
        assert_eq!(status_for("required at hire", hire, None, None, d(11, 12), 90), "confirm at hire");
    }

    #[test]
    fn positions_and_entries_match_by_keyword() {
        let (c, _) = content();
        assert_eq!(position_of(&c, "Public Health Nurse").as_deref(), Some("WF-POS-04"));
        assert_eq!(position_of(&c, "Health Director").as_deref(), Some("WF-POS-01"));
        assert_eq!(position_of(&c, "Astronaut"), None);
        assert!(entry_matches("MAVEN training", "MAVEN-trained"));
        assert!(entry_matches("Foundations for Local Public Health Practice", "Foundations course"));
        assert!(!entry_matches("CPR", "MAVEN-trained"));
    }
}
