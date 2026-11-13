//! LEAD-07 `board_of_health_meeting_assistant`: Open Meeting Law notice deadline calculator and
//! checker, meeting agendas and notice drafts, staff report templates, and minutes drafted from
//! the nurse's notes (M.G.L. c. 30A, ss. 18-25; 940 CMR 29.00; legal holidays from M.G.L. c. 4,
//! s. 7, clause Eighteenth).
//!
//! Notes are read locally and never returned: the result reports which required parts of the
//! minutes were found and which identifiers the PHI detector saw (by line number), and the minutes
//! are written to /data/output. Decision support, not legal advice.

use std::collections::BTreeSet;

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};
use phn_core::{Envelope, PhnError, Privacy, Result, Source};
use phn_docs::model::Block;
use phn_local::phi;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::oml::{self, Calendar};
use super::strategy_docs::{DocBuilder, OutFormat, tidy, write_documents};
use super::workforce_content::{self as wc, HolidayContent, OmlContent};
use crate::context::Context;
use crate::md;

const MAX_TOPICS: usize = 40;
const MAX_NOTES_CHARS: usize = 200_000;

// ---------------------------------------------------------------- arguments

/// What to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BoardAction {
    /// Calculate the latest time to post the meeting notice, and check a posting time, the
    /// notice contents, and the executive session items against the Open Meeting Law.
    NoticeCheck,
    /// Build the meeting agenda and notice draft (Word), with the posting deadline and the
    /// chair's checklist for staff.
    Agenda,
    /// Build a staff report template for the board (Word), filled with the sections you give.
    StaffReport,
    /// Draft minutes (Word) from your meeting notes, and check the required parts.
    Minutes,
    /// The Open Meeting Law requirements for a local board of health, with sources.
    OmlGuide,
}

/// One agenda topic.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct AgendaTopicArg {
    /// The topic as the public will read it. Be specific: the notice must list the topics the
    /// chair reasonably anticipates, specific enough to tell the public what will be discussed.
    pub title: String,
    /// A line of detail, for example the permit or program (no resident's name).
    pub detail: Option<String>,
    /// Who presents, by role (for example "Public health nurse").
    pub presenter_role: Option<String>,
    /// Minutes planned for the topic.
    pub minutes: Option<u32>,
    /// True when the board is asked to vote.
    pub action_requested: Option<bool>,
}

/// One section of a staff report.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ReportSectionArg {
    /// Section heading, for example "Background" or "Recommendation".
    pub heading: String,
    /// The text. Do not include client names or identifiers: board materials are public records.
    pub text: String,
}

/// Arguments for `board_of_health_meeting_assistant`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct BoardMeetingArgs {
    /// notice_check, agenda, staff_report, minutes, or oml_guide. Required.
    pub action: Option<BoardAction>,
    /// Name of the body (default "Board of Health").
    pub body_name: Option<String>,
    /// The city or town, to find its county (Suffolk county bodies also skip Evacuation Day and
    /// Bunker Hill Day). Defaults to the town set by configure_jurisdiction.
    pub town: Option<String>,
    /// Treat the body as a Suffolk county body (overrides the town's county).
    pub suffolk_county: Option<bool>,
    /// Meeting date (YYYY-MM-DD). Between 2026-01-06 and 2028-12-31.
    pub meeting_date: Option<String>,
    /// Meeting start time, such as 18:00 or 6:00 pm.
    pub meeting_time: Option<String>,
    /// Meeting place (room and building). Not needed for a fully remote meeting.
    pub location: Option<String>,
    /// True when the meeting is held entirely or partly by remote participation.
    pub remote_meeting: Option<bool>,
    /// How to join a remote meeting (link, call-in number) as it should appear on the notice.
    pub remote_access: Option<String>,
    /// True for an emergency (a sudden, generally unexpected occurrence demanding immediate
    /// action): notice is posted as soon as reasonably possible instead of 48 hours ahead.
    pub emergency: Option<bool>,
    /// When the notice was (or will be) posted, YYYY-MM-DD HH:MM, to check it against the
    /// deadline.
    pub posted_at: Option<String>,
    /// Agenda topics (up to 40).
    pub topics: Option<Vec<AgendaTopicArg>>,
    /// Executive session purposes by number (1 to 10, M.G.L. c. 30A, s. 21(a)). Executive session
    /// can be used only for these purposes.
    pub executive_session_purposes: Option<Vec<u32>>,
    /// Title of the staff report (action staff_report).
    pub report_title: Option<String>,
    /// Sections of the staff report you have drafted; the rest are placeholders.
    pub report_sections: Option<Vec<ReportSectionArg>>,
    /// A .txt or .md file in /data with your meeting notes (action minutes). Lines such as
    /// "Present:", "Absent:", "Remote:", "Called to order:", "Item:", "Motion:", "Second:",
    /// "Vote:", "Exhibit:", "Executive session:", and "Adjourned:" are placed in the minutes; other
    /// lines become discussion notes under the current item. The notes are never returned.
    pub notes_path: Option<String>,
    /// Notes typed here instead of a file (up to 200,000 characters); same layout.
    pub notes_text: Option<String>,
    /// Write the Word file to /data/output (default true for agenda, staff_report, and minutes).
    pub write_document: Option<bool>,
}

