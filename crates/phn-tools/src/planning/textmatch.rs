//! Small text-matching helpers shared by the Phase 16 interventions tools.
//!
//! Matching is on whole words or phrases (never substrings: "can't" must not match "can"),
//! and works on characters, so accented text is safe.

use std::collections::BTreeSet;

/// Words that carry no meaning for matching.
const STOP: &[&str] = &[
    "a", "about", "an", "and", "are", "as", "at", "be", "been", "by", "for", "from", "has", "have",
    "in", "is", "it", "its", "of", "on", "or", "our", "that", "the", "their", "them", "they",
    "this", "to", "was", "we", "were", "which", "with", "who", "people", "program", "programs",
    "health", "increase", "improve", "reduce", "use",
];

/// Lower-case words (letters and digits; apostrophes kept inside words).
pub fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        if c.is_alphanumeric() || c == '\'' || c == '\u{2019}' {
            cur.extend(
                c.to_lowercase()
                    .map(|x| if x == '\u{2019}' { '\'' } else { x }),
            );
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out.into_iter()
        .map(|w| w.trim_matches('\'').to_string())
        .filter(|w| !w.is_empty())
        .collect()
}

/// A light stem: drops a plural or "-ing"/"-ed" ending from words of five or more letters.
pub fn stem(word: &str) -> String {
    let n = word.chars().count();
    if n < 5 {
        return word.to_string();
    }
    for suffix in ["ies", "ing", "es", "ed", "s"] {
        if let Some(base) = word.strip_suffix(suffix) {
            let b = base.chars().count();
            if b >= 4 {
                return if suffix == "ies" {
                    format!("{base}y")
                } else {
                    base.to_string()
                };
            }
        }
    }
    word.to_string()
}

/// Stemmed content words of a text.
pub fn stems(text: &str) -> BTreeSet<String> {
    words(text)
        .into_iter()
        .filter(|w| !STOP.contains(&w.as_str()))
        .map(|w| stem(&w))
        .collect()
}

/// True when `phrase` occurs in `text` as whole words (both are compared lower-cased).
pub fn contains_phrase(text: &str, phrase: &str) -> bool {
    let t = words(text);
    let p = words(phrase);
    if p.is_empty() || t.len() < p.len() {
        return false;
    }
    t.windows(p.len()).any(|w| w == p.as_slice())
}

/// Like [`contains_phrase`] but also accepts a plural or "-ing" form of each word
/// (so the signal "barrier" matches "barriers").
pub fn contains_phrase_stemmed(text: &str, phrase: &str) -> bool {
    let t: Vec<String> = words(text).iter().map(|w| stem(w)).collect();
    let p: Vec<String> = words(phrase).iter().map(|w| stem(w)).collect();
    if p.is_empty() || t.len() < p.len() {
        return false;
    }
    t.windows(p.len()).any(|w| w == p.as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phrases_match_whole_words_only() {
        assert!(contains_phrase("They can't afford it", "can't afford"));
        assert!(!contains_phrase("The cost of transport", "cos"));
        assert!(!contains_phrase("scoop of ice cream", "co"));
        assert!(contains_phrase("No ride to the clinic", "no ride"));
        assert!(!contains_phrase("", "x"));
    }

    #[test]
    fn stemmed_matching_accepts_plurals() {
        assert!(contains_phrase_stemmed("many barriers to care", "barrier"));
        assert!(contains_phrase_stemmed("reminders were missed", "reminder"));
        assert!(!contains_phrase_stemmed("barrel", "barrier"));
        assert_eq!(stem("studies"), "study");
        assert_eq!(stem("screening"), "screen");
        assert_eq!(stem("care"), "care");
    }

    #[test]
    fn accented_and_curly_apostrophes_are_safe() {
        assert!(contains_phrase("Don\u{2019}t know how", "don't know"));
        assert!(contains_phrase("Educación para mamás", "educación"));
        assert_eq!(words("Café, au lait!"), vec!["café", "au", "lait"]);
    }

    #[test]
    fn stems_drop_stop_words() {
        let s = stems("The program to reduce screening barriers");
        assert!(s.contains("screen") && s.contains("barrier") && !s.contains("the"));
    }
}
