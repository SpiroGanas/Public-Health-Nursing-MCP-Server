//! Street-name normalization and address parsing.
//!
//! [`normalize_street`] turns the street part of an address into a canonical key
//! ("North Pleasant Street Apt 3" becomes `N PLEASANT ST`). The same function
//! (in Python, `xtask/scripts/build_geocoder_index.py`) builds the keys stored in
//! the index, so the vocabulary lives in one file, `data/geocoder_vocab.json`,
//! and both implementations are tested against `xtask/scripts/geocoder_normalizer_cases.json`.
//!
//! Steps: upper-case; accents removed; apostrophes and periods dropped;
//! everything else that is not a letter, digit, `#`, or `/` becomes a space;
//! the unit part (`APT`, `UNIT`, `STE`, `#`, `FLOOR`, ...) is cut; each word is
//! replaced by its canonical form (ordinal words to numbers, `SAINT` to `ST`,
//! directions and street types to the USPS abbreviation).

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use serde::Deserialize;

#[derive(Deserialize)]
struct VocabFile {
    types: HashMap<String, String>,
    directions: HashMap<String, String>,
    ordinals: HashMap<String, String>,
    words: HashMap<String, String>,
    unit_words: Vec<String>,
    drop_words: Vec<String>,
    state_words: Vec<String>,
}

struct Vocab {
    file: VocabFile,
    unit_words: HashSet<String>,
    drop_words: HashSet<String>,
    state_words: HashSet<String>,
    ordinal_values: HashSet<String>,
    type_values: HashSet<String>,
    direction_values: HashSet<String>,
}

static VOCAB: LazyLock<Vocab> = LazyLock::new(|| {
    let file: VocabFile = serde_json::from_str(include_str!("../../data/geocoder_vocab.json"))
        .expect("bundled geocoder vocabulary is valid JSON");
    Vocab {
        unit_words: file.unit_words.iter().cloned().collect(),
        drop_words: file.drop_words.iter().cloned().collect(),
        state_words: file.state_words.iter().cloned().collect(),
        ordinal_values: file.ordinals.values().cloned().collect(),
        type_values: file.types.values().cloned().collect(),
        direction_values: file.directions.values().cloned().collect(),
        file,
    }
});

fn accent(c: char) -> char {
    match c {
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'A',
        'Ç' => 'C',
        'È' | 'É' | 'Ê' | 'Ë' => 'E',
        'Ì' | 'Í' | 'Î' | 'Ï' => 'I',
        'Ñ' => 'N',
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' => 'O',
        'Ù' | 'Ú' | 'Û' | 'Ü' => 'U',
        'Ý' => 'Y',
        other => other,
    }
}

/// Upper-case words of `text` with accents, apostrophes, and periods removed and other
/// punctuation turned into spaces. Letters, digits, `#`, and `/` are kept.
fn words(text: &str) -> Vec<String> {
    words_with(text, false)
}

/// As [`words`]; with `keep_hyphen` a hyphen stays inside a word ("12-14", "01002-1234").
fn words_with(text: &str, keep_hyphen: bool) -> Vec<String> {
    let mut s = String::with_capacity(text.len());
    for c in text.chars() {
        for u in c.to_uppercase() {
            let u = accent(u);
            match u {
                '\'' | '`' | '\u{2019}' | '.' => {}
                'A'..='Z' | '0'..='9' | '#' | '/' => s.push(u),
                '-' if keep_hyphen => s.push(u),
                _ => s.push(' '),
            }
        }
    }
    s.split_whitespace().map(str::to_string).collect()
}

fn is_ordinal_or_number(t: &str) -> bool {
    if VOCAB.ordinal_values.contains(t) {
        return true;
    }
    let digits = t.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let tail = &t[digits.len()..];
    !digits.is_empty()
        && digits.chars().all(|c| c.is_ascii_digit())
        && matches!(tail, "" | "ST" | "ND" | "RD" | "TH")
}

