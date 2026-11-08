//! Flesch-Kincaid grade level, used to report the reading level of
//! family-facing text.
//!
//! Grade = 0.39 × (words ÷ sentences) + 11.8 × (syllables ÷ words) − 15.59
//! (Kincaid JP, Fishburne RP, Rogers RL, Chissom BS. Derivation of new
//! readability formulas for Navy enlisted personnel. Research Branch Report
//! 8-75, 1975). Syllables are estimated with a vowel-group heuristic (silent
//! final "e" dropped, "-le" endings kept, at least one per word), which is how
//! most word processors approximate them; expect about ±1 grade versus
//! dictionary syllable counts.

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
    let w: String = word
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if w.is_empty() {
        return 0;
    }
    if w.len() <= 3 {
        return 1;
    }
    let is_vowel = |c: char| matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y');
    let chars: Vec<char> = w.chars().collect();
    let mut count = 0;
    let mut prev = false;
    for &c in &chars {
        let v = is_vowel(c);
        if v && !prev {
            count += 1;
        }
        prev = v;
    }
    let n = chars.len();
    if chars[n - 1] == 'e' && !(n >= 3 && chars[n - 2] == 'l' && !is_vowel(chars[n - 3])) && count > 1 {
        count -= 1;
    }
    if (w.ends_with("ed") || w.ends_with("es")) && n > 4 && !matches!(chars[n - 3], 't' | 'd' | 's' | 'x' | 'z' | 'c' | 'g') && count > 1 {
        count -= 1;
    }
    count.max(1)
}

/// Flesch-Kincaid grade for a passage. Numbers count as one-syllable words.
pub fn flesch_kincaid(text: &str) -> Readability {
    let sentences = text
        .split(['.', '!', '?', ';', ':', '\n'])
        .filter(|s| s.split_whitespace().any(|w| w.chars().any(char::is_alphanumeric)))
        .count()
        .max(1);
    let words: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || c == '/' || c == '—')
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .collect();
    let n_words = words.len().max(1);
    let syl: usize = words
        .iter()
        .map(|w| if w.chars().any(|c| c.is_ascii_alphabetic()) { syllables(w) } else { 1 })
        .sum();
    let grade = 0.39 * (n_words as f64 / sentences as f64) + 11.8 * (syl as f64 / n_words as f64) - 15.59;
    Readability {
        words: words.len(),
        sentences,
        syllables: syl,
        fk_grade: (grade * 10.0).round() / 10.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_syllables_reasonably() {
        for (w, n) in [("cat", 1), ("vaccine", 2), ("immunization", 5), ("table", 2), ("made", 1), ("protected", 3), ("baby", 2)] {
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
        let hard = flesch_kincaid("Immunization recommendations necessitate comprehensive individualized consideration.");
        assert!(hard.fk_grade > 16.0, "{}", hard.fk_grade);
    }
}
