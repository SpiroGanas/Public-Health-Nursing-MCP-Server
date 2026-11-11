//! Detection of possible identifiers (PHI) in tables and free text.
//!
//! Three kinds of evidence are combined:
//!
//! 1. **Column names**: "DOB", "MRN", "Address", "Phone", "SASID", …
//! 2. **Value patterns**: SSNs, phone numbers, email addresses, dates, ZIP
//!    codes, street addresses, URLs, IP addresses, unique ID-like codes.
//! 3. **Public name lists**: U.S. Census surnames and SSA first names, used to
//!    recognize person names in columns and in running text.
//!
//! The detector errs toward flagging. It supports, and does not replace, the
//! nurse's own review; the Safe Harbor method also requires that the agency
//! have no actual knowledge that remaining data could identify a person.

use std::collections::{BTreeMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::table::Table;

/// Kinds of identifiers, aligned with the 18 HIPAA Safe Harbor identifiers.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Identifier {
    Name,
    StreetAddress,
    /// Town, city, county, precinct, census tract, or coordinates.
    Geography,
    ZipCode,
    BirthDate,
    /// Other dates related to a person (onset, visit, admission, death).
    Date,
    /// Age (identifying only above 89).
    Age,
    Phone,
    Fax,
    Email,
    Ssn,
    MedicalRecordNumber,
    HealthPlanNumber,
    AccountNumber,
    LicenseNumber,
    VehicleIdentifier,
    DeviceIdentifier,
    Url,
    IpAddress,
    Biometric,
    Photo,
    /// Any other unique identifying number or code (case IDs, student IDs).
    OtherIdNumber,
    /// Free-text notes that may contain any of the above.
    FreeText,
}

impl Identifier {
    /// Plain-language label.
    pub fn label(self) -> &'static str {
        match self {
            Identifier::Name => "names",
            Identifier::StreetAddress => "street addresses",
            Identifier::Geography => "geography smaller than a state",
            Identifier::ZipCode => "ZIP codes",
            Identifier::BirthDate => "dates of birth",
            Identifier::Date => "dates related to a person",
            Identifier::Age => "ages",
            Identifier::Phone => "phone numbers",
            Identifier::Fax => "fax numbers",
            Identifier::Email => "email addresses",
            Identifier::Ssn => "Social Security numbers",
            Identifier::MedicalRecordNumber => "medical record numbers",
            Identifier::HealthPlanNumber => "health plan numbers",
            Identifier::AccountNumber => "account numbers",
            Identifier::LicenseNumber => "certificate or license numbers",
            Identifier::VehicleIdentifier => "vehicle identifiers",
            Identifier::DeviceIdentifier => "device identifiers",
            Identifier::Url => "web addresses",
            Identifier::IpAddress => "IP addresses",
            Identifier::Biometric => "biometric identifiers",
            Identifier::Photo => "photographs",
            Identifier::OtherIdNumber => "ID numbers or codes",
            Identifier::FreeText => "free text",
        }
    }

    /// The Safe Harbor item, 45 CFR 164.514(b)(2)(i)(A)–(R).
    pub fn safe_harbor_item(self) -> &'static str {
        match self {
            Identifier::Name => "(A) names",
            Identifier::StreetAddress | Identifier::Geography | Identifier::ZipCode => {
                "(B) geographic subdivisions smaller than a state"
            }
            Identifier::BirthDate | Identifier::Date | Identifier::Age => {
                "(C) dates (except year) and ages over 89"
            }
            Identifier::Phone => "(D) telephone numbers",
            Identifier::Fax => "(E) fax numbers",
            Identifier::Email => "(F) email addresses",
            Identifier::Ssn => "(G) Social Security numbers",
            Identifier::MedicalRecordNumber => "(H) medical record numbers",
            Identifier::HealthPlanNumber => "(I) health plan beneficiary numbers",
            Identifier::AccountNumber => "(J) account numbers",
            Identifier::LicenseNumber => "(K) certificate/license numbers",
            Identifier::VehicleIdentifier => "(L) vehicle identifiers",
            Identifier::DeviceIdentifier => "(M) device identifiers",
            Identifier::Url => "(N) URLs",
            Identifier::IpAddress => "(O) IP addresses",
            Identifier::Biometric => "(P) biometric identifiers",
            Identifier::Photo => "(Q) full-face photographs",
            Identifier::OtherIdNumber | Identifier::FreeText => {
                "(R) other unique identifying numbers, characteristics, or codes"
            }
        }
    }

    /// True for direct identifiers that are never shown in row-level output.
    pub fn is_direct(self) -> bool {
        !matches!(
            self,
            Identifier::Age | Identifier::Date | Identifier::ZipCode | Identifier::Geography
        )
    }
}

/// How strong the evidence is.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Medium,
    High,
}

/// A column that may contain identifiers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ColumnFinding {
    pub column: String,
    pub index: usize,
    pub kind: Identifier,
    pub confidence: Confidence,
    /// Why the column was flagged (never includes values).
    pub evidence: String,
}

// ---------------------------------------------------------------------------
// Name lists
// ---------------------------------------------------------------------------

fn load_list(text: &'static str) -> HashSet<&'static str> {
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .collect()
}

static FIRST_NAMES: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| load_list(include_str!("../data/first_names.txt")));
static SURNAMES: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| load_list(include_str!("../data/surnames.txt")));

/// Capitalized words that are common in public health documents and also
/// appear in name lists; they never start a detected name.
static NOT_NAMES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "may",
        "june",
        "april",
        "august",
        "january",
        "march",
        "will",
        "grant",
        "page",
        "health",
        "public",
        "board",
        "mass",
        "general",
        "town",
        "city",
        "north",
        "south",
        "east",
        "west",
        "new",
        "saint",
        "chief",
        "school",
        "county",
        "state",
        "united",
        "regional",
        "district",
        "department",
        "office",
        "nurse",
        "doctor",
        "medical",
        "center",
        "hospital",
        "clinic",
        "director",
        "manager",
        "officer",
        "coordinator",
        "program",
        "services",
        "service",
        "section",
        "policy",
        "procedure",
        "table",
        "figure",
        "appendix",
        "chapter",
        "monday",
        "tuesday",
        "wednesday",
        "thursday",
        "friday",
        "saturday",
        "sunday",
        "christian",
        "major",
        "young",
        "king",
        "early",
        "summer",
        "winter",
        "spring",
        "fall",
        "faith",
        "hope",
        "joy",
        "dean",
        "hunter",
        "mason",
        "carter",
        "cook",
        "street",
        "avenue",
        "road",
        "lane",
        "drive",
        "court",
        "place",
        "way",
        "hill",
        "park",
        "lake",
        "river",
        "green",
        "white",
        "black",
        "brown",
        "gray",
        "grey",
        "rose",
        "lily",
        "ivy",
        "dawn",
        "sky",
    ]
    .into_iter()
    .collect()
});

/// True when `word` (any case) is a known first name.
pub fn is_first_name(word: &str) -> bool {
    FIRST_NAMES.contains(word.to_ascii_lowercase().as_str())
}