/// Canonical street key (see the module documentation). An empty or unusable
/// street gives an empty string.
pub fn normalize_street(text: &str) -> String {
    let mut toks = words(text);
    for i in 0..toks.len() {
        let t = &toks[i];
        if i > 0 && (VOCAB.unit_words.contains(t.as_str()) || t.starts_with('#')) {
            let mut cut = i;
            if i >= 2 && (t == "FL" || t == "FLOOR") && is_ordinal_or_number(&toks[i - 1]) {
                cut = i - 1;
            }
            toks.truncate(cut);
            break;
        }
    }
    let f = &VOCAB.file;
    let mut out: Vec<&str> = toks
        .iter()
        .filter(|t| !VOCAB.drop_words.contains(t.as_str()))
        .map(|t| {
            f.ordinals
                .get(t)
                .or_else(|| f.words.get(t))
                .or_else(|| f.directions.get(t))
                .or_else(|| f.types.get(t))
                .map_or(t.as_str(), String::as_str)
        })
        .collect();
    if out.len() >= 3 && out[0] == "STATE" && out[1] == "RT" {
        out.remove(0); // "State Route 9" and "Route 9" are the same road
    }
    out.join(" ")
}

/// Relaxed forms of a canonical key for matching a street the nurse's file spells a little
/// differently: `(without the street type, without the type and a leading direction)`.
/// A trailing direction and `EXT` are also dropped from both.
pub fn relaxed_keys(key: &str) -> (String, String) {
    let mut t: Vec<&str> = key.split(' ').filter(|s| !s.is_empty()).collect();
    if t.len() > 1 && VOCAB.direction_values.contains(*t.last().unwrap_or(&"")) {
        t.pop();
    }
    if t.len() > 1 && t.last() == Some(&"EXT") {
        t.pop();
    }
    if t.len() > 1 && VOCAB.type_values.contains(*t.last().unwrap_or(&"")) {
        t.pop();
    }
    let no_type = t.join(" ");
    if t.len() > 1 && VOCAB.direction_values.contains(t[0]) {
        t.remove(0);
    }
    (no_type, t.join(" "))
}

/// An address split into the parts the geocoder uses.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedAddress {
    /// House number (the first of a range such as 12-14).
    pub number: Option<u32>,
    /// Letter or fraction after the number (`A`, `1/2`), upper case.
    pub suffix: String,
    /// Canonical street key; empty for a PO box or when there is no street.
    pub street_key: String,
    /// True for "PO Box 12"-style addresses (they have no street position).
    pub po_box: bool,
    /// City or town written in the address (normalized), when one was found.
    pub town: Option<String>,
    /// ZIP code (five digits) written in the address.
    pub zip: Option<String>,
}

fn is_zip(t: &str) -> Option<String> {
    let (head, tail) = match t.split_once('-') {
        Some((h, t)) => (h, Some(t)),
        None => (t, None),
    };
    let ok_head = head.len() == 5 && head.chars().all(|c| c.is_ascii_digit());
    let ok_tail = tail.is_none_or(|x| x.len() == 4 && x.chars().all(|c| c.is_ascii_digit()));
    (ok_head && ok_tail).then(|| head.to_string())
}

/// Plain-text form of a city, town, or place name for matching: upper case, no periods,
/// "Town of"/"City of" dropped, and a leading N/S/E/W/Mt spelled out ("S. Hadley").
pub fn normalize_place(text: &str) -> String {
    let mut t = words(text);
    while t.first().is_some_and(|w| w == "TOWN" || w == "CITY") && t.get(1).is_some_and(|w| w == "OF") {
        t.drain(..2);
    }
    if t.len() > 1 {
        let full = match t[0].as_str() {
            "N" => Some("NORTH"),
            "S" => Some("SOUTH"),
            "E" => Some("EAST"),
            "W" => Some("WEST"),
            "MT" => Some("MOUNT"),
            _ => None,
        };
        if let Some(f) = full {
            t[0] = f.to_string();
        }
    }
    t.join(" ")
}

