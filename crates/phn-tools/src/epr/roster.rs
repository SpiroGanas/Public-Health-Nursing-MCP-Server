//! Staff roster for the Phase 13 preparedness documents (ICS forms, COOP
//! plans, exercise rosters).
//!
//! Product-owner decision (Phase 13): staff **names and positions** may pass
//! through chat as tool arguments, but **phone numbers, email addresses, radio
//! call signs, and other contact details** come only from a roster CSV/XLSX in
//! `/data`. Tools fill them into documents written to `/data/output` and never
//! return them. Nothing read from the roster, names included, is returned to
//! the AI: results report counts and recognized column names only.
//!
//! Recognized columns (case-insensitive; `_` and `-` count as spaces): name
//! (or first name + last name), position (ICS position, role, title,
//! assignment), agency (organization, department), work phone (phone, office
//! phone, telephone), cell phone (mobile), email, radio (call sign, channel),
//! and alternate contact (backup phone, home phone).

use phn_core::{PhnError, Result, Source};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;

/// One person on the roster. Values are for local documents only.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StaffContact {
    pub name: String,
    pub position: String,
    pub agency: String,
    pub work_phone: String,
    pub cell_phone: String,
    pub email: String,
    pub radio: String,
    pub alternate_contact: String,
}

impl StaffContact {
    /// Phone and email on one line for a document cell ("work · cell · email").
    pub fn contact_line(&self) -> String {
        [
            self.work_phone.as_str(),
            self.cell_phone.as_str(),
            self.email.as_str(),
        ]
        .iter()
        .filter(|s| !s.trim().is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" · ")
    }

    /// The best single phone number (cell, then work, then alternate).
    pub fn best_phone(&self) -> &str {
        [&self.cell_phone, &self.work_phone, &self.alternate_contact]
            .into_iter()
            .find(|s| !s.trim().is_empty())
            .map(String::as_str)
            .unwrap_or("")
    }
}

/// What a tool may report about the roster it used (counts only, never values).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RosterSummary {
    /// The roster file, relative to /data.
    pub file: String,
    /// Staff rows read.
    pub staff: usize,
    /// Roster fields recognized (field names, not values).
    pub columns_matched: Vec<String>,
    /// People or positions in the documents that were matched to a roster row.
    pub matched: usize,
    /// People or positions with no roster row (their contact cells are left blank to fill by hand).
    pub unmatched: usize,
    /// Rows with at least one phone number.
    pub with_phone: usize,
    /// Rows with an email address.
    pub with_email: usize,
}

/// A loaded staff roster.
#[derive(Debug, Clone)]
pub struct Roster {
    /// Path relative to /data.
    pub label: String,
    pub entries: Vec<StaffContact>,
    pub columns_matched: Vec<String>,
}

const NAME: &[&str] = &[
    "name",
    "full name",
    "staff name",
    "staff",
    "employee",
    "employee name",
    "person",
    "responder",
    "responder name",
];
const FIRST: &[&str] = &["first name", "first", "given name"];
const LAST: &[&str] = &["last name", "last", "surname", "family name"];
const POSITION: &[&str] = &[
    "ics position",
    "position",
    "ics role",
    "role",
    "title",
    "job title",
    "assignment",
    "ics assignment",
];
const AGENCY: &[&str] = &[
    "agency",
    "organization",
    "organisation",
    "department",
    "dept",
    "org",
    "employer",
];
const WORK_PHONE: &[&str] = &[
    "work phone",
    "office phone",
    "phone",
    "phone number",
    "desk phone",
    "telephone",
    "business phone",
];
const CELL: &[&str] = &[
    "cell phone",
    "cell",
    "mobile",
    "mobile phone",
    "cell number",
];
const EMAIL: &[&str] = &["email", "e mail", "email address", "work email"];
const RADIO: &[&str] = &[
    "radio",
    "radio call sign",
    "call sign",
    "callsign",
    "radio channel",
    "channel",
];
const ALTERNATE: &[&str] = &[
    "alternate contact",
    "alternate phone",
    "backup phone",
    "home phone",
    "other phone",
    "pager",
];