/// True when `word` (any case) is a known surname.
pub fn is_surname(word: &str) -> bool {
    SURNAMES.contains(word.to_ascii_lowercase().as_str())
}

// ---------------------------------------------------------------------------
// Patterns
// ---------------------------------------------------------------------------

static RE_SSN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d{3}[- ]\d{2}[- ]\d{4}$").unwrap());
static RE_LOCAL_PHONE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{3}-\d{4}$").unwrap());
static RE_SSN_BARE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{9}$").unwrap());
static RE_PHONE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:\+?1[\s.-]?)?\(?\d{3}\)?[\s.-]?\d{3}[\s.-]?\d{4}(?:\s*(?:x|ext\.?)\s*\d+)?$")
        .unwrap()
});
static RE_EMAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$").unwrap());
static RE_URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(?:https?://|www\.)\S+$").unwrap());
static RE_IP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(?:\d{1,3}\.){3}\d{1,3}$").unwrap());
static RE_ZIP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{5}(?:-\d{4})?$").unwrap());
static RE_STREET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\d{1,6}[A-Za-z]?\s+.*\b(?:st|street|ave|avenue|rd|road|dr|drive|ln|lane|way|ct|court|blvd|boulevard|pl|place|ter|terrace|hwy|highway|pkwy|parkway|cir|circle|sq|square)\b\.?")
        .unwrap()
});
static RE_ID_CODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z]{0,4}[-#]?\d{4,14}[A-Za-z]{0,2}$").unwrap());

// Free-text patterns.
static T_SSN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b\d{3}[- ]\d{2}[- ]\d{4}\b").unwrap());
/// A labeled SSN in any common layout ("SSN 123 45 6789", "SSN: 123456789").
static T_SSN_LABELED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:SSN|SS#|social security(?: number| no\.?| #)?)[\s:#]*\d{3}[-. ]?\d{2}[-. ]?\d{4}\b",
    )
    .unwrap()
});
static T_PHONE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:\+?1[\s.-]?)?(?:\(\d{3}\)|\b\d{3})[\s.-]?\d{3}[\s.-]\d{4}\b").unwrap()
});
/// Ten digits with no separators, in the North American numbering plan (area
/// code and exchange start with 2–9), optionally after a leading 1.
static T_PHONE_BARE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b1?[2-9]\d{2}[2-9]\d{6}\b").unwrap());
/// A seven-digit local number, only right after a phone word ("call 555-0101"),
/// so ranges of counts such as "500-1000" are not flagged. Group 1 is the number.
static T_PHONE_LOCAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:call|phone|tel|telephone|cell|mobile|fax|ph|text|pager)\b\.?(?:\s+(?:number|no\.?|is|at))*[\s:#-]*(\d{3}-\d{4})\b",
    )
    .unwrap()
});
static T_EMAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b").unwrap());
static T_URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)\b(?:https?://|www\.)[^\s<>"')]+"#).unwrap());
static T_IP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").unwrap());
const DATE_PAT: &str = r"(?:\d{1,2}[/-]\d{1,2}[/-]\d{2,4}|\d{4}-\d{2}-\d{2}|(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Sept|Oct|Nov|Dec)[a-z]*\.?\s+\d{1,2},?\s+\d{4}|\d{1,2}\.\d{1,2}\.\d{4}|\d{1,2}\s+(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Sept|Oct|Nov|Dec)[a-z]*\.?,?\s+\d{4})";
/// A compact date (YYYYMMDD) is accepted only right after a birth-date label.
const COMPACT_DATE_PAT: &str = r"(?:19|20)\d\d[01]\d[0-3]\d\b";
static T_DOB: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)\b(?:DOB|D\.O\.B\.?|date of birth|birth\s?date|birthday|born(?: on)?)\s*[:#-]?\s*(?:{DATE_PAT}|{COMPACT_DATE_PAT})"
    ))
    .unwrap()
});
static T_DATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"(?i)\b{DATE_PAT}\b")).unwrap());
static T_LABELED_ID: LazyLock<Regex> = LazyLock::new(|| {
    // The value (group 2) must contain a digit and be at least four characters,
    // so ordinary words after a label ("patient identified", "Medicare
    // enrollment") are not treated as record numbers. Any mix of spaces, ":"
    // and "#" may separate the label from the value ("MRN: #12345").
    Regex::new(r"(?i)\b(MRN|medical record(?: number| no\.?| #)?|patient id|client id|case (?:id|number|no\.?|#)|SASID|chart (?:number|no\.?|#)|member id|medicaid(?: id| number)?|masshealth(?: id| number)?|medicare(?: id| number)?|account (?:number|no\.?|#)|acct\.? ?#?|license (?:number|no\.?|#)|SSN|social security(?: number| no\.?)?)(?:\s+(?:number|num|no\.?))?[\s:#]*(\d[A-Z0-9-]{3,}|[A-Z]\d[A-Z0-9-]{2,}|[A-Z][A-Z-]\d[A-Z0-9-]+|[A-Z][A-Z-]{2,5}\d[A-Z0-9-]*)\b")
        .unwrap()
});
static T_STREET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b\d{1,6}\s+(?:[A-Z][A-Za-z]+\.?\s){1,3}(?:St|Street|Ave|Avenue|Rd|Road|Dr|Drive|Ln|Lane|Way|Ct|Court|Blvd|Boulevard|Pl|Place|Ter|Terrace|Hwy|Highway|Pkwy|Parkway|Cir|Circle)\b\.?(?:,?\s*(?:Apt|Unit|#)\s*\w+)?")
        .unwrap()
});
static T_STATE_ZIP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(?:MA|Mass\.?|Massachusetts)\s+(\d{5}(?:-\d{4})?)\b").unwrap());
static T_TITLED_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:Mr|Mrs|Ms|Miss|Mx)\.?\s+[A-Z][a-z]+(?:[-'][A-Z][a-z]+)?(?:\s+[A-Z][a-z]+)?")
        .unwrap()
});
static T_CAP_WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b[A-Z][a-z]+(?:[-'][A-Z][a-z]+)?\b").unwrap());
static T_INITIAL_GAP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s+(?:[A-Z]\.\s+)?$").unwrap());

/// Words that introduce a person ("Daughter Carmen", "client Ana").
const RELATION_CUES: [&str; 24] = [
    "mother",
    "father",
    "daughter",
    "son",
    "wife",
    "husband",
    "spouse",
    "partner",
    "sister",
    "brother",
    "grandmother",
    "grandfather",
    "aunt",
    "uncle",
    "cousin",
    "guardian",
    "parent",
    "client",
    "patient",
    "student",
    "child",
    "resident",
    "caregiver",
    "neighbor",
];

// ---------------------------------------------------------------------------
// Dates
// ---------------------------------------------------------------------------

/// Parse common date formats (ISO, US slash, dotted, compact YYYYMMDD,
/// Excel-style datetime, month names, and month-year as the first of the
/// month).
///
/// A result is accepted only when its year is from 1900 to 2100. chrono's
/// `%Y` reads one to four digits, so "3/5/21" would otherwise parse as the
/// year 21; such values fall through to the two-digit-year formats instead.
/// Two-digit years that would land more than a year in the future are read as
/// the previous century ("3/7/45" is 1945).
pub fn parse_date(s: &str) -> Option<chrono::NaiveDate> {
    use chrono::{Datelike, NaiveDate};
    let s = s.trim();
    if s.len() < 6 || s.len() > 30 {
        return None;
    }
    let plausible = |d: NaiveDate| (1900..=2100).contains(&d.year()).then_some(d);
    let head = s.split(['T', ' ']).next().unwrap_or(s);
    if head.len() == 8
        && head.bytes().all(|b| b.is_ascii_digit())
        && let Some(d) = NaiveDate::parse_from_str(head, "%Y%m%d")
            .ok()
            .and_then(plausible)
    {
        return Some(d);
    }
    for fmt in [
        "%Y-%m-%d", "%m/%d/%Y", "%m-%d-%Y", "%Y/%m/%d", "%d-%b-%Y", "%m.%d.%Y",
    ] {
        if let Some(d) = NaiveDate::parse_from_str(head, fmt)
            .ok()
            .and_then(plausible)
        {
            return Some(d);
        }
    }
    for fmt in ["%m/%d/%y", "%m-%d-%y", "%d-%b-%y", "%m.%d.%y"] {
        if let Ok(d) = NaiveDate::parse_from_str(head, fmt) {
            let limit = chrono::Local::now().date_naive() + chrono::Duration::days(366);
            let d = if d > limit {
                d.with_year(d.year() - 100).unwrap_or(d)
            } else {
                d
            };
            if let Some(d) = plausible(d) {
                return Some(d);
            }
        }
    }
    // Day-first dates are tried last, so they only apply when the first
    // number cannot be a month ("13/03/2015").
    for fmt in ["%d/%m/%Y", "%d-%m-%Y", "%d.%m.%Y"] {
        if let Some(d) = NaiveDate::parse_from_str(head, fmt)
            .ok()
            .and_then(plausible)
        {
            return Some(d);
        }
    }
    for fmt in [
        "%B %d, %Y",
        "%b %d, %Y",
        "%B %d %Y",
        "%b %d %Y",
        "%d %B %Y",
        "%d %b %Y",
    ] {
        if let Some(d) = NaiveDate::parse_from_str(s, fmt).ok().and_then(plausible) {
            return Some(d);
        }
    }
    // Month and year only ("March 2015"): the first of that month.
    if s.starts_with(|c: char| c.is_ascii_alphabetic())
        && let Some(d) = NaiveDate::parse_from_str(&format!("1 {s}"), "%d %B %Y")
            .ok()
            .and_then(plausible)
    {
        return Some(d);
    }
    None
}

/// A bare four-digit year from 1900 to 2100 ("2024"), which is not an
/// identifier under Safe Harbor and may be kept as it is.
pub fn plain_year(s: &str) -> Option<i32> {
    let s = s.trim();
    if s.len() != 4 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse::<i32>().ok().filter(|y| (1900..=2100).contains(y))
}

/// False when an age column's header says the unit is months, weeks, or days,
/// so the "over 89" rule (which is about years) does not apply to it.
pub fn age_header_is_years(header: &str) -> bool {
    !tokens(header).iter().any(|t| {
        matches!(
            t.as_str(),
            "month" | "months" | "mo" | "mos" | "week" | "weeks" | "wk" | "wks" | "day" | "days"
        )
    })
}

// ---------------------------------------------------------------------------
// Column classification
// ---------------------------------------------------------------------------

fn tokens(header: &str) -> Vec<String> {
    // "#" in a header means "number" ("Case #", "Record #").
    phn_core::text::normalize(&header.replace('#', " number "))
        .split(' ')
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// Classify a column by its header alone.
fn header_kind(header: &str) -> Option<(Identifier, Confidence)> {
    let t = tokens(header);
    let joined = t.join(" ");
    let has = |w: &str| t.iter().any(|x| x == w);
    let has_any = |ws: &[&str]| ws.iter().any(|w| has(w));
    let contains = |p: &str| joined.contains(p);
    use Confidence::*;
    use Identifier::*;

    if has_any(&["ssn", "ss"]) || contains("social security") {
        return Some((Ssn, High));
    }
    if has_any(&["dob", "birthdate", "bdate", "bday"])
        || contains("date of birth")
        || contains("birth date")
        || (has("birth") && !has("place") && !has("country"))
    {
        return Some((BirthDate, High));
    }
    if has_any(&["mrn"])
        || contains("medical record")
        || contains("chart number")
        || contains("chart no")
        || contains("patient id")
        || contains("patient number")
    {
        return Some((MedicalRecordNumber, High));
    }
    if has_any(&[
        "medicaid",
        "masshealth",
        "medicare",
        "mbi",
        "insurance",
        "insurer",
        "subscriber",
    ]) || contains("member id")
        || contains("policy number")
        || contains("health plan")
    {
        return Some((HealthPlanNumber, High));
    }
    if has_any(&["account", "acct"]) {
        return Some((AccountNumber, High));
    }
    if has("vin") || contains("license plate") || has("plate") {
        return Some((VehicleIdentifier, High));
    }
    if has_any(&["license", "licence", "certificate"]) || contains("drivers license") {
        return Some((LicenseNumber, High));
    }
    if has_any(&["imei", "serial"]) || contains("device id") || contains("mac address") {
        return Some((DeviceIdentifier, High));
    }
    if has("fax") {
        return Some((Fax, High));
    }
    if has_any(&["phone", "telephone", "tel", "mobile", "cell", "cellphone"])
        || contains("contact number")
    {
        return Some((Phone, High));
    }
    if has_any(&["email", "mail"]) || contains("e mail") {
        return Some((Email, High));
    }
    if has_any(&["url", "website", "webpage"]) {
        return Some((Url, High));
    }
    if has("ip") || contains("ip address") {
        return Some((IpAddress, High));
    }
    if has_any(&["photo", "photograph", "picture", "headshot"]) {
        return Some((Photo, High));
    }
    if has_any(&["fingerprint", "retina", "voiceprint", "biometric"]) {
        return Some((Biometric, High));
    }
    if has_any(&["zip", "zipcode", "postal", "postcode"]) {
        return Some((ZipCode, High));
    }
    if has_any(&["address", "addr", "street", "apt", "apartment"]) || contains("residence") {
        return Some((StreetAddress, High));
    }
    if has_any(&[
        "latitude",
        "longitude",
        "lat",
        "lon",
        "lng",
        "geocode",
        "tract",
        "precinct",
        "neighborhood",
        "neighbourhood",
        "block",
        "coordinates",
        "xy",
    ]) || has_any(&["city", "town", "municipality", "village", "county"])
    {
        return Some((Geography, High));
    }
    // Names: exclude names of things.
    const NOT_PERSON: [&str; 26] = [
        "town",
        "city",
        "school",
        "district",
        "facility",
        "site",
        "county",
        "vaccine",
        "drug",
        "medication",
        "product",
        "manufacturer",
        "organization",
        "agency",
        "program",
        "file",
        "disease",
        "condition",
        "test",
        "street",
        "company",
        "lab",
        "laboratory",
        "brand",
        "event",
        "clinic",
    ];
    let person_hint = has_any(&[
        "name",
        "fname",
        "lname",
        "surname",
        "firstname",
        "lastname",
        "middlename",
        "fullname",
        "initials",
        "guardian",
        "parent",
        "mother",
        "father",
        "spouse",
        "patient",
        "client",
        "student",
        "child",
        "contact",
    ]);
    if person_hint && !has_any(&NOT_PERSON) {
        let strong = has_any(&[
            "name",
            "fname",
            "lname",
            "surname",
            "firstname",
            "lastname",
            "fullname",
            "middlename",
        ]);
        if strong {
            return Some((Name, High));
        }
    }
    if has_any(&[
        "notes",
        "note",
        "comment",
        "comments",
        "narrative",
        "remarks",
        "description",
        "details",
        "history",
        "summary",
    ]) {
        return Some((FreeText, High));
    }
    if has("age") || contains("age years") || contains("age yrs") {
        return Some((Age, High));
    }
    if has_any(&["sasid", "lasid", "nhi", "maven"]) {
        return Some((OtherIdNumber, High));
    }
    // "Number of patient visits" is a count, not an ID.
    if (has("id") || has("identifier") || has("number") || has("no") || has("num"))
        && !has("of")
        && (person_hint
            || has_any(&[
                "case", "record", "student", "patient", "client", "member", "event",
            ]))
    {
        return Some((OtherIdNumber, High));
    }
    // A column headed just "ID", "ID number", or "Identifier".
    if (has("id") || has("identifier"))
        && t.iter().all(|x| {
            matches!(
                x.as_str(),
                "id" | "identifier" | "number" | "no" | "num" | "code"
            )
        })
    {
        return Some((OtherIdNumber, High));
    }
    if has("date") || has("dt") || joined.ends_with(" date") {
        return Some((Date, Medium));
    }
    None
}

/// Share of `values` satisfying `pred`.
fn share(values: &[&str], pred: impl Fn(&str) -> bool) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().filter(|v| pred(v)).count() as f64 / values.len() as f64
}

/// Words that make up a person's name when they are the whole header.
const NAME_PART_HEADERS: [&str; 10] = [
    "first", "last", "middle", "given", "family", "maiden", "nickname", "alias", "aka", "mi",
];

/// Header words that point at a person ("Student", "Guardian", "Contact").
const PERSON_HEADERS: [&str; 10] = [
    "guardian", "parent", "mother", "father", "spouse", "patient", "client", "student", "child",
    "contact",
];

/// Header words that show a "person" header is about something else
/// ("Student grade", "Contact type", "Patient status").
const NOT_PERSON_HEADERS: [&str; 22] = [
    "type",
    "status",
    "grade",
    "age",
    "sex",
    "gender",
    "race",
    "ethnicity",
    "language",
    "count",
    "total",
    "school",
    "town",
    "city",
    "relationship",
    "result",
    "category",
    "group",
    "setting",
    "method",
    "level",
    "risk",
];

/// True when every word of the header is a name part ("First", "Last, First").
fn name_part_header(tokens: &[String]) -> bool {
    !tokens.is_empty()
        && tokens
            .iter()
            .all(|t| NAME_PART_HEADERS.contains(&t.as_str()))
}

/// True when the header points at a person and not at an attribute of one.
fn person_header(tokens: &[String]) -> bool {
    tokens.iter().any(|t| PERSON_HEADERS.contains(&t.as_str()))
        && !tokens
            .iter()
            .any(|t| NOT_PERSON_HEADERS.contains(&t.as_str()))
}

/// True for text shaped like a name: one to five alphabetic words, and not a
/// yes/no or "unknown" style answer.
fn name_shaped(v: &str) -> bool {
    let words: Vec<&str> = v
        .split([' ', ','])
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .collect();
    if words.is_empty() || words.len() > 5 {
        return false;
    }
    if !words.iter().all(|w| {
        w.chars()
            .all(|c| c.is_alphabetic() || matches!(c, '-' | '\'' | '.' | '’'))
    }) {
        return false;
    }
    !matches!(
        v.trim().to_ascii_lowercase().as_str(),
        "yes"
            | "no"
            | "y"
            | "n"
            | "true"
            | "false"
            | "t"
            | "f"
            | "unknown"
            | "unk"
            | "other"
            | "none"
            | "self"
    )
}

fn name_like(v: &str) -> bool {
    let words: Vec<&str> = v
        .split([' ', ','])
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .collect();
    if words.is_empty() || words.len() > 4 {
        return false;
    }
    if !words.iter().all(|w| {
        w.chars()
            .all(|c| c.is_alphabetic() || c == '-' || c == '\'' || c == '.')
    }) {
        return false;
    }
    let first = words.iter().any(|w| is_first_name(w));
    let last = words.iter().any(|w| is_surname(w));
    if words.len() == 1 {
        first || last
    } else {
        first && last
    }
}

/// Classify every column of a table.
pub fn scan_table(table: &Table) -> Vec<ColumnFinding> {
    let mut out = Vec::new();
    for (index, header) in table.headers.iter().enumerate() {
        let values: Vec<&str> = table
            .column(index)
            .map(str::trim)
            .filter(|v| !v.is_empty() && !is_missing(v))
            .take(1000)
            .collect();
        if let Some(f) = classify_column(header, index, &values) {
            out.push(f);
        }
    }
    out
}

fn classify_column(header: &str, index: usize, values: &[&str]) -> Option<ColumnFinding> {
    let make = |kind, confidence, evidence: String| ColumnFinding {
        column: header.to_string(),
        index,
        kind,
        confidence,
        evidence,
    };
    let pct = |x: f64| format!("{:.0}%", x * 100.0);
    let distinct: HashSet<&str> = values.iter().copied().collect();
    let distinct_ratio = if values.is_empty() {
        0.0
    } else {
        distinct.len() as f64 / values.len() as f64
    };

    // Strong value patterns win regardless of header.
    let ssn = share(values, |v| RE_SSN.is_match(v));
    if ssn >= 0.5 {
        return Some(make(
            Identifier::Ssn,
            Confidence::High,
            format!("{} of values look like SSNs (###-##-####)", pct(ssn)),
        ));
    }
    let email = share(values, |v| RE_EMAIL.is_match(v));
    if email >= 0.5 {
        return Some(make(
            Identifier::Email,
            Confidence::High,
            format!("{} of values are email addresses", pct(email)),
        ));
    }

    // Nine-digit numbers under a "Social …" header are SSNs without hyphens.
    let header_tokens = tokens(header);
    if header_tokens.iter().any(|t| t == "social") {
        let bare = share(values, |v| RE_SSN_BARE.is_match(v));
        if bare >= 0.5 {
            return Some(make(
                Identifier::Ssn,
                Confidence::High,
                format!(
                    "column name mentions 'social' and {} of values are nine-digit numbers",
                    pct(bare)
                ),
            ));
        }
    }

    let by_header = header_kind(header);
    if let Some((kind, conf)) = by_header {
        // Date headers need date-like values; other header hits stand on their own.
        if kind == Identifier::Date {
            let d = share(values, |v| parse_date(v).is_some());
            if d >= 0.6 {
                return Some(make(
                    kind,
                    Confidence::High,
                    format!(
                        "column name suggests a date; {} of values are dates",
                        pct(d)
                    ),
                ));
            }
            // Values with digits that were not recognized as dates are still
            // treated as dates (and blanked or masked downstream), so an
            // unfamiliar date format is never passed through as it is. Yes/no
            // columns such as "Up to date" are left alone.
            let digits = share(values, |v| v.chars().any(|c| c.is_ascii_digit()));
            if digits >= 0.6 {
                return Some(make(
                    kind,
                    conf,
                    format!(
                        "column name suggests a date; only {} of values were recognized as dates",
                        pct(d)
                    ),
                ));
            }
        } else if kind == Identifier::Age {
            let numeric: Vec<f64> = values
                .iter()
                .filter_map(|v| v.parse::<f64>().ok())
                .collect();
            if !numeric.is_empty() {
                let over = numeric.iter().filter(|&&a| a > 89.0).count();
                return Some(make(
                    kind,
                    conf,
                    format!("column name suggests age; {over} value(s) over 89"),
                ));
            }
        } else {
            return Some(make(
                kind,
                conf,
                format!("column name '{header}' suggests {}", kind.label()),
            ));
        }
    }

    // Headers that point at a person without saying "name": "First", "Last",
    // "Student", "Guardian". The values decide: text shaped like names is
    // flagged even when the names are not on the public name lists.
    if by_header.is_none() && !values.is_empty() {
        let shaped = share(values, name_shaped);
        if shaped >= 0.6 && name_part_header(&header_tokens) {
            return Some(make(
                Identifier::Name,
                Confidence::Medium,
                format!("column name '{header}' suggests part of a person's name"),
            ));
        }
        if shaped >= 0.6 && person_header(&header_tokens) {
            let listed = share(values, name_like);
            let multiword = share(values, |v| v.split_whitespace().count() >= 2);
            // Names are capitalized; categories such as "Sent home" are not.
            let capitalized = share(values, |v| {
                v.split_whitespace()
                    .all(|w| w.chars().next().is_some_and(char::is_uppercase))
            });
            if capitalized >= 0.6
                && (listed >= 0.3
                    || multiword >= 0.5
                    || (distinct.len() >= 3 && distinct_ratio > 0.5))
            {
                return Some(make(
                    Identifier::Name,
                    Confidence::Medium,
                    format!(
                        "column name '{header}' suggests a person and the values look like names"
                    ),
                ));
            }
        }
    }

    let phone = share(values, |v| {
        RE_PHONE.is_match(v) && v.chars().filter(char::is_ascii_digit).count() >= 10
    });
    if phone >= 0.6 {
        return Some(make(
            Identifier::Phone,
            Confidence::High,
            format!("{} of values look like phone numbers", pct(phone)),
        ));
    }
    let url = share(values, |v| RE_URL.is_match(v));
    if url >= 0.6 {
        return Some(make(
            Identifier::Url,
            Confidence::High,
            format!("{} of values are web addresses", pct(url)),
        ));
    }
    let ip = share(values, |v| RE_IP.is_match(v));
    if ip >= 0.6 {
        return Some(make(
            Identifier::IpAddress,
            Confidence::High,
            format!("{} of values are IP addresses", pct(ip)),
        ));
    }
    let street = share(values, |v| RE_STREET.is_match(v));
    if street >= 0.5 {
        return Some(make(
            Identifier::StreetAddress,
            Confidence::High,
            format!("{} of values look like street addresses", pct(street)),
        ));
    }
    let date = share(values, |v| parse_date(v).is_some());
    if date >= 0.8 {
        return Some(make(
            Identifier::Date,
            Confidence::Medium,
            format!("{} of values are dates", pct(date)),
        ));
    }
    let names = share(values, name_like);
    let multiword = share(values, |v| v.split_whitespace().count() >= 2);
    // Single-word columns (first-name or surname columns) must be diverse: a
    // category such as race ("White", "Black") also matches surname lists.
    if names >= 0.6
        && values.len() >= 3
        && (multiword >= 0.5 || (distinct.len() >= 10 && distinct_ratio > 0.5))
    {
        return Some(make(
            Identifier::Name,
            Confidence::Medium,
            format!(
                "{} of values match common first names or surnames",
                pct(names)
            ),
        ));
    }
    let ssn_bare = share(values, |v| RE_SSN_BARE.is_match(v));
    let zip = share(values, |v| {
        RE_ZIP.is_match(v) && (v.starts_with("01") || v.starts_with("02") || v.starts_with("05"))
    });
    if zip >= 0.8 && ssn_bare < 0.5 {
        return Some(make(
            Identifier::ZipCode,
            Confidence::Medium,
            format!("{} of values look like Massachusetts ZIP codes", pct(zip)),
        ));
    }
    let id_like = share(values, |v| RE_ID_CODE.is_match(v));
    if id_like >= 0.9 && distinct_ratio >= 0.95 && values.len() >= 20 {
        return Some(make(
            Identifier::OtherIdNumber,
            Confidence::Medium,
            "values are unique ID-like codes".into(),
        ));
    }
    let avg_len = values.iter().map(|v| v.len()).sum::<usize>() as f64 / values.len().max(1) as f64;
    if avg_len > 40.0 && share(values, |v| v.split_whitespace().count() >= 6) > 0.5 {
        return Some(make(
            Identifier::FreeText,
            Confidence::Medium,
            "long free-text values".into(),
        ));
    }
    None
}

/// Common missing-value markers.
pub fn is_missing(v: &str) -> bool {
    matches!(
        v.trim().to_ascii_lowercase().as_str(),
        "" | "na" | "n/a" | "null" | "none" | "nan" | "#n/a" | "." | "-" | "--"
    )
}

/// How a header cell or a sheet name looks when it is checked for content
/// that is data about a person and not a label. Used to keep the first record
/// of a file without a header row, or names used as column headers, out of
/// tool results (column names are always returned).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderLook {
    /// An ordinary label.
    Label,
    /// A number or a date; real headers can be these ("2024", "9/14/2026").
    DataLike,
    /// A person's full name ("Maria Garcia", "Garcia, Maria").
    Name,
    /// An identifier value: SSN, phone number, email or street address, web
    /// address, a long ID code, or a labeled date of birth.
    Identifier,
}

/// Classify one header cell or sheet name. See [`HeaderLook`].
pub fn header_look(cell: &str) -> HeaderLook {
    let v = cell.trim();
    if v.is_empty() {
        return HeaderLook::Label;
    }
    let digits = v.chars().filter(char::is_ascii_digit).count();
    let all_digits = digits == v.chars().count();
    if RE_SSN.is_match(v)
        || RE_EMAIL.is_match(v)
        || RE_URL.is_match(v)
        || RE_IP.is_match(v)
        || RE_STREET.is_match(v)
        || RE_LOCAL_PHONE.is_match(v)
        || (RE_PHONE.is_match(v) && digits >= 10)
        || (RE_ID_CODE.is_match(v) && digits >= if all_digits { 7 } else { 6 })
    {
        return HeaderLook::Identifier;
    }
    if scan_text(v).iter().any(|f| {
        matches!(
            f.kind,
            Identifier::Ssn
                | Identifier::BirthDate
                | Identifier::Email
                | Identifier::Phone
                | Identifier::StreetAddress
        )
    }) {
        return HeaderLook::Identifier;
    }
    let words: Vec<&str> = v
        .split([' ', ','])
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .collect();
    let alphabetic = |w: &str| {
        w.chars()
            .all(|c| c.is_alphabetic() || matches!(c, '-' | '\'' | '.'))
    };
    if (2..=3).contains(&words.len()) && words.iter().copied().all(alphabetic) {
        let forward =
            !v.contains(',') && is_first_name(words[0]) && is_surname(words[words.len() - 1]);
        let reversed = v.contains(',') && is_surname(words[0]) && is_first_name(words[1]);
        if forward || reversed {
            return HeaderLook::Name;
        }
    }
    if v.replace(',', "").parse::<f64>().is_ok() || parse_date(v).is_some() || is_missing(v) {
        return HeaderLook::DataLike;
    }
    HeaderLook::Label
}

// ---------------------------------------------------------------------------
// Free text
// ---------------------------------------------------------------------------

/// A possible identifier found in text (byte offsets).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextFinding {
    pub kind: Identifier,
    pub start: usize,
    pub end: usize,
    pub confidence: Confidence,
}

