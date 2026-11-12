//! Helpers shared by the Phase 18 ethics tools (EBP-07, POL-05, POL-06):
//! cleaning of free-text arguments, refusal of text that looks like it
//! identifies a person, and writing the draft documents to /data/output.

use phn_core::{PhnError, Result};

use crate::context::Context;
use crate::school::templates;

/// Trim a free-text argument and enforce a length limit (in characters).
pub fn clean_text(label: &str, text: &str, max: usize) -> Result<String> {
    let t = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() > max {
        return Err(PhnError::InvalidInput(format!(
            "{label} is too long (at most {max} characters)"
        )));
    }
    Ok(t)
}

/// Refuse text that looks like it identifies a person. The message never
/// repeats the text.
pub fn refuse_identifiers(label: &str, text: &str) -> Result<()> {
    use phn_local::phi::{Identifier as I, scan_text};
    let kinds: Vec<&str> = scan_text(text)
        .into_iter()
        .filter(|f| {
            matches!(
                f.kind,
                I::Ssn
                    | I::Phone
                    | I::Email
                    | I::StreetAddress
                    | I::MedicalRecordNumber
                    | I::HealthPlanNumber
                    | I::BirthDate
            )
        })
        .map(|f| f.kind.label())
        .collect();
    if kinds.is_empty() {
        Ok(())
    } else {
        Err(PhnError::Privacy(format!(
            "{label} appears to contain {}. Describe the project, the population, or the decision, \
             not an individual person.",
            kinds.join(", ")
        )))
    }
}

/// Clean an optional free-text argument: trimmed, limited, and checked for
/// identifiers. Empty text is `None`.
pub fn optional_text(label: &str, text: &Option<String>, max: usize) -> Result<Option<String>> {
    match text {
        Some(t) if !t.trim().is_empty() => {
            let c = clean_text(label, t, max)?;
            refuse_identifiers(label, &c)?;
            Ok(Some(c))
        }
        _ => Ok(None),
    }
}

/// Write a Markdown draft as a DOCX file in the output folder and return its
/// display path. The first level-1 heading becomes the document title.
pub fn write_docx(ctx: &Context, file_stem: &str, title: &str, markdown: &str) -> Result<String> {
    let doc = templates::document(ctx, markdown, "en-US", title);
    let bytes = phn_docs::docx::to_docx(&doc)?;
    let sandbox = ctx.sandbox()?;
    let path = sandbox.new_output_file(&format!("{file_stem}.docx"))?;
    std::fs::write(&path, bytes)?;
    Ok(sandbox.display_path(&path))
}

/// Normalize text for keyword matching: lower case words with a trailing
/// plural "s" removed, so "vaccines" matches "vaccine".
pub fn stem_words(text: &str) -> Vec<String> {
    phn_core::text::normalize(text)
        .split_whitespace()
        .map(|w| {
            match w.strip_suffix('s') {
                Some(stem) if w.chars().count() > 3 && !w.ends_with("ss") => stem.to_string(),
                _ => w.to_string(),
            }
        })
        .collect()
}

/// True when the words of `phrase` appear contiguously, in order, in `words`.
pub fn contains_phrase(words: &[String], phrase: &str) -> bool {
    let p = stem_words(phrase);
    if p.is_empty() || p.len() > words.len() {
        return false;
    }
    words.windows(p.len()).any(|w| w == p.as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phrases_match_on_word_boundaries_with_plurals() {
        let w = stem_words("Can an LPN give vaccines under standing orders?");
        assert!(contains_phrase(&w, "vaccine"));
        assert!(contains_phrase(&w, "standing order"));
        assert!(contains_phrase(&w, "lpn"));
        assert!(!contains_phrase(&w, "vaccin"));
        assert!(!contains_phrase(&w, "rn"));
        assert!(!contains_phrase(&w, ""));
    }

    #[test]
    fn identifiers_are_refused_without_echo() {
        let e = refuse_identifiers("situation", "call 617-555-0142 about the family").unwrap_err();
        let m = e.to_string();
        assert!(m.contains("phone") || m.contains("Phone") || m.contains("telephone"), "{m}");
        assert!(!m.contains("617-555-0142"));
        assert!(refuse_identifiers("situation", "an outbreak of measles in a school").is_ok());
    }

    #[test]
    fn text_is_trimmed_and_limited() {
        assert_eq!(clean_text("x", "  a   b \n c ", 10).unwrap(), "a b c");
        assert!(clean_text("x", &"é".repeat(11), 10).is_err());
        assert!(clean_text("x", &"é".repeat(10), 10).is_ok());
    }
}