fn norm_header(h: &str) -> String {
    h.trim()
        .to_lowercase()
        .replace(['_', '-', '.'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Normalize free text for matching (lower case, single spaces, no punctuation).
fn norm_text(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Expand common ICS and COOP position abbreviations and normalize the text,
/// so "IC", "Incident Cmdr" and "incident commander" all match.
pub fn normalize_position(s: &str) -> String {
    let n = norm_text(s);
    let full = match n.as_str() {
        "ic" | "incident cmdr" | "incident command" => "incident commander",
        "uc" => "unified command",
        "dic" | "deputy ic" => "deputy incident commander",
        "pio" => "public information officer",
        "lno" | "liaison" => "liaison officer",
        "sofr" | "safety" | "so" => "safety officer",
        "osc" | "ops chief" | "operations chief" | "operations" => "operations section chief",
        "psc" | "planning chief" | "planning" => "planning section chief",
        "lsc" | "logistics chief" | "logistics" => "logistics section chief",
        "fsc"
        | "finance chief"
        | "finance"
        | "finance admin section chief"
        | "finance administration section chief" => "finance administration section chief",
        "mul" | "medical unit" => "medical unit leader",
        "sitl" => "situation unit leader",
        "resl" => "resources unit leader",
        "docl" => "documentation unit leader",
        "coml" => "communications unit leader",
        "dmob" => "demobilization unit leader",
        other => other,
    };
    full.to_string()
}

fn find_col(headers: &[String], aliases: &[&str]) -> Option<usize> {
    let normed: Vec<String> = headers.iter().map(|h| norm_header(h)).collect();
    aliases
        .iter()
        .find_map(|a| normed.iter().position(|h| h == a))
}

impl Roster {
    /// Load a roster CSV/XLSX from /data. The file is expected to contain
    /// identifiers (that is its purpose); its values never leave the server
    /// except inside documents written to /data/output.
    pub fn load(ctx: &Context, path: &str, sheet: Option<&str>) -> Result<Roster> {
        let (table, label) = crate::core::files::load_table(ctx, path, sheet)?;
        let h = &table.headers;
        let name = find_col(h, NAME);
        let first = find_col(h, FIRST);
        let last = find_col(h, LAST);
        let position = find_col(h, POSITION);
        let agency = find_col(h, AGENCY);
        let work = find_col(h, WORK_PHONE);
        let cell = find_col(h, CELL);
        let email = find_col(h, EMAIL);
        let radio = find_col(h, RADIO);
        let alt = find_col(h, ALTERNATE);
        if name.is_none() && first.is_none() && last.is_none() && position.is_none() {
            // The file's own header text is never echoed: a roster without a header row would
            // put a person's name or phone number there. Only this module's field names and a
            // count are reported.
            let recognized: Vec<&str> = [
                ("agency", agency),
                ("work phone", work),
                ("cell phone", cell),
                ("email", email),
                ("radio", radio),
                ("alternate contact", alt),
            ]
            .into_iter()
            .filter(|(_, col)| col.is_some())
            .map(|(field, _)| field)
            .collect();
            return Err(PhnError::InvalidInput(format!(
                "'{label}' needs a name column (name, or first name and last name) or a position \
                 column (position, ICS position, role, or title). It has {} column(s); recognized \
                 roster fields: {}. Check that the first row holds the column names.",
                h.len(),
                if recognized.is_empty() {
                    "none".to_string()
                } else {
                    recognized.join(", ")
                }
            )));
        }
        let mut columns_matched = Vec::new();
        for (field, col) in [
            ("name", name.or(first).or(last)),
            ("position", position),
            ("agency", agency),
            ("work phone", work),
            ("cell phone", cell),
            ("email", email),
            ("radio", radio),
            ("alternate contact", alt),
        ] {
            if col.is_some() {
                columns_matched.push(field.to_string());
            }
        }
        let get = |row: &[String], c: Option<usize>| -> String {
            c.and_then(|i| row.get(i))
                .map(|v| v.trim().to_string())
                .unwrap_or_default()
        };
        let entries = table
            .rows
            .iter()
            .map(|row| {
                let full = match name {
                    Some(_) => get(row, name),
                    None => [get(row, first), get(row, last)]
                        .into_iter()
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join(" "),
                };
                StaffContact {
                    name: full,
                    position: get(row, position),
                    agency: get(row, agency),
                    work_phone: get(row, work),
                    cell_phone: get(row, cell),
                    email: get(row, email),
                    radio: get(row, radio),
                    alternate_contact: get(row, alt),
                }
            })
            .filter(|e| !e.name.is_empty() || !e.position.is_empty())
            .collect::<Vec<_>>();
        if entries.is_empty() {
            return Err(PhnError::InvalidInput(format!(
                "'{label}' has no rows with a name or position"
            )));
        }
        Ok(Roster {
            label,
            entries,
            columns_matched,
        })
    }

    /// Citation for the roster file.
    pub fn source(&self) -> Source {
        Source::local_file(&self.label)
    }

    /// Find a person. When a name is given, only an exact full-name match counts
    /// (case and punctuation ignored): a name that is not on the roster returns
    /// `None`, so another person's contact details are never placed beside it.
    /// The position (its first holder) is used only when no name is given.
    pub fn find(&self, name: Option<&str>, position: Option<&str>) -> Option<&StaffContact> {
        if let Some(n) = name.map(norm_text).filter(|n| !n.is_empty()) {
            return self.entries.iter().find(|e| norm_text(&e.name) == n);
        }
        position
            .map(normalize_position)
            .filter(|p| !p.is_empty())
            .and_then(|p| {
                self.entries
                    .iter()
                    .find(|e| normalize_position(&e.position) == p)
            })
    }

    /// Everyone holding a position, in roster order (for orders of succession).
    pub fn holders(&self, position: &str) -> Vec<&StaffContact> {
        let p = normalize_position(position);
        self.entries
            .iter()
            .filter(|e| !p.is_empty() && normalize_position(&e.position) == p)
            .collect()
    }

    /// Summary safe to return (counts only). `matched`/`unmatched` are the
    /// tool's own tallies of lookups that did or did not find a row.
    pub fn summary(&self, matched: usize, unmatched: usize) -> RosterSummary {
        RosterSummary {
            file: self.label.clone(),
            staff: self.entries.len(),
            columns_matched: self.columns_matched.clone(),
            matched,
            unmatched,
            with_phone: self
                .entries
                .iter()
                .filter(|e| !e.best_phone().is_empty())
                .count(),
            with_email: self
                .entries
                .iter()
                .filter(|e| !e.email.trim().is_empty())
                .count(),
        }
    }
}

/// Counts lookups against an optional roster, for [`Roster::summary`].
#[derive(Debug, Default)]
pub struct RosterTally {
    pub matched: usize,
    pub unmatched: usize,
}

impl RosterTally {
    /// Look a person up and count the result. Returns `None` without counting when there is no roster.
    pub fn find<'r>(
        &mut self,
        roster: Option<&'r Roster>,
        name: Option<&str>,
        position: Option<&str>,
    ) -> Option<&'r StaffContact> {
        let r = roster?;
        let hit = r.find(name, position);
        if hit.is_some() {
            self.matched += 1;
        } else {
            self.unmatched += 1;
        }
        hit
    }
}

/// Identifier categories (phone numbers, emails, and other contact details)
/// found in free-text tool arguments. Contacts belong in the roster file; tools
/// report these categories in the privacy block and ask the nurse to move them.
pub fn contacts_in_arguments<'a>(texts: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    use phn_local::phi::{Identifier as I, scan_text};
    let mut found: Vec<String> = texts
        .into_iter()
        .flat_map(scan_text)
        .filter(|f| {
            matches!(
                f.kind,
                I::Phone | I::Fax | I::Email | I::Ssn | I::StreetAddress | I::BirthDate
            )
        })
        .map(|f| f.kind.label().to_string())
        .collect();
    found.sort();
    found.dedup();
    found
}

/// Every non-empty string in a tool's arguments, for [`contacts_in_arguments`],
/// so a phone number typed into any field (a supervisor's name, a resource
/// leader, a message recipient) is noticed. Fields named in `skip_keys` are
/// left out with everything under them: file paths, dates and times, and
/// fields where a facility's or the public's number or address belongs.
pub fn argument_strings<T: Serialize>(args: &T, skip_keys: &[&str]) -> Vec<String> {
    fn walk(v: &serde_json::Value, skip: &[&str], out: &mut Vec<String>) {
        match v {
            serde_json::Value::String(s) => {
                if !s.trim().is_empty() {
                    out.push(s.clone());
                }
            }
            serde_json::Value::Array(items) => {
                for x in items {
                    walk(x, skip, out);
                }
            }
            serde_json::Value::Object(map) => {
                for (k, x) in map {
                    if !skip.contains(&k.as_str()) {
                        walk(x, skip, out);
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    if let Ok(v) = serde_json::to_value(args) {
        walk(&v, skip_keys, &mut out);
    }
    out
}

/// Privacy note for tools that used [`contacts_in_arguments`] and found something.
pub const CONTACTS_IN_ARGUMENTS_NOTE: &str = "Contact details (phone numbers, emails, or \
    addresses) were found in the request. Put staff contacts in a roster file in /data \
    (roster_path) so they go only into the documents written to /data/output.";

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_with(csv: &str) -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        std::fs::write(dir.path().join("data/roster.csv"), csv).unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, ctx)
    }

    #[test]
    fn loads_columns_and_matches_by_name_or_position() {
        let (_d, ctx) = ctx_with(
            "Full_Name,ICS Position,Agency,Cell,E-mail,Call Sign\n\
             Ada Quell,IC,Health,508-555-0101,ada@example.org,HD-1\n\
             Bo Ren,Public Information Officer,Health,,bo@example.org,\n\
             Cy Dunn,Public Information Officer,Town,508-555-0103,,\n",
        );
        let r = Roster::load(&ctx, "roster.csv", None).unwrap();
        assert_eq!(r.entries.len(), 3);
        assert_eq!(
            r.columns_matched,
            vec!["name", "position", "agency", "cell phone", "email", "radio"]
        );
        let ic = r.find(None, Some("Incident Commander")).unwrap();
        assert_eq!(ic.name, "Ada Quell");
        assert_eq!(ic.contact_line(), "508-555-0101 · ada@example.org");
        assert_eq!(
            r.find(Some("bo  REN"), None).unwrap().email,
            "bo@example.org"
        );
        assert_eq!(r.holders("PIO").len(), 2);
        let s = r.summary(2, 1);
        assert_eq!((s.staff, s.with_phone, s.with_email), (3, 2, 2));
        let mut t = RosterTally::default();
        assert!(
            t.find(Some(&r), Some("Nobody"), Some("Safety Officer"))
                .is_none()
        );
        // A name that is not on the roster never borrows the position holder's contacts.
        assert!(
            r.find(Some("A. Quell"), Some("Incident Commander"))
                .is_none()
        );
        assert!(r.find(Some("Nobody"), Some("PIO")).is_none());
        assert_eq!(
            r.find(Some("ada quell"), Some("PIO")).unwrap().name,
            "Ada Quell"
        );
        assert!(t.find(Some(&r), None, Some("pio")).is_some());
        assert_eq!((t.matched, t.unmatched), (1, 1));
        assert!(t.find(None, Some("x"), None).is_none());
        assert_eq!((t.matched, t.unmatched), (1, 1));
    }

    #[test]
    fn first_and_last_name_columns_combine() {
        let (_d, ctx) = ctx_with("First Name,Last Name,Role\nAda,Quell,Safety\n");
        let r = Roster::load(&ctx, "roster.csv", None).unwrap();
        assert_eq!(r.entries[0].name, "Ada Quell");
        assert!(r.find(None, Some("safety officer")).is_some());
    }

    #[test]
    fn errors_name_columns_but_never_values() {
        let (_d, ctx) = ctx_with("Phone,Email\n508-555-0101,ada@example.org\n");
        let e = Roster::load(&ctx, "roster.csv", None)
            .unwrap_err()
            .to_string();
        assert!(e.contains("work phone, email"), "{e}");
        assert!(!e.contains("555-0101") && !e.contains("ada@"), "{e}");
    }

    #[test]
    fn a_roster_without_a_header_row_does_not_echo_its_first_row() {
        // The first row is taken as headers; none of it may come back in the error.
        let (_d, ctx) = ctx_with(
            "Ada Quell,508-555-0101,ada@example.org\nBo Ren,508-555-0102,bo@example.org\n",
        );
        let e = Roster::load(&ctx, "roster.csv", None)
            .unwrap_err()
            .to_string();
        assert!(e.contains("3 column(s)"), "{e}");
        assert!(
            !e.contains("Quell") && !e.contains("555-0101") && !e.contains("ada@"),
            "{e}"
        );
    }

    #[test]
    fn argument_strings_walks_every_field_except_skipped_keys() {
        #[derive(Serialize)]
        struct Inner {
            leader: Option<String>,
            location: String,
        }
        #[derive(Serialize)]
        struct Args {
            roster_path: String,
            people: Vec<Inner>,
            count: u32,
        }
        let a = Args {
            roster_path: "roster 617-555-0123.csv".into(),
            people: vec![Inner {
                leader: Some("Dana Lee 617-555-0123".into()),
                location: "12 Main Street".into(),
            }],
            count: 3,
        };
        let s = argument_strings(&a, &["roster_path", "location"]);
        assert_eq!(s, vec!["Dana Lee 617-555-0123".to_string()]);
        let found = contacts_in_arguments(s.iter().map(String::as_str));
        assert!(found.iter().any(|f| f.contains("phone")), "{found:?}");
    }

    #[test]
    fn finds_contacts_in_arguments() {
        let found = contacts_in_arguments(["Call 508-555-0101 or ada@example.org", "IC: Ada"]);
        assert!(found.iter().any(|f| f.contains("phone")), "{found:?}");
        assert!(found.iter().any(|f| f.contains("email")), "{found:?}");
        assert!(contacts_in_arguments(["Incident Commander: Ada Quell"]).is_empty());
    }
}