/// Find possible identifiers in free text.
pub fn scan_text(text: &str) -> Vec<TextFinding> {
    let mut f = Vec::new();
    let mut push = |re: &Regex, kind, confidence| {
        for m in re.find_iter(text) {
            f.push(TextFinding {
                kind,
                start: m.start(),
                end: m.end(),
                confidence,
            });
        }
    };
    push(&T_SSN, Identifier::Ssn, Confidence::High);
    push(&T_SSN_LABELED, Identifier::Ssn, Confidence::High);
    push(&T_DOB, Identifier::BirthDate, Confidence::High);
    push(&T_EMAIL, Identifier::Email, Confidence::High);
    push(&T_URL, Identifier::Url, Confidence::High);
    push(&T_STREET, Identifier::StreetAddress, Confidence::High);
    push(&T_TITLED_NAME, Identifier::Name, Confidence::High);
    for c in T_LABELED_ID.captures_iter(text) {
        let label = c
            .get(1)
            .map(|m| m.as_str().to_ascii_lowercase())
            .unwrap_or_default();
        let kind = if label.starts_with("ssn") || label.starts_with("social") {
            Identifier::Ssn
        } else if label.starts_with("medicaid")
            || label.starts_with("masshealth")
            || label.starts_with("medicare")
            || label.starts_with("member")
        {
            Identifier::HealthPlanNumber
        } else if label.starts_with("acc") {
            Identifier::AccountNumber
        } else if label.starts_with("license") {
            Identifier::LicenseNumber
        } else if label.starts_with("mrn")
            || label.starts_with("medical")
            || label.starts_with("chart")
            || label.starts_with("patient")
        {
            Identifier::MedicalRecordNumber
        } else {
            Identifier::OtherIdNumber
        };
        let m = c.get(0).expect("whole match");
        f.push(TextFinding {
            kind,
            start: m.start(),
            end: m.end(),
            confidence: Confidence::High,
        });
    }
    for c in T_STATE_ZIP.captures_iter(text) {
        let m = c.get(1).expect("zip group");
        f.push(TextFinding {
            kind: Identifier::ZipCode,
            start: m.start(),
            end: m.end(),
            confidence: Confidence::High,
        });
    }
    // Phones and IPs that are not part of something already found.
    for m in T_PHONE.find_iter(text) {
        f.push(TextFinding {
            kind: Identifier::Phone,
            start: m.start(),
            end: m.end(),
            confidence: Confidence::Medium,
        });
    }
    for m in T_PHONE_BARE.find_iter(text) {
        f.push(TextFinding {
            kind: Identifier::Phone,
            start: m.start(),
            end: m.end(),
            confidence: Confidence::Medium,
        });
    }
    for c in T_PHONE_LOCAL.captures_iter(text) {
        let m = c.get(1).expect("number group");
        f.push(TextFinding {
            kind: Identifier::Phone,
            start: m.start(),
            end: m.end(),
            confidence: Confidence::Medium,
        });
    }
    for m in T_IP.find_iter(text) {
        f.push(TextFinding {
            kind: Identifier::IpAddress,
            start: m.start(),
            end: m.end(),
            confidence: Confidence::Medium,
        });
    }
    for m in T_DATE.find_iter(text) {
        f.push(TextFinding {
            kind: Identifier::Date,
            start: m.start(),
            end: m.end(),
            confidence: Confidence::Medium,
        });
    }
    // Person names: adjacent capitalized words (optionally with a middle
    // initial) where the first is a known first name and the second a known
    // surname, or a first name right after a relationship word. Pairs are
    // checked at every position so "Daughter Carmen Delgado" is caught.
    let words: Vec<regex::Match<'_>> = T_CAP_WORD.find_iter(text).collect();
    let adjacent = |a: &regex::Match<'_>, b: &regex::Match<'_>| {
        T_INITIAL_GAP.is_match(&text[a.end()..b.start()])
    };
    let mut surnames_found: HashSet<String> = HashSet::new();
    for i in 0..words.len() {
        let a = words[i];
        let a_l = a.as_str().to_ascii_lowercase();
        let next = words.get(i + 1).filter(|b| adjacent(&a, b));
        // Relationship cue: "Daughter Carmen [Delgado]". The cue word may be
        // lower case, so it is looked for even before the first capitalized word.
        let preceded_by_cue = {
            let prev = &text[..a.start()];
            let cue = prev
                .trim_end()
                .rsplit(|c: char| !c.is_alphabetic())
                .next()
                .unwrap_or("");
            RELATION_CUES.contains(&cue.to_ascii_lowercase().as_str())
        };
        if NOT_NAMES.contains(a_l.as_str()) {
            continue;
        }
        if let Some(b) = next {
            let b_first_part = b.as_str().split(['-', '\'']).next().unwrap_or(b.as_str());
            if !NOT_NAMES.contains(b.as_str().to_ascii_lowercase().as_str())
                && is_first_name(a.as_str())
                && is_surname(b_first_part)
            {
                f.push(TextFinding {
                    kind: Identifier::Name,
                    start: a.start(),
                    end: b.end(),
                    confidence: Confidence::Medium,
                });
                surnames_found.insert(b.as_str().to_string());
                continue;
            }
        }
        if preceded_by_cue && is_first_name(a.as_str()) {
            let end = next.map(|b| b.end()).unwrap_or(a.end());
            f.push(TextFinding {
                kind: Identifier::Name,
                start: a.start(),
                end,
                confidence: Confidence::Medium,
            });
            if let Some(b) = next {
                surnames_found.insert(b.as_str().to_string());
            }
        }
    }
    // Surnames from titled names ("Mrs. Rosa Delgado") count too.
    for m in T_TITLED_NAME.find_iter(text) {
        if let Some(last) = m.as_str().split_whitespace().last()
            && last.chars().next().is_some_and(char::is_uppercase)
            && !last.ends_with('.')
        {
            surnames_found.insert(last.to_string());
        }
    }
    // Propagate: other mentions of a detected surname are names too.
    for w in &words {
        if surnames_found.contains(w.as_str()) {
            f.push(TextFinding {
                kind: Identifier::Name,
                start: w.start(),
                end: w.end(),
                confidence: Confidence::Medium,
            });
        }
    }
    // Overlapping spans are merged into one finding that covers all of them,
    // so the tail of a later finding (for example the number in "Mrs. Delgado
    // Medicaid ID: 100234567", where the name span reaches "Medicaid") is never
    // left outside a redaction. The merged finding takes the kind that standard
    // redaction removes when either span has one.
    f.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then(b.confidence.cmp(&a.confidence))
            .then((b.end - b.start).cmp(&(a.end - a.start)))
    });
    let rank = |x: &TextFinding| (redacted_by(RedactionLevel::Standard, x), x.confidence);
    let mut out: Vec<TextFinding> = Vec::new();
    for x in f {
        if let Some(last) = out.last_mut()
            && x.start < last.end
        {
            if rank(&x) > rank(last) {
                last.kind = x.kind;
                last.confidence = x.confidence;
            }
            last.end = last.end.max(x.end);
            continue;
        }
        out.push(x);
    }
    out
}