fn starts_unit(part: &str) -> bool {
    let w = words(part);
    w.first()
        .is_some_and(|t| VOCAB.unit_words.contains(t.as_str()) || t.starts_with('#'))
}

fn po_box(line: &str) -> bool {
    let w = words(line);
    let joined = w.join(" ");
    let rest = if let Some(r) = joined.strip_prefix("POST OFFICE BOX ") {
        r
    } else if let Some(r) = joined.strip_prefix("PO BOX ") {
        r
    } else if let Some(r) = joined.strip_prefix("P O BOX ") {
        r
    } else if let Some(r) = joined.strip_prefix("BOX ") {
        r
    } else {
        // "P.O. Box": the periods are dropped, which leaves "PO"; "P O" is covered above.
        return false;
    };
    rest.chars().next().is_some_and(|c| c.is_ascii_digit())
}

/// Split an address (a street line, or a full "12 Elm St, Amherst, MA 01002") into its parts.
/// `is_place` says whether a normalized name (see [`normalize_place`]) is a town or known
/// place name; it is used to find a city written after the street without a comma.
pub fn parse_address(raw: &str, is_place: &dyn Fn(&str) -> bool) -> ParsedAddress {
    let mut parts: Vec<String> = raw
        .split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    let mut out = ParsedAddress::default();
    if parts.is_empty() {
        return out;
    }
    // Trailing ZIP, state, and (after a comma or when a ZIP or state was found) city.
    let mut found_tail = false;
    let mut last_words = words_with(parts.last().map_or("", String::as_str), true);
    if last_words.last().is_some_and(|w| w == "USA" || w == "US") && last_words.len() > 1 {
        last_words.pop();
    }
    if let Some(z) = last_words.last().and_then(|w| is_zip(w)) {
        out.zip = Some(z);
        last_words.pop();
        found_tail = true;
    }
    if last_words
        .last()
        .is_some_and(|w| VOCAB.state_words.contains(w.as_str()))
    {
        last_words.pop();
        found_tail = true;
    }
    if found_tail {
        if last_words.is_empty() {
            parts.pop();
            if parts.is_empty() {
                // The whole value was a ZIP and/or state.
                return out;
            }
        } else if let Some(last) = parts.last_mut() {
            *last = last_words.join(" ");
        }
    }
    // A city after the street line, separated by a comma.
    if parts.len() >= 2 {
        let last = parts.last().cloned().unwrap_or_default();
        let w = words(&last);
        let has_digit = w.iter().any(|t| t.chars().any(|c| c.is_ascii_digit()));
        if !w.is_empty() && !has_digit && !starts_unit(&last) {
            out.town = Some(normalize_place(&last));
            parts.pop();
        }
    }
    // The street line: the first part, or the next one when the first is only a unit.
    let mut line = parts[0].clone();
    if starts_unit(&line) && parts.len() > 1 {
        line = parts[1].clone();
    }
    let mut lw = words_with(&line, true);
    // A city written after the street on the same line ("12 Elm St Amherst MA 01002").
    if found_tail && out.town.is_none() && lw.len() > 2 {
        for n in (1..=3usize.min(lw.len() - 2)).rev() {
            let cand = lw[lw.len() - n..].join(" ");
            if is_place(&normalize_place(&cand)) {
                out.town = Some(normalize_place(&cand));
                lw.truncate(lw.len() - n);
                break;
            }
        }
    }
    let joined = lw.join(" ");
    if po_box(&joined) {
        out.po_box = true;
        return out;
    }
    // House number: 12, 12A, 12-14, 12-14A, 12 1/2.
    if let Some(first) = lw.first() {
        let digits: String = first.chars().take_while(char::is_ascii_digit).collect();
        if !digits.is_empty() {
            let rest = &first[digits.len()..];
            let mut suffix = String::new();
            let mut ok = true;
            if rest.is_empty() {
                // plain number
            } else if let Some(r2) = rest.strip_prefix('-') {
                let d2: String = r2.chars().take_while(char::is_ascii_digit).collect();
                let tail = &r2[d2.len()..];
                if d2.is_empty() || tail.len() > 1 || !tail.chars().all(|c| c.is_ascii_uppercase()) {
                    ok = tail.is_empty() && !d2.is_empty();
                } else {
                    suffix = tail.to_string();
                }
            } else if rest.len() == 1 && rest.chars().all(|c| c.is_ascii_uppercase()) {
                suffix = rest.to_string();
            } else if rest.chars().all(|c| c.is_ascii_uppercase()) {
                // "1297RR" (rear): no usable suffix
            } else {
                ok = false;
            }
            if ok {
                out.number = digits.parse::<u32>().ok().filter(|n| *n > 0);
                lw.remove(0);
                if suffix.is_empty()
                    && lw.first().is_some_and(|t| {
                        t.split_once('/').is_some_and(|(a, b)| {
                            !a.is_empty()
                                && !b.is_empty()
                                && a.chars().all(|c| c.is_ascii_digit())
                                && b.chars().all(|c| c.is_ascii_digit())
                        })
                    })
                {
                    suffix = lw.remove(0);
                }
                out.suffix = suffix;
            }
        }
    }
    out.street_key = normalize_street(&lw.join(" "));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Cases {
        cases: Vec<Case>,
    }
    #[derive(Deserialize)]
    struct Case {
        #[serde(rename = "in")]
        input: String,
        key: String,
    }

    /// The vectors shared with the Python index builder.
    #[test]
    fn normalizer_matches_the_shared_cases() {
        let cases: Cases =
            serde_json::from_str(include_str!("../../../../xtask/scripts/geocoder_normalizer_cases.json"))
                .unwrap();
        assert!(cases.cases.len() >= 40);
        for c in &cases.cases {
            assert_eq!(normalize_street(&c.input), c.key, "input {:?}", c.input);
        }
    }

    fn places(p: &str) -> bool {
        matches!(
            p,
            "AMHERST" | "PELHAM" | "NORTHAMPTON" | "NORTH ADAMS" | "SOUTH HADLEY" | "FLORENCE"
        )
    }

    fn parse(s: &str) -> ParsedAddress {
        parse_address(s, &places)
    }

    #[test]
    fn splits_number_street_and_unit() {
        let a = parse("837 Elm Street");
        assert_eq!((a.number, a.suffix.as_str(), a.street_key.as_str()), (Some(837), "", "ELM ST"));
        let a = parse("24A Forbes Ave");
        assert_eq!((a.number, a.suffix.as_str(), a.street_key.as_str()), (Some(24), "A", "FORBES AVE"));
        let a = parse("80-82 Williams St");
        assert_eq!((a.number, a.suffix.as_str()), (Some(80), ""));
        let a = parse("105-105A Main Street");
        assert_eq!((a.number, a.suffix.as_str()), (Some(105), "A"));
        let a = parse("94 1/2 Coleman Street");
        assert_eq!((a.number, a.suffix.as_str(), a.street_key.as_str()), (Some(94), "1/2", "COLEMAN ST"));
        let a = parse("268 Rear Tremont St");
        assert_eq!((a.number, a.street_key.as_str()), (Some(268), "TREMONT ST"));
        let a = parse("1297RR Washington Street");
        assert_eq!((a.number, a.suffix.as_str()), (Some(1297), ""));
        let a = parse("12 Elm St Apt 3B");
        assert_eq!((a.number, a.street_key.as_str()), (Some(12), "ELM ST"));
        let a = parse("12 Elm St, Apt 3B");
        assert_eq!((a.number, a.street_key.as_str(), a.town), (Some(12), "ELM ST", None));
        let a = parse("Apt 3B, 12 Elm St");
        assert_eq!((a.number, a.street_key.as_str()), (Some(12), "ELM ST"));
        let a = parse("12 N. Pleasant St. #4");
        assert_eq!((a.number, a.street_key.as_str()), (Some(12), "N PLEASANT ST"));
        let a = parse("Main Street");
        assert_eq!((a.number, a.street_key.as_str()), (None, "MAIN ST"));
        let a = parse("0 Pond Rd");
        assert_eq!(a.number, None);
    }

    #[test]
    fn reads_city_state_and_zip() {
        let a = parse("12 Elm St, Amherst, MA 01002");
        assert_eq!(a.town.as_deref(), Some("AMHERST"));
        assert_eq!(a.zip.as_deref(), Some("01002"));
        assert_eq!(a.street_key, "ELM ST");
        let a = parse("12 Elm St, Amherst MA 01002-1234");
        assert_eq!((a.town.as_deref(), a.zip.as_deref()), (Some("AMHERST"), Some("01002")));
        let a = parse("12 Elm St Amherst MA 01002");
        assert_eq!((a.town.as_deref(), a.zip.as_deref(), a.street_key.as_str()), (Some("AMHERST"), Some("01002"), "ELM ST"));
        let a = parse("5 Main St North Adams Massachusetts 01247");
        assert_eq!((a.town.as_deref(), a.street_key.as_str()), (Some("NORTH ADAMS"), "MAIN ST"));
        let a = parse("5 Main St, S. Hadley, MA");
        assert_eq!(a.town.as_deref(), Some("SOUTH HADLEY"));
        // A street named like a town is not mistaken for the city without a ZIP or state.
        let a = parse("12 Amherst Road");
        assert_eq!((a.town, a.street_key.as_str()), (None, "AMHERST RD"));
        let a = parse("12 Pelham Rd Apt 2");
        assert_eq!(a.town, None);
        let a = parse("01002");
        assert_eq!((a.zip.as_deref(), a.number), (Some("01002"), None));
    }

    #[test]
    fn recognizes_po_boxes() {
        for s in ["PO Box 12", "P.O. Box 12, Pelham, MA 01002", "P O Box 7", "Post Office Box 3", "Box 14"] {
            assert!(parse(s).po_box, "{s}");
        }
        assert!(!parse("12 Box Road").po_box);
        assert!(!parse("Boxwood Lane").po_box);
        assert_eq!(parse("PO Box 12, Pelham, MA 01002").town.as_deref(), Some("PELHAM"));
    }

    #[test]
    fn relaxed_keys_drop_type_and_direction() {
        assert_eq!(relaxed_keys("N PLEASANT ST"), ("N PLEASANT".into(), "PLEASANT".into()));
        assert_eq!(relaxed_keys("WASHINGTON ST EXT"), ("WASHINGTON".into(), "WASHINGTON".into()));
        assert_eq!(relaxed_keys("PLEASANT ST N"), ("PLEASANT".into(), "PLEASANT".into()));
        assert_eq!(relaxed_keys("ST JAMES AVE"), ("ST JAMES".into(), "ST JAMES".into()));
        assert_eq!(relaxed_keys("RT 9"), ("RT 9".into(), "RT 9".into()));
        assert_eq!(relaxed_keys("ELM"), ("ELM".into(), "ELM".into()));
    }

    #[test]
    fn place_names_are_normalized() {
        assert_eq!(normalize_place("Town of Amherst"), "AMHERST");
        assert_eq!(normalize_place("S. Hadley"), "SOUTH HADLEY");
        assert_eq!(normalize_place("N Adams"), "NORTH ADAMS");
        assert_eq!(normalize_place("Mt. Washington"), "MOUNT WASHINGTON");
        assert_eq!(normalize_place("Manchester-by-the-Sea"), "MANCHESTER BY THE SEA");
    }
}
