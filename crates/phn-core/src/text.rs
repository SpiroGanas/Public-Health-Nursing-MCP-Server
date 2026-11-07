//! Text normalization and fuzzy matching used by lookups (towns, districts,
//! reportable conditions).

use unicode_normalization::UnicodeNormalization;

/// Lower-case, strip accents and punctuation, and collapse whitespace.
///
/// `"Manchester-by-the-Sea"` becomes `"manchester by the sea"`;
/// `"E. coli O157:H7"` becomes `"e coli o157 h7"`.
pub fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = true;
    for c in s.nfkd() {
        if unicode_normalization::char::is_combining_mark(c) {
            continue;
        }
        if c.is_alphanumeric() {
            for lc in c.to_lowercase() {
                out.push(lc);
            }
            last_space = false;
        } else if c == '\'' || c == '’' {
            // "Hansen's" -> "hansens"
            continue;
        } else if !last_space {
            out.push(' ');
            last_space = true;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Similarity between a normalized query and a normalized key, in `[0, 1]`.
pub fn similarity(query: &str, key: &str) -> f64 {
    if query.is_empty() || key.is_empty() {
        return 0.0;
    }
    if query == key {
        return 1.0;
    }
    let mut best: f64 = 0.0;

    // Whole-phrase containment in either direction. Longer keys that cover
    // more of the query score higher, so "salmonella typhi" beats "salmonella".
    let padded_q = format!(" {query} ");
    let padded_k = format!(" {key} ");
    if padded_q.contains(&padded_k) {
        let cover = key.len() as f64 / query.len() as f64;
        best = best.max(0.80 + 0.15 * cover);
    } else if padded_k.contains(&padded_q) && query.len() >= 3 {
        let cover = query.len() as f64 / key.len() as f64;
        best = best.max(0.75 + 0.15 * cover);
    }

    // Whole-string typo tolerance.
    best = best.max(0.90 * strsim::jaro_winkler(query, key));

    // Token-level typo tolerance: every meaningful query token must find a
    // close token in the key.
    let q_tokens: Vec<&str> = query.split(' ').filter(|t| t.len() >= 3).collect();
    let k_tokens: Vec<&str> = key.split(' ').collect();
    if !q_tokens.is_empty() {
        let total: f64 = q_tokens
            .iter()
            .map(|qt| {
                k_tokens
                    .iter()
                    .map(|kt| strsim::jaro_winkler(qt, kt))
                    .fold(0.0, f64::max)
            })
            .sum();
        let avg = total / q_tokens.len() as f64;
        best = best.max(0.85 * avg);
    }
    best
}

/// A ranked candidate from [`rank`].
#[derive(Debug, Clone, PartialEq)]
pub struct Ranked {
    /// Index into the candidate slice.
    pub index: usize,
    /// Best similarity score across the candidate's keys.
    pub score: f64,
    /// The key that produced the best score (original spelling).
    pub matched_on: String,
}

/// Rank candidates by their best-matching key.
///
/// `keys` returns every name, alias, or term a candidate can be matched on.
/// Results are sorted by descending score and filtered by `min_score`.
pub fn rank<T, F>(query: &str, items: &[T], keys: F, min_score: f64) -> Vec<Ranked>
where
    F: Fn(&T) -> Vec<String>,
{
    let q = normalize(query);
    let mut out: Vec<Ranked> = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            keys(item)
                .into_iter()
                .map(|k| (similarity(&q, &normalize(&k)), k))
                .max_by(|a, b| a.0.total_cmp(&b.0))
                .filter(|(score, _)| *score >= min_score)
                .map(|(score, matched_on)| Ranked {
                    index,
                    score,
                    matched_on,
                })
        })
        .collect();
    out.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.index.cmp(&b.index)));
    out
}

/// Convert an upper-case name such as `"MANCHESTER-BY-THE-SEA"` to title case
/// (`"Manchester-by-the-Sea"`).
pub fn title_case(name: &str) -> String {
    const SMALL: [&str; 5] = ["by", "the", "of", "and", "on"];
    let lower = name.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut word = String::new();
    let mut first = true;
    let flush = |word: &mut String, out: &mut String, first: &mut bool| {
        if word.is_empty() {
            return;
        }
        if !*first && SMALL.contains(&word.as_str()) {
            out.push_str(word);
        } else {
            let mut cs = word.chars();
            if let Some(c) = cs.next() {
                out.extend(c.to_uppercase());
                out.push_str(cs.as_str());
            }
        }
        word.clear();
        *first = false;
    };
    for c in lower.chars() {
        if c == ' ' || c == '-' {
            flush(&mut word, &mut out, &mut first);
            out.push(c);
        } else {
            word.push(c);
        }
    }
    flush(&mut word, &mut out, &mut first);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_punctuation_and_accents() {
        assert_eq!(normalize("E. coli O157:H7"), "e coli o157 h7");
        assert_eq!(normalize("  Hansen's disease "), "hansens disease");
        assert_eq!(normalize("Señora"), "senora");
    }

    #[test]
    fn ranking_prefers_specific_phrase() {
        let items = vec![
            ("Salmonellosis", vec!["salmonella"]),
            ("Typhoid fever", vec!["salmonella typhi"]),
        ];
        let ranked = rank(
            "Salmonella Typhi isolated",
            &items,
            |(n, a)| {
                let mut k = vec![n.to_string()];
                k.extend(a.iter().map(|s| s.to_string()));
                k
            },
            0.5,
        );
        assert_eq!(ranked[0].index, 1);
    }

    #[test]
    fn tolerates_typos() {
        assert!(similarity(&normalize("pertusis"), &normalize("pertussis")) > 0.85);
        assert!(similarity(&normalize("measels"), &normalize("measles")) > 0.85);
        assert!(similarity(&normalize("asthma"), &normalize("measles")) < 0.80);
    }

    #[test]
    fn title_cases_town_names() {
        assert_eq!(title_case("MANCHESTER-BY-THE-SEA"), "Manchester-by-the-Sea");
        assert_eq!(title_case("NORTH ATTLEBOROUGH"), "North Attleborough");
    }
}