/// What to redact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RedactionLevel {
    /// Redact Social Security numbers, labeled dates of birth, and labeled
    /// record, member, account, and license numbers. Suitable for agency
    /// documents, which legitimately contain staff names and phone numbers.
    #[default]
    Standard,
    /// Redact everything detected, including names, phone numbers, email and
    /// street addresses, URLs, and dates. Used for client data.
    Strict,
}

fn redacted_by(level: RedactionLevel, f: &TextFinding) -> bool {
    match level {
        RedactionLevel::Strict => true,
        RedactionLevel::Standard => {
            matches!(
                f.kind,
                Identifier::Ssn
                    | Identifier::BirthDate
                    | Identifier::MedicalRecordNumber
                    | Identifier::HealthPlanNumber
                    | Identifier::AccountNumber
                    | Identifier::LicenseNumber
                    | Identifier::OtherIdNumber
            ) && f.confidence == Confidence::High
        }
    }
}

/// Result of redacting text.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Redaction {
    pub text: String,
    /// Counts of redacted spans by kind.
    pub redacted: BTreeMap<Identifier, usize>,
    /// Counts of detected but not redacted spans by kind.
    pub detected_only: BTreeMap<Identifier, usize>,
}

/// Replace detected identifiers with `[REDACTED: kind]`.
pub fn redact_text(text: &str, level: RedactionLevel) -> Redaction {
    let findings = scan_text(text);
    let mut out = String::with_capacity(text.len());
    let mut r = Redaction::default();
    let mut pos = 0;
    for f in &findings {
        if redacted_by(level, f) {
            out.push_str(&text[pos..f.start]);
            out.push_str(&format!("[REDACTED: {}]", f.kind.label()));
            pos = f.end;
            *r.redacted.entry(f.kind).or_default() += 1;
        } else {
            *r.detected_only.entry(f.kind).or_default() += 1;
        }
    }
    out.push_str(&text[pos..]);
    r.text = out;
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(headers: &[&str], rows: &[&[&str]]) -> Table {
        Table {
            name: "t".into(),
            headers: headers.iter().map(|s| s.to_string()).collect(),
            rows: rows
                .iter()
                .map(|r| r.iter().map(|s| s.to_string()).collect())
                .collect(),
            truncated: false,
        }
    }

    #[test]
    fn name_lists_load() {
        assert!(is_first_name("Maria") && is_surname("Garcia"));
        assert!(!is_first_name("pertussis"));
    }

    #[test]
    fn classifies_columns_by_header_and_values() {
        let t = table(
            &[
                "Patient", "DOB", "Phone #", "Town", "Onset", "Dx", "Age", "Case ID", "Comments",
                "Contact",
            ],
            &[
                &[
                    "Maria Garcia",
                    "2015-03-02",
                    "413-555-0101",
                    "Amherst",
                    "2024-01-03",
                    "Pertussis",
                    "92",
                    "C-1001",
                    "called mom",
                    "maria.g@example.com",
                ],
                &[
                    "James Smith",
                    "2012-11-30",
                    "(617) 555-0199",
                    "Boston",
                    "2024-01-05",
                    "Pertussis",
                    "8",
                    "C-1002",
                    "",
                    "jsmith@example.org",
                ],
                &[
                    "Linda Nguyen",
                    "2010-07-14",
                    "508.555.0123",
                    "Worcester",
                    "2024-01-06",
                    "Measles",
                    "13",
                    "C-1003",
                    "",
                    "ln@example.net",
                ],
            ],
        );
        let f = scan_table(&t);
        let kind = |c: &str| f.iter().find(|x| x.column == c).map(|x| x.kind);
        assert_eq!(kind("Patient"), Some(Identifier::Name));
        assert_eq!(kind("DOB"), Some(Identifier::BirthDate));
        assert_eq!(kind("Phone #"), Some(Identifier::Phone));
        assert_eq!(kind("Town"), Some(Identifier::Geography));
        assert_eq!(kind("Onset"), Some(Identifier::Date));
        assert_eq!(kind("Dx"), None);
        assert_eq!(kind("Age"), Some(Identifier::Age));
        assert_eq!(kind("Case ID"), Some(Identifier::OtherIdNumber));
        assert_eq!(kind("Comments"), Some(Identifier::FreeText));
        assert_eq!(kind("Contact"), Some(Identifier::Email));
        // Evidence never contains values.
        assert!(
            f.iter()
                .all(|x| !x.evidence.contains("Garcia") && !x.evidence.contains("555"))
        );
    }

    #[test]
    fn categorical_columns_are_not_names() {
        let t = table(
            &["Race"],
            &[&["White"], &["Black"], &["White"], &["Asian"], &["White"]],
        );
        assert!(scan_table(&t).is_empty());
    }

    #[test]
    fn scans_and_redacts_free_text() {
        let text = "Spoke with Mrs. Alvarez. Maria Garcia, DOB: 03/02/2015, MRN 00451234, \
                    lives at 12 Elm Street, Amherst MA 01002. SSN 123-45-6789. Call 413-555-0101 \
                    or email maria@example.com. Policy updated June 2024 by the Board of Health.";
        let strict = redact_text(text, RedactionLevel::Strict);
        for secret in [
            "Alvarez",
            "Garcia",
            "03/02/2015",
            "00451234",
            "12 Elm Street",
            "01002",
            "123-45-6789",
            "413-555-0101",
            "maria@example.com",
        ] {
            assert!(
                !strict.text.contains(secret),
                "{secret} survived: {}",
                strict.text
            );
        }
        assert!(strict.text.contains("Board of Health"));
        let standard = redact_text(text, RedactionLevel::Standard);
        assert!(!standard.text.contains("123-45-6789"));
        assert!(!standard.text.contains("03/02/2015"));
        assert!(!standard.text.contains("00451234"));
        assert!(standard.text.contains("413-555-0101")); // phone kept, but counted
        assert!(standard.detected_only.contains_key(&Identifier::Phone));
    }

    #[test]
    fn overlapping_findings_are_merged_not_dropped() {
        // The titled-name span reaches "Medicaid"; the number must still go.
        for (text, secret) in [
            ("Mrs. Delgado\nMedicaid ID: 100234567", "100234567"),
            ("Carmen\nCase number: 2024-0118", "2024-0118"),
            (
                "Client: Mrs. Delgado\nDate of Birth: 04/17/1958",
                "04/17/1958",
            ),
        ] {
            for level in [RedactionLevel::Standard, RedactionLevel::Strict] {
                let r = redact_text(text, level);
                assert!(!r.text.contains(secret), "{secret} survived: {}", r.text);
            }
        }
        // The merged span is counted under the kind standard redaction removes.
        let r = redact_text(
            "Client: Mrs. Delgado\nDate of Birth: 04/17/1958",
            RedactionLevel::Standard,
        );
        assert!(r.redacted.contains_key(&Identifier::BirthDate));
    }

    #[test]
    fn common_identifier_formats_are_found() {
        let strict = |t: &str| redact_text(t, RedactionLevel::Strict).text;
        assert!(!strict("SSN 123 45 6789 on file").contains("6789"));
        assert!(!strict("SSN: 123456789").contains("123456789"));
        assert!(!strict("MRN: #12345 and MRN#: 99887").contains("12345"));
        assert!(!strict("MRN: #12345 and MRN#: 99887").contains("99887"));
        assert!(!strict("call 555-0101 today").contains("555-0101"));
        assert!(!strict("cell 4135550101").contains("4135550101"));
        assert!(!strict("DOB 20150307").contains("20150307"));
        assert!(!strict("born 7 March 2015").contains("March"));
        assert!(!strict("DOB: 03.07.2015").contains("03.07"));
        assert!(!strict("spoke with daughter Carmen today").contains("Carmen"));
        // Ordinary words after a label, ranges of counts, and ZIP+4 are left alone.
        for plain in [
            "Patient identified by medicare enrollment",
            "serving 500-1000 residents",
            "ZIP 01002-1234",
            "school year 2024-2025",
        ] {
            assert!(
                scan_text(plain).is_empty(),
                "{plain}: {:?}",
                scan_text(plain)
            );
        }
    }

    #[test]
    fn person_and_id_headers_are_flagged_in_small_files() {
        let t = table(
            &[
                "First",
                "Last",
                "ID",
                "Case #",
                "Student",
                "Up to date",
                "MI",
            ],
            &[
                &["Thuy", "Okafor", "104", "7", "Ngoc Adeyemi", "Y", "N"],
                &["Abdi", "Mohamud", "105", "8", "Linh Abdi", "N", "Y"],
                &["Thuy", "Okafor", "104", "9", "Ngoc Adeyemi", "Y", "N"],
            ],
        );
        let f = scan_table(&t);
        let kind = |c: &str| f.iter().find(|x| x.column == c).map(|x| x.kind);
        assert_eq!(kind("First"), Some(Identifier::Name));
        assert_eq!(kind("Last"), Some(Identifier::Name));
        assert_eq!(kind("Student"), Some(Identifier::Name));
        assert_eq!(kind("ID"), Some(Identifier::OtherIdNumber));
        assert_eq!(kind("Case #"), Some(Identifier::OtherIdNumber));
        // Yes/no columns are not names or dates.
        assert_eq!(kind("Up to date"), None);
        assert_eq!(kind("MI"), None);
        // A date header with an unfamiliar format is still treated as a date.
        let t = table(&["Onset Date"], &[&["2024.W03-2"], &["2024.W04-5"]]);
        assert_eq!(scan_table(&t)[0].kind, Identifier::Date);
    }

    #[test]
    fn header_cells_that_are_data_are_recognized() {
        assert_eq!(header_look("413-555-0101"), HeaderLook::Identifier);
        assert_eq!(header_look("maria@example.org"), HeaderLook::Identifier);
        assert_eq!(header_look("12 Elm St"), HeaderLook::Identifier);
        assert_eq!(header_look("C-20260001"), HeaderLook::Identifier);
        assert_eq!(header_look("Maria Garcia"), HeaderLook::Name);
        assert_eq!(header_look("Garcia, Maria"), HeaderLook::Name);
        assert_eq!(header_look("2024"), HeaderLook::DataLike);
        assert_eq!(header_look("3/7/2015"), HeaderLook::DataLike);
        for label in [
            "Town",
            "Onset Date",
            "Student Name",
            "MRN Number",
            "FY2024",
            "Total",
        ] {
            assert_eq!(header_look(label), HeaderLook::Label, "{label}");
        }
    }

    #[test]
    fn two_digit_years_and_other_formats_parse() {
        let d = |y, m, day| chrono::NaiveDate::from_ymd_opt(y, m, day);
        assert_eq!(parse_date("3/5/21"), d(2021, 3, 5));
        assert_eq!(parse_date("3/7/45"), d(1945, 3, 7));
        assert_eq!(parse_date("20150307"), d(2015, 3, 7));
        assert_eq!(parse_date("03.07.2015"), d(2015, 3, 7));
        assert_eq!(parse_date("13/03/2015"), d(2015, 3, 13));
        assert_eq!(parse_date("7 March 2015"), d(2015, 3, 7));
        assert_eq!(parse_date("March 2015"), d(2015, 3, 1));
        assert!(parse_date("3/7/0015").is_none());
        assert!(parse_date("12345678").is_none());
        assert_eq!(plain_year("2024"), Some(2024));
        assert_eq!(plain_year("0015"), None);
        assert!(!age_header_is_years("Age (months)") && age_header_is_years("Age"));
    }

    #[test]
    fn parses_dates() {
        assert!(parse_date("2024-01-03").is_some());
        assert!(parse_date("1/3/2024").is_some());
        assert!(parse_date("March 3, 2024").is_some());
        assert!(parse_date("2024-01-03T10:00").is_some());
        assert!(parse_date("Pertussis").is_none());
    }
}
