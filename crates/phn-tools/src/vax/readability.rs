//! Flesch-Kincaid grade level, used to report the reading level of
//! family-facing text (VAX-09).
//!
//! The text statistics now live in [`crate::comms::textstats`], shared with
//! COMM-01 `check_readability`; this module keeps the small interface VAX-09
//! uses. Grade = 0.39 × (words ÷ sentences) + 11.8 × (syllables ÷ words) −
//! 15.59 (Kincaid JP, Fishburne RP, Rogers RL, Chissom BS. Derivation of new
//! readability formulas for Navy enlisted personnel. Research Branch Report
//! 8-75, 1975). Each line of the text is treated as its own block, so a
//! spoken script with one sentence or question per line is counted line by
//! line.

use crate::comms::textstats;

/// Counts behind a readability score.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Readability {
    pub words: usize,
    pub sentences: usize,
    pub syllables: usize,
    pub fk_grade: f64,
}

/// Estimate syllables in one word.
pub fn syllables(word: &str) -> usize {
    if !word.chars().any(|c| c.is_ascii_alphabetic()) {
        return 0;
    }
    textstats::word_syllables(word)
}

/// Flesch-Kincaid grade for a passage. Numbers count as one-syllable words.
pub fn flesch_kincaid(text: &str) -> Readability {
    let per_line = text.lines().collect::<Vec<_>>().join("\n\n");
    let c = textstats::parse(&per_line).counts();
    let sentences = c.sentences.max(1);
    let n_words = c.words.max(1);
    let grade = 0.39 * (n_words as f64 / sentences as f64)
        + 11.8 * (c.syllables as f64 / n_words as f64)
        - 15.59;
    Readability {
        words: c.words,
        sentences,
        syllables: c.syllables,
        fk_grade: (grade * 10.0).round() / 10.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_syllables_reasonably() {
        for (w, n) in [
            ("cat", 1),
            ("vaccine", 2),
            ("immunization", 5),
            ("table", 2),
            ("made", 1),
            ("protected", 3),
            ("baby", 2),
        ] {
            assert_eq!(syllables(w), n, "{w}");
        }
    }

    /// Hand-computed: "The cat sat on the mat. The dog ran." has 9 words, 2
    /// sentences, 9 syllables: 0.39 × 4.5 + 11.8 × 1 − 15.59 = −2.035.
    #[test]
    fn matches_a_hand_computed_example() {
        let r = flesch_kincaid("The cat sat on the mat. The dog ran.");
        assert_eq!((r.words, r.sentences, r.syllables), (9, 2, 9));
        assert!((r.fk_grade - (-2.0)).abs() < 0.11, "{}", r.fk_grade);
        let hard = flesch_kincaid(
            "Immunization recommendations necessitate comprehensive individualized consideration.",
        );
        assert!(hard.fk_grade > 16.0, "{}", hard.fk_grade);
    }
}