// ---------------------------------------------------------------- results

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExcludedDay {
    pub date: String,
    pub weekday: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PostedCheck {
    pub posted_at: String,
    /// Hours between posting and the meeting that count (not Saturdays, Sundays, or legal
    /// holidays).
    pub countable_hours: f64,
    pub required_hours: u32,
    pub compliant: bool,
    pub shortfall_minutes: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Check {
    pub id: String,
    /// `ok`, `attention`, or `info`.
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NoticeResult {
    pub meeting: String,
    pub body_is_suffolk_county: bool,
    pub emergency: bool,
    /// Latest time to post (48 countable hours before the meeting); null for an emergency.
    pub post_by: Option<String>,
    pub post_by_iso: Option<String>,
    pub excluded_days_passed: Vec<ExcludedDay>,
    pub meeting_on_excluded_day: bool,
    pub posted_check: Option<PostedCheck>,
    pub checks: Vec<Check>,
    pub method: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExecPurpose {
    pub number: u32,
    pub summary: String,
    pub chair_must_declare_detrimental_effect: bool,
    pub special_requirements: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AgendaResult {
    pub topics: usize,
    pub executive_session_purposes: Vec<ExecPurpose>,
    pub planned_minutes: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReportResult {
    pub sections_drafted: usize,
    pub sections_placeholder: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MinutesPart {
    pub part: String,
    /// `found`, `missing`, `not_needed`, or `check`.
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MinutesResult {
    pub notes_lines: usize,
    pub items: usize,
    pub items_without_discussion: usize,
    pub votes_recorded: usize,
    pub documents_listed: usize,
    pub executive_session_in_notes: bool,
    pub remote_members_in_notes: bool,
    pub parts: Vec<MinutesPart>,
    /// Identifier categories the PHI detector saw in the notes (no values).
    pub identifier_categories: Vec<String>,
    /// Line numbers of notes lines with a possible identifier (at most 50).
    pub identifier_lines: Vec<usize>,
    /// 30 days after the meeting: the usual outer limit to approve open session minutes (or the
    /// third following meeting, whichever is later).
    pub approve_by_30_days: Option<String>,
    pub release_on_request_days: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HolidayOut {
    pub date: String,
    pub weekday: String,
    pub name: String,
    pub scope: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OmlGuide {
    pub public_body: String,
    pub notice_rule: String,
    pub notice_contents: Vec<String>,
    pub common_questions: Vec<String>,
    pub executive_session_purposes: Vec<ExecPurpose>,
    pub executive_session_steps: Vec<String>,
    pub minutes_required: Vec<String>,
    pub minutes_conditional: Vec<String>,
    pub minutes_approval: String,
    pub chair_duties: Vec<String>,
    pub remote_participation: String,
    pub legal_holidays: Vec<HolidayOut>,
}

/// Result of `board_of_health_meeting_assistant`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BoardMeetingResult {
    pub action: String,
    pub notice: Option<NoticeResult>,
    pub agenda: Option<AgendaResult>,
    pub staff_report: Option<ReportResult>,
    pub minutes: Option<MinutesResult>,
    pub oml_guide: Option<OmlGuide>,
    pub document_file: Option<String>,
}

// ---------------------------------------------------------------- helpers

fn clean(what: &str, s: &str, max: usize) -> Result<String> {
    let t = tidy(s);
    if t.is_empty() {
        return Err(PhnError::InvalidInput(format!("{what} is empty")));
    }
    if t.chars().count() > max {
        return Err(PhnError::InvalidInput(format!(
            "{what} is longer than {max} characters"
        )));
    }
    if !phi::scan_text(&t).is_empty() {
        return Err(PhnError::Privacy(format!(
            "{what} appears to contain an identifier; board materials are public records, so leave out names and identifiers of residents and clients"
        )));
    }
    Ok(t)
}

fn clean_opt(what: &str, s: &Option<String>, max: usize) -> Result<Option<String>> {
    match s {
        Some(v) if !v.trim().is_empty() => Ok(Some(clean(what, v, max)?)),
        _ => Ok(None),
    }
}

fn is_suffolk(ctx: &Context, args: &BoardMeetingArgs) -> Result<(bool, Option<String>)> {
    if let Some(s) = args.suffolk_county {
        return Ok((s, Some("as you said".into())));
    }
    let town = args
        .town
        .clone()
        .filter(|t| !t.trim().is_empty())
        .or_else(|| ctx.settings().jurisdiction.town);
    match town {
        Some(t) => match ctx.ma.town(&t) {
            Some(m) => Ok((
                m.county.eq_ignore_ascii_case("Suffolk"),
                Some(format!("{} is in {} County", m.name, m.county)),
            )),
            None => {
                let sug: Vec<String> = ctx
                    .ma
                    .suggest_towns(&t, 4)
                    .into_iter()
                    .map(|m| m.name.clone())
                    .collect();
                Err(PhnError::InvalidInput(format!(
                    "'{t}' is not a Massachusetts city or town in the bundled list{}",
                    if sug.is_empty() {
                        String::new()
                    } else {
                        format!("; did you mean {}?", sug.join(", "))
                    }
                )))
            }
        },
        None => Ok((false, None)),
    }
}

fn exec_purposes(c: &OmlContent, nums: &[u32]) -> Result<Vec<ExecPurpose>> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for n in nums {
        if !(1..=10).contains(n) {
            return Err(PhnError::InvalidInput(
                "executive_session_purposes are numbers 1 to 10 (M.G.L. c. 30A, s. 21(a))".into(),
            ));
        }
        if !seen.insert(*n) {
            return Err(PhnError::InvalidInput(format!(
                "purpose {n} is listed twice"
            )));
        }
        let p = c
            .executive_session
            .purposes
            .iter()
            .find(|p| p.number == *n)
            .ok_or_else(|| PhnError::Internal(format!("purpose {n} missing")))?;
        out.push(ExecPurpose {
            number: p.number,
            summary: p.summary.clone(),
            chair_must_declare_detrimental_effect: p.chair_declaration_required,
            special_requirements: p.special_requirements.clone(),
        });
    }
    Ok(out)
}

/// Topics that are probably too general (this project's cues; the regulation says the list needs
/// "sufficient specificity").
const GENERAL_TOPICS: [&str; 9] = [
    "other business",
    "new business",
    "old business",
    "unfinished business",
    "miscellaneous",
    "any other business",
    "other matters",
    "general discussion",
    "discussion",
];

fn topic_is_general(title: &str) -> bool {
    let t = title.trim().to_lowercase();
    let t = t.trim_end_matches(['.', ':']);
    GENERAL_TOPICS.contains(&t) || t.split_whitespace().count() < 2
}

struct Meeting {
    date: NaiveDate,
    dt: NaiveDateTime,
}

fn meeting_of(args: &BoardMeetingArgs) -> Result<Meeting> {
    let d = args.meeting_date.as_deref().ok_or_else(|| {
        PhnError::InvalidInput("give meeting_date (YYYY-MM-DD) and meeting_time".into())
    })?;
    let t = args.meeting_time.as_deref().ok_or_else(|| {
        PhnError::InvalidInput("give meeting_time, such as 18:00 or 6:00 pm".into())
    })?;
    let date = oml::parse_date("meeting_date", d)?;
    let time = oml::parse_time("meeting_time", t)?;
    Ok(Meeting {
        date,
        dt: date.and_time(time),
    })
}

fn topics_of(args: &BoardMeetingArgs) -> Result<Vec<AgendaTopicArg>> {
    let given = args.topics.clone().unwrap_or_default();
    if given.len() > MAX_TOPICS {
        return Err(PhnError::InvalidInput(format!(
            "at most {MAX_TOPICS} topics"
        )));
    }
    let mut out = Vec::new();
    for t in &given {
        out.push(AgendaTopicArg {
            title: clean("a topic title", &t.title, 200)?,
            detail: clean_opt("a topic detail", &t.detail, 300)?,
            presenter_role: clean_opt("a presenter role", &t.presenter_role, 80)?,
            minutes: t.minutes.filter(|m| (1..=600).contains(m)),
            action_requested: t.action_requested,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------- notice

struct NoticeInputs<'a> {
    args: &'a BoardMeetingArgs,
    topics: &'a [AgendaTopicArg],
    suffolk: bool,
    exec: &'a [ExecPurpose],
}

fn notice_result(ctx_note: &str, i: &NoticeInputs, m: &Meeting) -> Result<NoticeResult> {
    let cal = Calendar::load(i.suffolk)?;
    let emergency = i.args.emergency.unwrap_or(false);
    let method = format!(
        "Hours are counted back from the meeting, leaving out whole Saturdays, Sundays, and legal holidays (M.G.L. c. 4, s. 7, clause Eighteenth{}), until 48 have been counted. This is this project's reading of the statute; the Attorney General gives no worked example. File the notice with the clerk early enough for the clerk to post it by this time.",
        if i.suffolk {
            "; this body is treated as a Suffolk county body, so Evacuation Day and Bunker Hill Day also count"
        } else {
            ""
        }
    );
    let _ = ctx_note;
    let (deadline, skipped) = cal.posting_deadline(m.dt)?;
    let excluded_days_passed: Vec<ExcludedDay> = skipped
        .iter()
        .rev()
        .map(|e| ExcludedDay {
            date: e.date.to_string(),
            weekday: oml::weekday_name(e.date).to_string(),
            reason: e.reason.clone(),
        })
        .collect();
    let on_excluded = cal.exclusion(m.date).is_some();
    let posted_check = match args_posted(i.args)? {
        Some(p) => {
            let countable = cal.countable_minutes(p, m.dt)?;
            let compliant = if emergency { p < m.dt } else { p <= deadline };
            Some(PostedCheck {
                posted_at: oml::show(p),
                countable_hours: (countable as f64 / 60.0 * 100.0).round() / 100.0,
                required_hours: 48,
                compliant,
                shortfall_minutes: if emergency {
                    0
                } else {
                    (2880 - countable).max(0)
                },
            })
        }
        None => None,
    };

    let mut checks = Vec::new();
    let mut add = |id: &str, status: &str, message: String| {
        checks.push(Check {
            id: id.into(),
            status: status.into(),
            message,
        });
    };
    if emergency {
        add(
            "emergency",
            "info",
            "Emergency: post notice as soon as reasonably possible before the meeting. An emergency is a sudden, generally unexpected occurrence or set of circumstances demanding immediate action; the 48-hour rule does not apply, but record why the meeting was an emergency in the minutes.".into(),
        );
    }
    if on_excluded {
        add(
            "excluded_day",
            "attention",
            format!(
                "The meeting falls on a {}. The law is silent on Sunday meetings, and the Attorney General says the best practice is not to hold meetings on Sundays or holidays.",
                cal.exclusion(m.date).unwrap_or_default()
            ),
        );
    }
    match (&i.args.location, i.args.remote_meeting.unwrap_or(false)) {
        (Some(l), _) if !l.trim().is_empty() => {
            add("place", "ok", "A meeting place is given.".into())
        }
        (_, true) => add(
            "place",
            "ok",
            "A fully remote meeting: the notice needs the information to join.".into(),
        ),
        _ => add(
            "place",
            "attention",
            "No meeting place given: the notice must contain the date, time, and place.".into(),
        ),
    }
    if i.args.remote_meeting.unwrap_or(false) {
        if i.args
            .remote_access
            .as_deref()
            .is_none_or(|s| s.trim().is_empty())
        {
            add(
                "remote_access",
                "attention",
                "Remote participation: the notice needs clear information to access the meeting (a link or call-in instructions).".into(),
            );
        } else {
            add(
                "remote_access",
                "ok",
                "Remote access information is given.".into(),
            );
        }
    }
    if i.topics.is_empty() {
        add(
            "topics",
            "attention",
            "No topics given: the notice must list the topics the chair reasonably anticipates will be discussed.".into(),
        );
    } else {
        let general: Vec<usize> = i
            .topics
            .iter()
            .enumerate()
            .filter(|(_, t)| topic_is_general(&t.title))
            .map(|(n, _)| n + 1)
            .collect();
        if general.is_empty() {
            add(
                "topics",
                "ok",
                format!(
                    "{} topics listed; none looks too general by this tool's word cues.",
                    i.topics.len()
                ),
            );
        } else {
            add(
                "topics",
                "attention",
                format!(
                    "Topic number(s) {} may be too general. Topics need sufficient specificity to reasonably advise the public of the issues (940 CMR 29.03(1)(b)); this check uses word cues only.",
                    general
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
        }
    }
    if !i.exec.is_empty() {
        let listed = i
            .topics
            .iter()
            .any(|t| t.title.to_lowercase().contains("executive session"));
        if listed {
            add(
                "executive_session",
                "ok",
                "Executive session appears among the topics.".into(),
            );
        } else {
            add(
                "executive_session",
                "attention",
                "Executive session must be listed as a topic on the notice, with as much information as possible without defeating its purpose. Add a topic that names it (for example \"Executive session under M.G.L. c. 30A, s. 21(a)(3): litigation strategy\").".into(),
            );
        }
        for p in i.exec {
            if p.number == 1 {
                add("executive_session_1", "attention", "Purpose 1: the individual to be discussed must be notified in writing at least 48 hours before the executive session (unless waived in writing), and may ask for open session.".into());
            }
            if p.chair_must_declare_detrimental_effect {
                add(
                    &format!("executive_session_{}", p.number),
                    "attention",
                    format!(
                        "Purpose {}: the chair must declare in open session that an open meeting may have a detrimental effect on the stated position.",
                        p.number
                    ),
                );
            }
        }
    }
    add(
        "posting_record",
        "info",
        "Record the date and time the notice is posted on or with it (940 CMR 29.03(1)(d)); if you amend it within the 48 hours, record the amendment's date and time too. Post where the clerk's office is, or on the municipal website if that is your municipality's official method.".into(),
    );
    if let Some(p) = &posted_check {
        if p.compliant {
            add(
                "posted_at",
                "ok",
                format!(
                    "Posted {}: {} countable hours before the meeting, enough.",
                    p.posted_at, p.countable_hours
                ),
            );
        } else {
            add(
                "posted_at",
                "attention",
                format!(
                    "Posted {}: only {} countable hours before the meeting, {} minutes short of 48. Re-post for a later meeting time or date, or treat the meeting as an emergency only if it is one.",
                    p.posted_at, p.countable_hours, p.shortfall_minutes
                ),
            );
        }
    }

    Ok(NoticeResult {
        meeting: oml::show(m.dt),
        body_is_suffolk_county: i.suffolk,
        emergency,
        post_by: (!emergency).then(|| oml::show(deadline)),
        post_by_iso: (!emergency).then(|| deadline.format("%Y-%m-%d %H:%M").to_string()),
        excluded_days_passed,
        meeting_on_excluded_day: on_excluded,
        posted_check,
        checks,
        method,
    })
}

fn args_posted(args: &BoardMeetingArgs) -> Result<Option<NaiveDateTime>> {
    match args.posted_at.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(s) => Ok(Some(oml::parse_datetime("posted_at", s)?)),
        None => Ok(None),
    }
}

// ---------------------------------------------------------------- agenda and report documents

fn body_name(args: &BoardMeetingArgs) -> Result<String> {
    match clean_opt("body_name", &args.body_name, 100)? {
        Some(b) => Ok(b),
        None => Ok("Board of Health".into()),
    }
}

fn agenda_document(
    ctx: &Context,
    c: &OmlContent,
    args: &BoardMeetingArgs,
    m: &Meeting,
    notice: &NoticeResult,
    topics: &[AgendaTopicArg],
    exec: &[ExecPurpose],
) -> Result<phn_docs::model::Document> {
    let body = body_name(args)?;
    let mut b = DocBuilder::new();
    let place = match (&args.location, args.remote_meeting.unwrap_or(false)) {
        (Some(l), true) => format!("{} (remote participation also available)", tidy(l)),
        (Some(l), false) => tidy(l),
        (None, true) => "Remote meeting".into(),
        (None, false) => "[NURSE: meeting place]".into(),
    };
    b.labeled("Date and time:", oml::show(m.dt));
    b.labeled("Place:", place);
    if args.remote_meeting.unwrap_or(false) {
        b.labeled(
            "How to join:",
            clean_opt("remote_access", &args.remote_access, 300)?
                .unwrap_or_else(|| "[NURSE: link or call-in instructions]".into()),
        );
    }
    b.labeled(
        "Notice posted (date and time):",
        "[CLERK: record when the notice is posted]",
    );
    b.heading(2, "Agenda");
    let mut n = 1;
    let mut agenda: Vec<String> = Vec::new();
    agenda.push(format!(
        "{n}. Call to order. [CHAIR: announce any recording of the meeting and the name of each member taking part remotely.]"
    ));
    n += 1;
    for t in topics {
        let mut line = format!("{n}. {}", t.title);
        if let Some(d) = &t.detail {
            line.push_str(&format!(" - {d}"));
        }
        let mut extra = Vec::new();
        if let Some(p) = &t.presenter_role {
            extra.push(format!("presented by {p}"));
        }
        if let Some(mn) = t.minutes {
            extra.push(format!("about {mn} minutes"));
        }
        if t.action_requested.unwrap_or(false) {
            extra.push("vote requested".into());
        }
        if !extra.is_empty() {
            line.push_str(&format!(" ({})", extra.join("; ")));
        }
        agenda.push(line);
        n += 1;
    }
    if !exec.is_empty() {
        let nums: Vec<String> = exec.iter().map(|p| p.number.to_string()).collect();
        agenda.push(format!(
            "{n}. Executive session under M.G.L. c. 30A, s. 21(a), purpose {}. [CHAIR: before the vote, state the purpose and every subject that may be revealed without defeating it; state whether the open session will reconvene.]",
            nums.join(", ")
        ));
        n += 1;
    }
    agenda.push(format!("{n}. Adjournment."));
    b.list(&agenda);
    b.para("This list includes the topics the chair reasonably anticipates will be discussed. Topics that come up later and could not reasonably have been anticipated 48 hours ahead may still be taken up.");

    // Staff pages: not for posting.
    b.blocks.push(Block::PageBreak);
    b.heading(1, "For staff: not for posting");
    b.heading(2, "Notice deadline");
    match &notice.post_by {
        Some(p) => {
            b.labeled("Post the notice by:", p.clone());
            let days: Vec<String> = notice
                .excluded_days_passed
                .iter()
                .map(|e| format!("{} {}: {}", e.weekday, e.date, e.reason))
                .collect();
            if !days.is_empty() {
                b.para("Days left out of the 48-hour count:");
                b.list(&days);
            }
        }
        None => {
            b.para("Emergency: post notice as soon as reasonably possible before the meeting.");
        }
    }
    b.para(notice.method.clone());
    b.heading(2, "Notice checklist (Attorney General, November 2024)");
    let contents: Vec<String> = c
        .notice
        .contents
        .iter()
        .map(|x| format!("[ ] {}", x.text))
        .collect();
    b.list(&contents);
    b.heading(2, "Chair's responsibilities");
    let duties: Vec<String> = c
        .chair_duties
        .iter()
        .map(|d| format!("[ ] {}", d.text))
        .collect();
    b.list(&duties);
    if !exec.is_empty() {
        b.heading(2, "Executive session script");
        b.list(&c.executive_session.steps);
        let rows: Vec<Vec<String>> = exec
            .iter()
            .map(|p| {
                vec![
                    p.number.to_string(),
                    p.summary.clone(),
                    if p.chair_must_declare_detrimental_effect {
                        "yes"
                    } else {
                        "no"
                    }
                    .to_string(),
                    if p.special_requirements.is_empty() {
                        "-".into()
                    } else {
                        p.special_requirements.clone()
                    },
                ]
            })
            .collect();
        b.table(
            "Executive session purposes",
            &[
                "Purpose",
                "What it covers",
                "Chair declares detrimental effect",
                "Special requirements",
            ],
            rows,
        );
        b.para(c.executive_session.only_for_listed_purposes.clone());
    }
    b.para("Decision support from the Open Meeting Law (M.G.L. c. 30A, ss. 18-25) and 940 CMR 29.00 as read on 2026-11-12; not legal advice. Check with town counsel or the Attorney General's Division of Open Government (617-963-2540).");
    Ok(b.finish(ctx, &format!("{body} meeting agenda"), None))
}

fn report_document(
    ctx: &Context,
    args: &BoardMeetingArgs,
) -> Result<(phn_docs::model::Document, ReportResult)> {
    let body = body_name(args)?;
    let title = clean_opt("report_title", &args.report_title, 150)?
        .unwrap_or_else(|| "Staff report".into());
    let given = args.report_sections.clone().unwrap_or_default();
    if given.len() > 12 {
        return Err(PhnError::InvalidInput("at most 12 report sections".into()));
    }
    let mut taken: BTreeSet<String> = BTreeSet::new();
    let mut custom: Vec<(String, String)> = Vec::new();
    for s in &given {
        let h = clean("a report section heading", &s.heading, 100)?;
        if !taken.insert(h.to_lowercase()) {
            return Err(PhnError::InvalidInput(format!(
                "section '{h}' is given twice"
            )));
        }
        let text = s.text.trim();
        if text.is_empty() {
            return Err(PhnError::InvalidInput(format!("section '{h}' has no text")));
        }
        if text.chars().count() > 6000 {
            return Err(PhnError::InvalidInput(format!(
                "section '{h}' is longer than 6000 characters"
            )));
        }
        if !phi::scan_text(text).is_empty() {
            return Err(PhnError::Privacy(format!(
                "section '{h}' appears to contain an identifier; board materials are public records, so use aggregate figures and leave out names and identifiers"
            )));
        }
        custom.push((h, text.to_string()));
    }
    let defaults: [(&str, &str); 6] = [
        (
            "Purpose and action requested",
            "[NURSE: what the board is asked to do (receive the report, vote on a recommendation, give direction)]",
        ),
        (
            "Background",
            "[NURSE: why the item is before the board; the regulation or program that applies]",
        ),
        (
            "Findings and data",
            "[NURSE: aggregate figures only, with small numbers suppressed; cite the source and date of each figure. Leave out names and anything that could identify a resident or client.]",
        ),
        (
            "Options and recommendation",
            "[NURSE: options considered and the recommendation]",
        ),
        (
            "Fiscal and staffing impact",
            "[NURSE: cost, funding source, and staff time, or write 'none']",
        ),
        (
            "Attachments",
            "[NURSE: list each document the board will use at the meeting. Documents used at a meeting are part of the official record and are listed in the minutes.]",
        ),
    ];
    let mut b = DocBuilder::new();
    b.labeled("To:", body.clone());
    b.labeled(
        "Meeting date:",
        match &args.meeting_date {
            Some(d) => oml::parse_date("meeting_date", d)?.to_string(),
            None => "[NURSE: meeting date]".into(),
        },
    );
    b.labeled("From:", "[NURSE: name and title]");
    b.labeled("Date prepared:", "[NURSE: date]");
    let mut drafted = 0usize;
    let mut placeholder = 0usize;
    let mut used: BTreeSet<usize> = BTreeSet::new();
    for (h, ph) in defaults {
        b.heading(2, h);
        if let Some(k) = custom.iter().position(|(ch, _)| ch.eq_ignore_ascii_case(h)) {
            used.insert(k);
            for line in custom[k].1.lines().filter(|l| !l.trim().is_empty()) {
                b.para(line.trim());
            }
            drafted += 1;
        } else {
            b.para(ph);
            placeholder += 1;
        }
    }
    for (k, (h, text)) in custom.iter().enumerate() {
        if used.contains(&k) {
            continue;
        }
        b.heading(2, h.clone());
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            b.para(line.trim());
        }
        drafted += 1;
    }
    b.para("Board materials are public records. Do not attach anything that identifies a resident or client.");
    Ok((
        b.finish(ctx, &title, Some(format!("Report to the {body}"))),
        ReportResult {
            sections_drafted: drafted,
            sections_placeholder: placeholder,
        },
    ))
}

// ---------------------------------------------------------------- minutes

#[derive(Default)]
struct Item {
    title: Option<String>,
    discussion: Vec<String>,
    motions: Vec<String>,
    votes: Vec<String>,
    documents: Vec<String>,
}

#[derive(Default)]
struct Parsed {
    lines: usize,
    date: Option<String>,
    place: Option<String>,
    called: Option<String>,
    adjourned: Option<String>,
    present: Vec<String>,
    absent: Vec<String>,
    remote: Vec<String>,
    items: Vec<Item>,
    exec: Vec<String>,
    exec_votes: Vec<String>,
}

fn label_of(line: &str) -> Option<(String, String)> {
    let (l, rest) = line.split_once(':')?;
    let key = l.trim().to_lowercase();
    if key.is_empty() || key.chars().count() > 40 {
        return None;
    }
    Some((key, rest.trim().to_string()))
}

fn parse_notes(text: &str) -> Parsed {
    let mut p = Parsed::default();
    let mut cur: Option<usize> = None;
    let mut in_exec = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        p.lines += 1;
        let new_item = |p: &mut Parsed, title: Option<String>| {
            p.items.push(Item {
                title,
                ..Item::default()
            });
            p.items.len() - 1
        };
        if let Some(h) = line.strip_prefix("## ").or_else(|| line.strip_prefix("# ")) {
            in_exec = false;
            cur = Some(new_item(&mut p, Some(h.trim().to_string())));
            continue;
        }
        let Some((key, rest)) = label_of(line) else {
            let i = *cur.get_or_insert_with(|| new_item(&mut p, None));
            p.items[i].discussion.push(line.to_string());
            continue;
        };
        let item = |p: &mut Parsed, cur: &mut Option<usize>| -> usize {
            *cur.get_or_insert_with(|| new_item(p, None))
        };
        match key.as_str() {
            "date" | "meeting date" => p.date = Some(rest),
            "place" | "location" => p.place = Some(rest),
            "called to order" | "time called to order" | "call to order" => p.called = Some(rest),
            "adjourned" | "adjournment" | "time adjourned" => p.adjourned = Some(rest),
            "present" | "members present" => p.present.push(rest),
            "absent" | "members absent" => p.absent.push(rest),
            "remote" | "remote participants" | "participating remotely" => p.remote.push(rest),
            "item" | "topic" | "agenda item" => {
                in_exec = false;
                cur = Some(new_item(&mut p, Some(rest)));
            }
            "executive session" => {
                in_exec = true;
                p.exec.push(rest);
            }
            "motion" => {
                if in_exec {
                    p.exec.push(format!("Motion: {rest}"));
                } else {
                    let i = item(&mut p, &mut cur);
                    p.items[i].motions.push(rest);
                }
            }
            "second" | "seconded" => {
                if in_exec {
                    p.exec.push(format!("Second: {rest}"));
                } else {
                    let i = item(&mut p, &mut cur);
                    p.items[i].motions.push(format!("Seconded: {rest}"));
                }
            }
            "vote" | "roll call" | "roll call vote" => {
                let v = if key.starts_with("roll call") {
                    format!("Roll call: {rest}")
                } else {
                    rest
                };
                if in_exec {
                    p.exec_votes.push(v);
                } else {
                    let i = item(&mut p, &mut cur);
                    p.items[i].votes.push(v);
                }
            }
            "exhibit" | "exhibits" | "document" | "documents" => {
                let i = item(&mut p, &mut cur);
                p.items[i].documents.push(rest);
            }
            _ => {
                let i = item(&mut p, &mut cur);
                if in_exec {
                    p.exec.push(line.to_string());
                } else {
                    p.items[i].discussion.push(line.to_string());
                }
            }
        }
    }
    p
}

fn is_roll_call(v: &str) -> bool {
    let l = v.to_lowercase();
    if l.contains("roll call") {
        return true;
    }
    let words: Vec<&str> = l
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| !w.is_empty())
        .collect();
    words
        .iter()
        .filter(|w| matches!(**w, "yes" | "no" | "aye" | "nay" | "abstain" | "abstained"))
        .count()
        >= 2
}

fn read_notes(ctx: &Context, args: &BoardMeetingArgs) -> Result<(String, Option<Source>)> {
    match (&args.notes_path, &args.notes_text) {
        (Some(_), Some(_)) => Err(PhnError::InvalidInput(
            "give notes_path or notes_text, not both".into(),
        )),
        (None, None) => Err(PhnError::InvalidInput(
            "give notes_path (a .txt or .md file in /data) or notes_text".into(),
        )),
        (None, Some(t)) => {
            if t.chars().count() > MAX_NOTES_CHARS {
                return Err(PhnError::InvalidInput(format!(
                    "notes_text is longer than {MAX_NOTES_CHARS} characters; save it as a file and give notes_path"
                )));
            }
            Ok((t.clone(), None))
        }
        (Some(p), None) => {
            let sb = ctx.sandbox()?;
            let path = sb.resolve_existing(p)?;
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !matches!(ext.as_str(), "txt" | "md" | "text" | "markdown") {
                return Err(PhnError::InvalidInput(
                    "notes_path must be a .txt or .md file".into(),
                ));
            }
            let meta = std::fs::metadata(&path)?;
            if meta.len() > 1_000_000 {
                return Err(PhnError::InvalidInput(
                    "the notes file is larger than 1 MB".into(),
                ));
            }
            let text = std::fs::read_to_string(&path)
                .map_err(|e| PhnError::InvalidInput(format!("cannot read the notes file ({e})")))?;
            Ok((text, Some(Source::local_file(sb.display_path(&path)))))
        }
    }
}

fn line_numbers(text: &str, findings: &[phi::TextFinding]) -> Vec<usize> {
    let mut v: Vec<usize> = findings
        .iter()
        .filter_map(|f| text.get(..f.start))
        .map(|pre| pre.matches('\n').count() + 1)
        .collect();
    v.sort_unstable();
    v.dedup();
    v.truncate(50);
    v
}

fn minutes_document(
    ctx: &Context,
    args: &BoardMeetingArgs,
    p: &Parsed,
    meeting_date: Option<NaiveDate>,
    id_lines: &[usize],
) -> Result<phn_docs::model::Document> {
    let body = body_name(args)?;
    let mut b = DocBuilder::new();
    b.para("DRAFT minutes prepared from the notes. Edit each discussion note into a summary a person who was not there could follow, confirm every vote, and approve the minutes at a meeting of the body. Open session minutes are public records, and so are the notes used to prepare them.");
    if !id_lines.is_empty() {
        b.para(format!(
            "CHECK BEFORE RELEASE: the PHI detector saw possible identifiers on notes line(s) {}. Review and remove or justify them before these minutes are approved or released.",
            id_lines.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
        ));
    }
    let or = |v: &Option<String>, ph: &str| {
        v.clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| ph.to_string())
    };
    let date = match (&args.meeting_date, meeting_date) {
        (_, Some(d)) => oml::show(d.and_hms_opt(0, 0, 0).expect("midnight"))
            .split(" at ")
            .next()
            .unwrap_or_default()
            .to_string(),
        _ => or(&p.date, "[NURSE: meeting date]"),
    };
    b.labeled("Date:", date);
    b.labeled(
        "Time called to order:",
        or(
            &p.called.clone().or_else(|| args.meeting_time.clone()),
            "[NURSE: time]",
        ),
    );
    b.labeled(
        "Place:",
        or(
            &p.place.clone().or_else(|| args.location.clone()),
            "[NURSE: place]",
        ),
    );
    let join = |v: &[String]| {
        v.iter()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join("; ")
    };
    b.labeled(
        "Members present:",
        if p.present.is_empty() {
            "[NURSE: members present]".into()
        } else {
            join(&p.present)
        },
    );
    b.labeled(
        "Members absent:",
        if p.absent.is_empty() {
            "[NURSE: members absent, or none]".into()
        } else {
            join(&p.absent)
        },
    );
    if !p.remote.is_empty() {
        b.labeled("Members participating remotely:", join(&p.remote));
        b.para("All votes were taken by roll call because members participated remotely (940 CMR 29.10(7)).");
    }
    let mut n = 1;
    let mut docs: Vec<String> = Vec::new();
    for it in &p.items {
        b.heading(
            2,
            format!(
                "{n}. {}",
                it.title
                    .clone()
                    .unwrap_or_else(|| "[NURSE: agenda item]".into())
            ),
        );
        if it.discussion.is_empty() {
            b.para("[NURSE: summary of the discussion]");
        } else {
            b.para("Discussion (from your notes; edit into a summary):");
            b.list(&it.discussion);
        }
        if !it.motions.is_empty() {
            b.para("Motions:");
            b.list(&it.motions);
        }
        if it.votes.is_empty() {
            b.para(
                "Vote or action taken: [NURSE: decision, motion, and vote, or write 'no action']",
            );
        } else {
            b.para("Votes (record every vote; a roll-call vote is recorded by roll call even when unanimous):");
            b.list(&it.votes);
        }
        docs.extend(it.documents.iter().cloned());
        n += 1;
    }
    if p.items.is_empty() {
        b.heading(2, "1. [NURSE: agenda item]");
        b.para("[NURSE: summary of the discussion, decisions, and votes]");
    }
    if !p.exec.is_empty() || !p.exec_votes.is_empty() {
        b.heading(2, "Executive session");
        b.para("Executive session minutes need the same detail as open session minutes; every vote is by roll call. Keep them separate from the open session minutes.");
        b.list(&p.exec);
        if p.exec_votes.is_empty() {
            b.para("[NURSE: roll-call vote to enter executive session, purpose stated by the chair, and whether the open session reconvened]");
        } else {
            b.para("Roll-call votes:");
            b.list(&p.exec_votes);
        }
    }
    b.heading(2, "Documents and exhibits used at the meeting");
    if docs.is_empty() {
        b.para("[NURSE: list each document or exhibit used at the meeting, or write 'none']");
    } else {
        b.list(&docs);
    }
    b.heading(2, "Adjournment");
    b.para(or(&p.adjourned, "[NURSE: time adjourned]"));
    b.para("Respectfully submitted, [NURSE: name and title]");
    b.para("Approved by the body on: [CHAIR: date approved]");
    Ok(b.finish(ctx, &format!("{body} minutes"), Some("Draft".into())))
}

fn minutes_result(
    args: &BoardMeetingArgs,
    p: &Parsed,
    findings: &[phi::TextFinding],
    id_lines: Vec<usize>,
    c: &OmlContent,
) -> Result<(MinutesResult, Option<NaiveDate>)> {
    let date = match &args.meeting_date {
        Some(d) => Some(oml::parse_date("meeting_date", d)?),
        None => p
            .date
            .as_deref()
            .and_then(|d| NaiveDate::parse_from_str(d.trim(), "%Y-%m-%d").ok())
            .filter(|d| (1900..=2100).contains(&d.year())),
    };
    let votes: Vec<&String> = p.items.iter().flat_map(|i| i.votes.iter()).collect();
    let docs_n: usize = p.items.iter().map(|i| i.documents.len()).sum();
    let without: usize = p.items.iter().filter(|i| i.discussion.is_empty()).count();
    let remote = !p.remote.is_empty();
    let exec = !p.exec.is_empty() || !p.exec_votes.is_empty();
    let part = |name: &str, found: bool, detail: &str| MinutesPart {
        part: name.into(),
        status: if found { "found" } else { "missing" }.into(),
        detail: detail.into(),
    };
    let mut parts = vec![
        part(
            "Date",
            date.is_some() || p.date.is_some(),
            "G.L. c. 30A, s. 22(a): date of the meeting",
        ),
        part(
            "Time",
            p.called.is_some() || args.meeting_time.is_some(),
            "s. 22(a): time of the meeting",
        ),
        part(
            "Place",
            p.place.is_some() || args.location.is_some(),
            "s. 22(a): place of the meeting",
        ),
        part(
            "Members present",
            !p.present.is_empty(),
            "s. 22(a): members present",
        ),
        part(
            "Members absent",
            !p.absent.is_empty(),
            "s. 22(a): members absent (write 'none' if none)",
        ),
        part(
            "Discussion summary for each subject",
            !p.items.is_empty() && without == 0,
            "s. 22(a): a summary of the discussion of each subject; a statement that a topic was discussed is not enough",
        ),
        part(
            "Decisions and votes",
            !votes.is_empty() || p.items.iter().any(|i| !i.motions.is_empty()),
            "s. 22(a): decisions made, actions taken, and the record of all votes",
        ),
        part(
            "Documents and exhibits",
            docs_n > 0,
            "s. 22(a): a list of documents and exhibits used at the meeting",
        ),
    ];
    if remote {
        let all_roll = votes.iter().all(|v| is_roll_call(v));
        parts.push(MinutesPart {
            part: "Remote members named; votes by roll call".into(),
            status: if all_roll { "found" } else { "check" }.into(),
            detail: "940 CMR 29.10(7): the minutes name each member participating remotely, and every vote is by roll call, even when unanimous".into(),
        });
    } else {
        parts.push(MinutesPart {
            part: "Remote participation".into(),
            status: "not_needed".into(),
            detail: "No remote members were named in the notes (use a 'Remote:' line if any member took part remotely).".into(),
        });
    }
    if exec {
        let all_roll = p.exec_votes.iter().all(|v| is_roll_call(v)) && !p.exec_votes.is_empty();
        parts.push(MinutesPart {
            part: "Executive session recorded; votes by roll call".into(),
            status: if all_roll { "found" } else { "check" }.into(),
            detail: "s. 21(b) and s. 22(b): the vote to enter executive session and every vote in it are by roll call and entered in the minutes; the chair states the purpose and whether open session reconvenes. Executive session minutes are kept and reviewed at reasonable intervals.".into(),
        });
    }
    Ok((
        MinutesResult {
            notes_lines: p.lines,
            items: p.items.len(),
            items_without_discussion: without,
            votes_recorded: votes.len() + p.exec_votes.len(),
            documents_listed: docs_n,
            executive_session_in_notes: exec,
            remote_members_in_notes: remote,
            parts,
            identifier_categories: {
                let mut v: Vec<String> = findings
                    .iter()
                    .map(|f| f.kind.label().to_string())
                    .collect();
                v.sort();
                v.dedup();
                v
            },
            identifier_lines: id_lines,
            approve_by_30_days: date
                .map(|d| (d + Duration::days(c.minutes.approval.days_to_approve)).to_string()),
            release_on_request_days: c.minutes.approval.days_to_release_on_request,
        },
        date,
    ))
}

// ---------------------------------------------------------------- guide

fn guide(c: &OmlContent, h: &HolidayContent) -> OmlGuide {
    OmlGuide {
        public_body: c.public_body.text.clone(),
        notice_rule: c.notice.rule_text.clone(),
        notice_contents: c.notice.contents.iter().map(|x| x.text.clone()).collect(),
        common_questions: c
            .notice
            .common_questions
            .iter()
            .map(|q| format!("{} {}", q.question, q.answer))
            .collect(),
        executive_session_purposes: c
            .executive_session
            .purposes
            .iter()
            .map(|p| ExecPurpose {
                number: p.number,
                summary: p.summary.clone(),
                chair_must_declare_detrimental_effect: p.chair_declaration_required,
                special_requirements: p.special_requirements.clone(),
            })
            .collect(),
        executive_session_steps: c.executive_session.steps.clone(),
        minutes_required: c
            .minutes
            .required_elements
            .iter()
            .map(|e| e.text.clone())
            .collect(),
        minutes_conditional: c
            .minutes
            .conditional
            .iter()
            .map(|x| format!("{}: {}", x.when, x.text))
            .collect(),
        minutes_approval: c.minutes.approval.text.clone(),
        chair_duties: c.chair_duties.iter().map(|d| d.text.clone()).collect(),
        remote_participation: c.remote_participation.text.clone(),
        legal_holidays: h
            .holidays
            .iter()
            .map(|r| HolidayOut {
                date: r.date.clone(),
                weekday: r.weekday.clone(),
                name: r.name.clone(),
                scope: r.scope.clone(),
            })
            .collect(),
    }
}

// ---------------------------------------------------------------- the tool

pub fn board_of_health_meeting_assistant(
    ctx: &Context,
    args: BoardMeetingArgs,
) -> Result<Envelope<BoardMeetingResult>> {
    let action = args.action.ok_or_else(|| {
        PhnError::InvalidInput(
            "action is required: notice_check, agenda, staff_report, minutes, or oml_guide".into(),
        )
    })?;
    let c = OmlContent::load()?;
    let h = HolidayContent::load()?;
    let mut result = BoardMeetingResult {
        action: format!("{action:?}"),
        notice: None,
        agenda: None,
        staff_report: None,
        minutes: None,
        oml_guide: None,
        document_file: None,
    };
    let mut files: Vec<String> = Vec::new();
    let mut sources: Vec<Source> = Vec::new();
    let mut identifier_categories: Vec<String> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let write = args.write_document.unwrap_or(true);
    let summary: String;

    match action {
        BoardAction::NoticeCheck | BoardAction::Agenda => {
            let m = meeting_of(&args)?;
            let topics = topics_of(&args)?;
            let exec = exec_purposes(
                &c,
                args.executive_session_purposes.as_deref().unwrap_or(&[]),
            )?;
            let (suffolk, why) = is_suffolk(ctx, &args)?;
            let inputs = NoticeInputs {
                args: &args,
                topics: &topics,
                suffolk,
                exec: &exec,
            };
            let notice = notice_result(why.as_deref().unwrap_or(""), &inputs, &m)?;
            let mut s = notice_summary(&notice);
            if action == BoardAction::Agenda {
                result.agenda = Some(AgendaResult {
                    topics: topics.len(),
                    executive_session_purposes: exec.clone(),
                    planned_minutes: {
                        let t: u32 = topics.iter().filter_map(|t| t.minutes).sum();
                        (t > 0).then_some(t)
                    },
                });
                if write {
                    let doc = agenda_document(ctx, &c, &args, &m, &notice, &topics, &exec)?;
                    let (_, w) = write_documents(
                        ctx,
                        "board_meeting",
                        "board_agenda",
                        &doc,
                        &[OutFormat::Docx],
                        None,
                    )?;
                    if let Some(f) = w.into_iter().next() {
                        s.push_str(&format!("\nAgenda and notice draft (with staff pages to remove before posting): {f}\n"));
                        files.push(f.clone());
                        result.document_file = Some(f);
                    }
                }
            }
            summary = s;
            result.notice = Some(notice);
            sources.extend(wc::cite(
                &[
                    "statute_20",
                    "regs_940_cmr_29",
                    "ag_notice_checklist",
                    "ag_faq",
                    "ag_chair_checklist",
                    "ag_exec_checklist",
                ],
                &c.sources,
            ));
            sources.extend(wc::cite(&["statute", "trial_court"], &h.sources));
            sources.push(Source::computed(
                "48-hour notice calculation (excluding Saturdays, Sundays, and legal holidays)",
                "Calculated by this server",
            ));
        }
        BoardAction::StaffReport => {
            let (doc, r) = report_document(ctx, &args)?;
            let mut s = format!(
                "**Staff report template:** {} section(s) drafted by you, {} placeholder(s) for you to complete.\n",
                r.sections_drafted, r.sections_placeholder
            );
            if write {
                let (_, w) = write_documents(
                    ctx,
                    "board_meeting",
                    "staff_report",
                    &doc,
                    &[OutFormat::Docx],
                    None,
                )?;
                if let Some(f) = w.into_iter().next() {
                    s.push_str(&format!("\nDraft: {f}\n"));
                    files.push(f.clone());
                    result.document_file = Some(f);
                }
            }
            summary = s;
            result.staff_report = Some(r);
            sources.extend(wc::cite(&["ag_minutes_checklist"], &c.sources));
            sources.push(Source::computed(
                "Staff report template",
                "Prepared by this server from your inputs",
            ));
        }
        BoardAction::Minutes => {
            let (text, src) = read_notes(ctx, &args)?;
            if let Some(s) = src {
                sources.push(s);
            }
            let findings = phi::scan_text(&text);
            let id_lines = line_numbers(&text, &findings);
            let parsed = parse_notes(&text);
            if parsed.lines == 0 {
                return Err(PhnError::InvalidInput("the notes are empty".into()));
            }
            let (mr, date) = minutes_result(&args, &parsed, &findings, id_lines.clone(), &c)?;
            identifier_categories = mr.identifier_categories.clone();
            let mut s = minutes_summary(&mr);
            if write {
                let doc = minutes_document(ctx, &args, &parsed, date, &id_lines)?;
                let (_, w) = write_documents(
                    ctx,
                    "board_meeting",
                    "board_minutes_draft",
                    &doc,
                    &[OutFormat::Docx],
                    None,
                )?;
                if let Some(f) = w.into_iter().next() {
                    s.push_str(&format!("\nDraft minutes: {f}\n"));
                    files.push(f.clone());
                    result.document_file = Some(f);
                }
            }
            notes.push("Your notes were read locally and are not returned; only counts, the required parts found, and the line numbers of possible identifiers are.".into());
            summary = s;
            result.minutes = Some(mr);
            sources.extend(wc::cite(
                &[
                    "statute_22",
                    "regs_940_cmr_29",
                    "ag_minutes_checklist",
                    "ag_exec_checklist",
                ],
                &c.sources,
            ));
        }
        BoardAction::OmlGuide => {
            let g = guide(&c, &h);
            let mut s =
                String::from("**Open Meeting Law requirements for a local board of health**\n\n");
            s.push_str(&format!("- Public body: {}\n", g.public_body));
            s.push_str(&format!("- Notice: {}\n", g.notice_rule));
            s.push_str("\n**Notice contents**\n\n");
            for x in &g.notice_contents {
                s.push_str(&format!("- {x}\n"));
            }
            s.push_str("\n**Executive session purposes (c. 30A, s. 21(a))**\n\n");
            let rows: Vec<Vec<String>> = g
                .executive_session_purposes
                .iter()
                .map(|p| vec![p.number.to_string(), p.summary.clone()])
                .collect();
            s.push_str(&md::table(&["#", "Purpose"], &rows));
            s.push_str("\n**Minutes**\n\n");
            for x in &g.minutes_required {
                s.push_str(&format!("- {x}\n"));
            }
            s.push_str(&format!("\n{}\n", g.minutes_approval));
            if let Some(d) = &args.meeting_date
                && args.meeting_time.is_some()
            {
                let m = meeting_of(&args)?;
                let _ = d;
                let (suffolk, _) = is_suffolk(ctx, &args)?;
                let (dl, _) = Calendar::load(suffolk)?.posting_deadline(m.dt)?;
                s.push_str(&format!(
                    "\nFor a meeting {}: post notice by {}.\n",
                    oml::show(m.dt),
                    oml::show(dl)
                ));
            }
            summary = s;
            result.oml_guide = Some(g);
            sources.extend(wc::cite(
                &[
                    "statute_18",
                    "statute_20",
                    "statute_21",
                    "statute_22",
                    "regs_940_cmr_29",
                    "ag_guide",
                    "ag_notice_checklist",
                    "ag_minutes_checklist",
                    "ag_exec_checklist",
                    "ag_chair_checklist",
                ],
                &c.sources,
            ));
            sources.extend(wc::cite(&["statute", "trial_court"], &h.sources));
        }
    }

    let mut caveats = vec![
        "Decision support, not legal advice. This tool applies the Open Meeting Law (M.G.L. c. 30A, ss. 18-25) and the Attorney General's regulations (940 CMR 29.00) as read on 2026-11-12. For a question about your own board, ask town counsel or the Attorney General's Division of Open Government (617-963-2540, openmeeting@mass.gov).".to_string(),
        "The 48-hour count leaves out whole Saturdays, Sundays, and legal holidays (this project's reading; no Attorney General worked example was found). Your municipal clerk must have the notice early enough to post it by the deadline, and other laws (hearings, permits) may require longer or published notice.".into(),
        "The bundled legal holiday list covers 2026-2028 (M.G.L. c. 4, s. 7, clause Eighteenth); 2028 and the two Suffolk-county-only days have no second published list. A holiday that falls on a Saturday is not moved; one that falls on a Sunday is observed Monday.".into(),
        "Remote participation: the provisions in Chapter 20 of the Acts of 2021 that allow remote-only public access expire on June 30, 2027 unless extended; this tool does not decide whether remote participation is allowed.".into(),
    ];
    caveats.extend(wc::caveats(&[wc::OML, wc::HOLIDAYS]));
    notes.push("Board materials and open session minutes are public records; leave out names and identifiers of residents and clients.".into());
    let privacy = Privacy {
        row_level_data: false,
        suppression_applied: false,
        suppression_threshold: None,
        identifiers_detected: identifier_categories,
        files_written: files,
        notes,
    };
    Ok(Envelope::new(summary, result)
        .decision_support()
        .sources(sources)
        .caveats(caveats)
        .privacy(privacy))
}

fn notice_summary(n: &NoticeResult) -> String {
    let mut s = format!("**Meeting:** {}.\n\n", n.meeting);
    match &n.post_by {
        Some(p) => s.push_str(&format!("**Post the notice by {p}** (48 hours before the meeting, leaving out Saturdays, Sundays, and legal holidays).\n")),
        None => s.push_str("**Emergency:** post notice as soon as reasonably possible.\n"),
    }
    if !n.excluded_days_passed.is_empty() {
        s.push_str("\nDays left out of the count: ");
        s.push_str(
            &n.excluded_days_passed
                .iter()
                .map(|e| format!("{} {} ({})", e.weekday, e.date, e.reason))
                .collect::<Vec<_>>()
                .join("; "),
        );
        s.push_str(".\n");
    }
    if let Some(p) = &n.posted_check {
        s.push_str(&format!(
            "\nPosted {}: {} countable hours; {}.\n",
            p.posted_at,
            p.countable_hours,
            if p.compliant { "enough" } else { "NOT enough" }
        ));
    }
    s.push_str("\n**Checks**\n\n");
    let rows: Vec<Vec<String>> = n
        .checks
        .iter()
        .map(|c| vec![c.status.clone(), c.message.clone()])
        .collect();
    s.push_str(&md::table(&["Status", "Check"], &rows));
    s
}

fn minutes_summary(m: &MinutesResult) -> String {
    let mut s = format!(
        "**Draft minutes from {} lines of notes:** {} agenda item(s), {} vote(s) recorded, {} document(s) listed.\n\n",
        m.notes_lines, m.items, m.votes_recorded, m.documents_listed
    );
    let rows: Vec<Vec<String>> = m
        .parts
        .iter()
        .map(|p| vec![p.part.clone(), p.status.clone(), p.detail.clone()])
        .collect();
    s.push_str(&md::table(&["Required part", "Status", "Basis"], &rows));
    if !m.identifier_lines.is_empty() {
        s.push_str(&format!(
            "\nThe PHI detector saw possible identifiers ({}) on notes line(s) {}. Review them before the minutes are approved or released.\n",
            m.identifier_categories.join(", "),
            m.identifier_lines.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
        ));
    }
    if let Some(d) = &m.approve_by_30_days {
        s.push_str(&format!(
            "\nApprove the open session minutes in a timely manner: generally within the next three meetings or 30 days (by {d}), whichever is later. Draft or approved, open session minutes must be given to anyone who asks within {} days.\n",
            m.release_on_request_days
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_are_sorted_into_the_minutes_parts() {
        let notes = "Date: 2026-12-08\nCalled to order: 6:02 pm\nPlace: Town Hall\nPresent: A, B, C\nAbsent: none\n\
            Item: Flu clinic schedule\nThe nurse described the clinic dates.\nMotion: approve the schedule\nSecond: B\nVote: roll call A yes B yes C yes\nExhibit: Clinic schedule\n\
            Item: Budget\nDiscussed the grant.\nExecutive session: purpose 3\nRoll call: A yes B yes C yes\nAdjourned: 7:30 pm";
        let p = parse_notes(notes);
        assert_eq!(p.items.len(), 2);
        assert_eq!(p.items[0].votes.len(), 1);
        assert_eq!(p.items[0].documents.len(), 1);
        assert_eq!(p.present.len(), 1);
        assert!(!p.exec.is_empty());
        assert_eq!(p.exec_votes.len(), 1);
        assert!(is_roll_call(&p.items[0].votes[0]));
        assert!(!is_roll_call("3-0"));
    }

    #[test]
    fn general_topics_are_flagged() {
        assert!(topic_is_general("New business"));
        assert!(topic_is_general("Updates"));
        assert!(!topic_is_general(
            "Seasonal influenza clinic schedule and staffing"
        ));
    }

    #[test]
    fn identifier_lines_are_counted_from_one_and_never_quote_text() {
        let text = "first line\nsecond line has 413-555-0100 and ann.lee@example.org\nthird";
        let f = phi::scan_text(text);
        let lines = line_numbers(text, &f);
        assert_eq!(lines, vec![2]);
    }
}
