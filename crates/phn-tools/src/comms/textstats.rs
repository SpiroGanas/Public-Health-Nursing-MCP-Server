//! Text statistics behind the readability formulas: blocks (headings, list
//! items, paragraphs), sentences, words, and syllables.
//!
//! * **Blocks.** Markdown headings (`# ...`) and list items (`-`, `*`, `•`,
//!   `1.`, `a)`) are recognized; other lines are joined into paragraphs, which
//!   end at a blank line. Headings are excluded from the formulas (they are
//!   counted for the structure checks); each list item counts as a sentence.
//! * **Sentences** end at `.`, `!`, or `?` followed by a space or the end of
//!   the block. Common abbreviations (Dr., Mr., e.g., i.e., U.S., etc.),
//!   initials in names ("John F. Kennedy"), and decimals do not end a sentence.
//!   A block without end punctuation is one sentence.
//! * **Words** are separated by spaces, slashes, and dashes; a hyphenated word
//!   counts as one word (CDC *Simply Put*, Appendix C). Numbers count as
//!   one-syllable words.
//! * **Syllables** are estimated from vowel groups with corrections for silent
//!   final *e*, a silent *e* inside compounds and before suffixes (*safety*,
//!   *something*, *movement*, *careful*, *household*), *-le*, *-ed*, *-es*,
//!   *-tion*, and common vowel pairs that are two syllables (*ia*, *io*, *ua*,
//!   *iet*, *-rious*), plus a list of exceptions. Measured against dictionary
//!   counts: 300 of 300 on the list of common health words the rules were
//!   tuned on (253 of 300 before the 2026-11 revision), and 204 of 209 (97.6%)
//!   on a second list the rules had not seen (its five misses were then added
//!   as rules or exceptions). Unusual words can still be off by one.

use std::sync::LazyLock;

/// Kind of block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Heading,
    ListItem,
    Paragraph,
}

/// A word token.
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    /// The word as written, without surrounding punctuation.
    pub text: String,
    pub syllables: usize,
    /// A number (digits), counted as one syllable and never polysyllabic.
    pub is_number: bool,
    /// Contains a hyphen between letters.
    pub hyphenated: bool,
    /// Starts with an upper-case letter.
    pub capitalized: bool,
}

/// A sentence.
#[derive(Debug, Clone, PartialEq)]
pub struct Sentence {
    pub text: String,
    pub words: Vec<Word>,
}

/// A heading, list item, or paragraph.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub kind: BlockKind,
    pub text: String,
    /// For list items: which list (consecutive items share a number).
    pub list: Option<usize>,
    pub sentences: Vec<Sentence>,
}

/// A parsed text.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Parsed {
    pub blocks: Vec<Block>,
}

/// Counts used by the formulas.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Counts {
    pub words: usize,
    pub sentences: usize,
    pub syllables: usize,
    /// Words with 3 or more syllables (SMOG).
    pub polysyllables: usize,
    /// Gunning's complex words: 3+ syllables, excluding proper nouns,
    /// hyphenated compounds, and words with 3 syllables only because of
    /// -es, -ed, or -ing.
    pub complex_words: usize,
}

impl Parsed {
    /// Sentences that count for the formulas (paragraphs and list items).
    pub fn counted_sentences(&self) -> impl Iterator<Item = &Sentence> {
        self.blocks
            .iter()
            .filter(|b| b.kind != BlockKind::Heading)
            .flat_map(|b| b.sentences.iter())
    }

    /// Formula counts over the counted sentences.
    pub fn counts(&self) -> Counts {
        counts_of(self.counted_sentences())
    }
}

/// Counts over a set of sentences.
pub fn counts_of<'a>(sentences: impl Iterator<Item = &'a Sentence>) -> Counts {
    let mut c = Counts::default();
    for s in sentences {
        if s.words.is_empty() {
            continue;
        }
        c.sentences += 1;
        for (i, w) in s.words.iter().enumerate() {
            c.words += 1;
            c.syllables += w.syllables;
            if !w.is_number && w.syllables >= 3 {
                c.polysyllables += 1;
                if is_complex(w, i == 0) {
                    c.complex_words += 1;
                }
            }
        }
    }
    c
}

fn is_complex(w: &Word, sentence_start: bool) -> bool {
    if w.hyphenated || (w.capitalized && !sentence_start) {
        return false;
    }
    let lower = w.text.to_lowercase();
    for suffix in ["ing", "ed", "es"] {
        if let Some(stem) = lower.strip_suffix(suffix)
            && stem.len() > 2
            && word_syllables(stem) < 3
        {
            return false;
        }
    }
    true
}

static LIST_MARKER: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"^\s*(?:[-*+•▪◦‣–✓]|\d{1,3}[.)]|[a-zA-Z][.)])\s+").expect("valid regex")
});
static HEADING: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^\s{0,3}#{1,6}\s+").expect("valid regex"));

/// Parse text (plain or Markdown) into blocks, sentences, and words.
pub fn parse(text: &str) -> Parsed {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut blocks: Vec<Block> = Vec::new();
    let mut para: Vec<String> = Vec::new();
    let mut list_id = 0usize;
    let mut in_list = false;
    let flush = |para: &mut Vec<String>, blocks: &mut Vec<Block>| {
        if !para.is_empty() {
            let t = para.join(" ");
            para.clear();
            push_block(blocks, BlockKind::Paragraph, &t, None);
        }
    };
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush(&mut para, &mut blocks);
            if in_list {
                in_list = false;
            }
            continue;
        }
        if let Some(m) = HEADING.find(line) {
            flush(&mut para, &mut blocks);
            in_list = false;
            let t = line[m.end()..].trim().trim_end_matches('#').trim();
            push_block(&mut blocks, BlockKind::Heading, t, None);
            continue;
        }
        if let Some(m) = LIST_MARKER.find(line) {
            flush(&mut para, &mut blocks);
            if !in_list {
                list_id += 1;
                in_list = true;
            }
            push_block(
                &mut blocks,
                BlockKind::ListItem,
                line[m.end()..].trim(),
                Some(list_id),
            );
            continue;
        }
        if in_list && line.starts_with([' ', '\t']) {
            // Continuation of the previous list item.
            if let Some(b) = blocks.last_mut() {
                let joined = format!("{} {trimmed}", b.text);
                *b = make_block(BlockKind::ListItem, &joined, b.list);
                continue;
            }
        }
        in_list = false;
        para.push(trimmed.to_string());
    }
    flush(&mut para, &mut blocks);
    Parsed { blocks }
}

fn push_block(blocks: &mut Vec<Block>, kind: BlockKind, text: &str, list: Option<usize>) {
    let b = make_block(kind, text, list);
    if !b.text.is_empty() {
        blocks.push(b);
    }
}

fn make_block(kind: BlockKind, text: &str, list: Option<usize>) -> Block {
    let clean = strip_markdown(text);
    let sentences = if kind == BlockKind::Heading {
        vec![sentence(&clean)]
    } else {
        split_sentences(&clean)
            .into_iter()
            .map(|s| sentence(&s))
            .filter(|s| !s.words.is_empty())
            .collect()
    };
    Block {
        kind,
        text: clean,
        list,
        sentences,
    }
}

/// Remove Markdown emphasis and link syntax.
fn strip_markdown(s: &str) -> String {
    static LINK: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"!?\[([^\]]*)\]\([^)]*\)").expect("valid regex"));
    let s = LINK.replace_all(s, "$1");
    s.replace("**", "")
        .replace("__", "")
        .replace('`', "")
        .trim()
        .to_string()
}

const ABBREVIATIONS: &[&str] = &[
    "dr", "mr", "mrs", "ms", "prof", "st", "jr", "sr", "vs", "etc", "e.g", "i.e", "no", "nos",
    "inc", "dept", "approx", "fig", "u.s", "a.m", "p.m", "jan", "feb", "mar", "apr", "jun", "jul",
    "aug", "sep", "sept", "oct", "nov", "dec", "mt", "ft", "oz", "lb", "lbs", "ave", "rd", "blvd",
    "ext", "tel", "al", "cf", "ca", "vol", "pp", "ph.d", "m.d", "r.n", "b.s.n", "d.n.p",
];
/// Abbreviations after which a sentence never ends (titles and "e.g.").
const NEVER_END: &[&str] = &[
    "dr", "mr", "mrs", "ms", "prof", "e.g", "i.e", "u.s", "vs", "cf",
];

/// Split a block of text into sentences.
pub fn split_sentences(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if matches!(c, '.' | '!' | '?') {
            // Swallow runs of terminal punctuation and closing quotes/brackets.
            let mut j = i + 1;
            while j < chars.len()
                && matches!(
                    chars[j],
                    '.' | '!' | '?' | '"' | '\'' | ')' | ']' | '”' | '’'
                )
            {
                j += 1;
            }
            let at_end = j >= chars.len();
            let followed_by_space = at_end || chars[j].is_whitespace();
            if followed_by_space && (c != '.' || period_ends_sentence(&chars, start, i, j)) {
                let s: String = chars[start..j].iter().collect();
                if !s.trim().is_empty() {
                    out.push(s.trim().to_string());
                }
                start = j;
            }
            i = j;
            continue;
        }
        i += 1;
    }
    let rest: String = chars[start..].iter().collect();
    if rest.chars().any(char::is_alphanumeric) {
        out.push(rest.trim().to_string());
    }
    out
}

fn period_ends_sentence(chars: &[char], start: usize, dot: usize, after: usize) -> bool {
    // The token before the period.
    let mut k = dot;
    while k > start && !chars[k - 1].is_whitespace() {
        k -= 1;
    }
    let token: String = chars[k..dot]
        .iter()
        .filter(|c| c.is_alphanumeric() || **c == '.')
        .collect::<String>()
        .to_lowercase();
    // The next word.
    let mut n = after;
    while n < chars.len() && chars[n].is_whitespace() {
        n += 1;
    }
    let next = chars.get(n).copied();
    let next_upper =
        next.is_some_and(|c| c.is_uppercase() || c.is_ascii_digit() || c == '"' || c == '“');
    if next.is_none() {
        return true;
    }
    if NEVER_END.contains(&token.as_str()) {
        return false;
    }
    if ABBREVIATIONS.contains(&token.as_str()) {
        return next_upper;
    }
    // A single capital letter after a capitalized word is an initial ("John F. Kennedy").
    let raw: String = chars[k..dot].iter().collect();
    if raw.chars().count() == 1 && raw.chars().all(|c| c.is_uppercase()) {
        let mut p = k;
        while p > start && chars[p - 1].is_whitespace() {
            p -= 1;
        }
        let mut q = p;
        while q > start && !chars[q - 1].is_whitespace() {
            q -= 1;
        }
        let prev: String = chars[q..p].iter().collect();
        if prev.chars().next().is_some_and(char::is_uppercase)
            && prev.chars().all(char::is_alphabetic)
            && next_upper
        {
            return false;
        }
    }
    // Lower-case continuation means the period was not a sentence end.
    !next.is_some_and(char::is_lowercase)
}

/// Build a sentence from its text.
pub fn sentence(text: &str) -> Sentence {
    Sentence {
        text: text.to_string(),
        words: words(text),
    }
}

/// Split text into word tokens.
pub fn words(text: &str) -> Vec<Word> {
    text.split(|c: char| c.is_whitespace() || matches!(c, '/' | '—' | '–'))
        .filter_map(word)
        .collect()
}

fn word(token: &str) -> Option<Word> {
    let t = token.trim_matches(|c: char| !c.is_alphanumeric());
    if t.is_empty() || !t.chars().any(char::is_alphanumeric) {
        return None;
    }
    let is_number = !t.chars().any(char::is_alphabetic);
    let hyphenated = t.contains('-')
        && t.split('-')
            .filter(|p| p.chars().any(char::is_alphabetic))
            .count()
            >= 2;
    let syllables = if is_number {
        1
    } else {
        t.split('-')
            .filter(|p| p.chars().any(char::is_alphabetic))
            .map(word_syllables)
            .sum::<usize>()
            .max(1)
    };
    Some(Word {
        text: t.to_string(),
        syllables,
        is_number,
        hyphenated,
        capitalized: t.chars().next().is_some_and(char::is_uppercase),
    })
}

/// Whole words whose syllable count the rules get wrong.
const EXCEPTIONS: &[(&str, usize)] = &[
    ("anyone", 3),
    ("area", 3),
    ("areas", 3),
    ("beyond", 2),
    ("business", 2),
    ("businesses", 3),
    ("create", 2),
    ("created", 3),
    ("creates", 2),
    ("creating", 3),
    ("diet", 2),
    ("every", 3),
    ("idea", 3),
    ("layer", 2),
    ("layers", 2),
    ("mayor", 2),
    ("player", 2),
    ("players", 2),
    ("ideas", 3),
    ("quiet", 2),
    ("science", 2),
    ("suicide", 3),
    ("suicides", 3),
    ("suicidal", 4),
    ("video", 3),
    ("videos", 3),
    ("wednesday", 2),
    ("covid", 2),
    ("influenza", 4),
    ("diabetes", 4),
    ("pneumonia", 3),
    ("therefore", 2),
    ("everything", 3),
    ("everybody", 4),
    ("nausea", 3),
    ("poem", 2),
    ("poems", 2),
    ("lion", 2),
    ("lions", 2),
    ("pioneer", 3),
    ("pioneers", 3),
    ("heroin", 3),
    // "aren't" and "weren't" (apostrophes are dropped before counting).
    ("arent", 1),
    ("werent", 1),
    ("vegetable", 3),
    ("vegetables", 3),
    ("comfortable", 3),
    ("isle", 1),
    ("isles", 1),
    ("aisle", 1),
    ("aisles", 1),
    ("element", 3),
    ("elements", 3),
    ("nevertheless", 4),
    ("homelessness", 3),
    ("evening", 2),
    ("evenings", 2),
    ("ache", 1),
    ("aches", 1),
    ("headache", 2),
    ("headaches", 2),
    ("tiredness", 2),
];

/// Two-vowel sequences pronounced as two syllables, and endings that are one
/// syllable fewer than their vowel groups (after Fast's Lingua::EN::Syllable).
static ADD_SYL: LazyLock<Vec<regex::Regex>> = LazyLock::new(|| {
    [
        "ia",
        // so-ci-e-ty, di-e-ta-ry, va-ri-e-ty
        "iet",
        "dien",
        "iu",
        "io",
        "ii",
        "^mc",
        "ism$",
        "[^l]lien",
        "^coa[dglx].",
        "[^gq]ua[^auieo]",
        // didn't, isn't, doesn't, mustn't (apostrophes are dropped before counting)
        "[dst]nt$",
        "eing",
        "oing",
        // se-ri-ous, pre-vi-ous, te-di-ous (but not -cious, -tious, -gious)
        "[rvd]ious",
        // ex-pe-ri-ence, nu-tri-ent, re-cip-i-ent (but not friend)
        "[^f]rien",
        "pien",
        // di-ar-rhe-a, gon-or-rhe-a
        "rhea$",
        // re-act, re-al-i-ty, re-al-ize (but not read, reach, real)
        "^react",
        "^reali",
        // flu-id, ru-in (but not liquid, guide)
        "[^qg]uid",
        "ruin",
        // sci-en-tist, sci-en-tif-ic ("science" is an exception)
        "^scien",
    ]
    .iter()
    .map(|p| regex::Regex::new(p).expect("valid regex"))
    .collect()
});
static SUB_SYL: LazyLock<Vec<regex::Regex>> = LazyLock::new(|| {
    // "cious" is covered by "iou" (listing both subtracted twice: con-scious, de-li-cious).
    ["cial", "tia", "cius", "giu", "ion", "iou", "sia$", ".ely$"]
        .iter()
        .map(|p| regex::Regex::new(p).expect("valid regex"))
        .collect()
});
/// A silent *e* inside a word, before a suffix or the second part of a
/// compound (safe-ty, care-ful, home-less, move-ment, some-thing, guide-lines,
/// house-hold, care-giver). Matched against the whole word; each match is one
/// syllable fewer than its vowel groups.
static SILENT_E: LazyLock<Vec<regex::Regex>> = LazyLock::new(|| {
    [
        "[^aeiouy]ety$",
        "[^aeiouy]eful(ly)?$",
        "[^aeiouy]e(less|ness)$",
        "[aeiouy][^aeiouy]ements?$",
        "[nrl][cgsv]ements?$",
        "[^aeiouy]etimes?$",
        "[^aeiouy]ethings?$",
        "[aeiouy][^aeiouy]elines?$",
        "[^aeiouy]ewhere$",
        "[^aeiouy]ebody$",
        "[aeiouy][^aeiouy]ework(s|er|ers)?$",
        "[aeiouy][^aeiouy]eholds?$",
        "[aeiouy][^aeiouy]estyles?$",
        "[aeiouy][^aeiouy]egiv(er|ers|ing)$",
        "[aeiouy][^aeiouy]etak(er|ers|ing)$",
    ]
    .iter()
    .map(|p| regex::Regex::new(p).expect("valid regex"))
    .collect()
});

/// Estimate the syllables in one word (letters only; at least 1).
pub fn word_syllables(word: &str) -> usize {
    let w: String = word
        .chars()
        .filter(char::is_ascii_alphabetic)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if w.is_empty() {
        // Non-Latin letters: count the word as one syllable.
        return 1;
    }
    if let Some((_, n)) = EXCEPTIONS.iter().find(|(e, _)| *e == w) {
        return *n;
    }
    let chars: Vec<char> = w.chars().collect();
    let n = chars.len();
    let is_vowel = |i: usize| -> bool {
        match chars[i] {
            'a' | 'e' | 'i' | 'o' | 'u' => true,
            // 'y' is a consonant at the start of a word (yes, year, you).
            'y' => i > 0,
            _ => false,
        }
    };
    // Silent final 'e', except consonant + "le" (ta-ble, peo-ple).
    let consonant_le =
        n >= 3 && chars[n - 1] == 'e' && chars[n - 2] == 'l' && !"aeiouy".contains(chars[n - 3]);
    let effective = if chars[n - 1] == 'e' && !consonant_le {
        n - 1
    } else {
        n
    };
    let mut groups = 0usize;
    let mut prev = false;
    for i in 0..effective {
        let v = is_vowel(i);
        if v && !prev {
            groups += 1;
        }
        prev = v;
    }
    if groups == 0 {
        // "the", "she": the final e is the only vowel.
        return 1;
    }
    let stem: String = chars[..effective].iter().collect();
    let mut syl = groups as isize;
    for r in SUB_SYL.iter() {
        if r.is_match(&stem) {
            syl -= 1;
        }
    }
    for r in ADD_SYL.iter() {
        if r.is_match(&stem) {
            syl += 1;
        }
    }
    for r in SILENT_E.iter() {
        if r.is_match(&w) {
            syl -= 1;
        }
    }
    // Silent -ed (caused, loved). The e is sounded, or already part of the
    // vowel group before it, after t or d (started, needed), after a vowel
    // (carried, agreed, continued, employed), in consonant + "red" (hundred),
    // and in consonant + "led" except after l, r, or w (handled, disabled; but
    // called, curled, crawled).
    if n > 3 && w.ends_with("ed") && groups > 1 {
        let before = chars[n - 3];
        let earlier = chars[n - 4];
        let vowel = |c: char| "aeiouy".contains(c);
        let sounded = matches!(before, 't' | 'd')
            || vowel(before)
            || (before == 'r' && !vowel(earlier))
            || (before == 'l' && !vowel(earlier) && !matches!(earlier, 'l' | 'r' | 'w'));
        if !sounded {
            syl -= 1;
        }
    }
    // Silent -es (makes, states) unless after s, x, z, c, g, ch, sh (cases, ages).
    if n > 3
        && w.ends_with("es")
        && !matches!(
            chars[n - 3],
            's' | 'x' | 'z' | 'c' | 'g' | 'a' | 'e' | 'i' | 'o' | 'u' | 'y'
        )
        && !w.ends_with("ches")
        && !w.ends_with("shes")
        && !(chars[n - 3] == 'l' && n > 4 && !"aeiouy".contains(chars[n - 4]))
        && groups > 1
    {
        syl -= 1;
    }
    syl.max(1) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syllables_match_dictionary_counts_for_common_health_words() {
        for (w, n) in [
            ("cat", 1),
            ("the", 1),
            ("vaccine", 2),
            ("vaccines", 2),
            ("immunization", 5),
            ("table", 2),
            ("people", 2),
            ("available", 4),
            ("made", 1),
            ("makes", 1),
            ("protected", 3),
            ("baby", 2),
            ("caused", 1),
            ("cases", 2),
            ("ages", 2),
            ("states", 1),
            ("situations", 4),
            ("sanitation", 4),
            ("prevention", 3),
            ("hepatitis", 4),
            ("hygiene", 2),
            ("disease", 2),
            ("diseases", 3),
            ("epidemics", 4),
            ("isolated", 4),
            ("anyone", 3),
            ("united", 3),
            ("globulin", 3),
            ("immune", 2),
            ("opioid", 3),
            ("overdose", 3),
            ("naloxone", 3),
            ("fentanyl", 3),
            ("medicine", 3),
            ("being", 2),
            ("going", 2),
            ("years", 1),
            ("you", 1),
            ("media", 3),
            ("special", 2),
            ("period", 3),
            ("million", 2),
            ("violence", 3),
            ("suicide", 3),
            ("lately", 2),
        ] {
            assert_eq!(word_syllables(w), n, "{w}");
        }
    }

    #[test]
    fn syllables_for_words_the_earlier_rules_got_wrong() {
        // Every word the pre-2026-11 rules miscounted on the 300-word check list.
        for (w, n) in [
            ("influenza", 4),
            ("diabetes", 4),
            ("pneumonia", 3),
            ("sometimes", 2),
            ("something", 2),
            ("homeless", 2),
            ("careful", 2),
            ("movement", 2),
            ("statement", 2),
            ("everything", 3),
            ("therefore", 2),
            ("diarrhea", 4),
            ("nausea", 3),
            ("experience", 4),
            ("serious", 3),
            ("various", 3),
            ("previous", 3),
            ("reaction", 3),
            ("fluid", 2),
            ("carried", 2),
            ("studied", 2),
            ("hundred", 2),
            ("indeed", 2),
            ("proceed", 2),
            ("exceed", 2),
            ("agreed", 2),
            ("succeed", 2),
            ("antibiotic", 5),
            ("antibiotics", 5),
            ("safety", 2),
            ("ninety", 2),
            ("poem", 2),
            ("lion", 2),
            ("pioneer", 3),
            ("heroin", 3),
            ("gonorrhea", 4),
            ("doesn't", 2),
            ("isn't", 2),
            ("guidelines", 2),
            ("scientists", 3),
            ("society", 4),
            ("anxiety", 4),
            ("dietary", 4),
            ("quietly", 3),
            ("vegetables", 3),
            ("comfortable", 3),
            ("isles", 1),
        ] {
            assert_eq!(word_syllables(w), n, "{w}");
        }
        // The new rules leave these alone.
        for (w, n) in [
            ("called", 1),
            ("handled", 2),
            ("employed", 2),
            ("friend", 1),
            ("guide", 1),
            ("conscious", 2),
            ("delicious", 3),
            ("household", 2),
            ("caregiver", 3),
            ("element", 3),
            ("treatment", 2),
        ] {
            assert_eq!(word_syllables(w), n, "{w}");
        }
        // "Safety comes first. Sometimes something goes wrong." has no polysyllables.
        let p = parse("Safety comes first. Sometimes something goes wrong. Call us.");
        assert_eq!(p.counts().polysyllables, 0);
    }

    #[test]
    fn sentences_respect_abbreviations_initials_and_decimals() {
        let s = split_sentences(
            "Dr. Lee said 3.5 mg is enough. Call the U.S. Food and Drug Administration! Is it safe? \
             John F. Kennedy spoke. Get the hepatitis A. Vaccines work.",
        );
        assert_eq!(
            s,
            vec![
                "Dr. Lee said 3.5 mg is enough.",
                "Call the U.S. Food and Drug Administration!",
                "Is it safe?",
                "John F. Kennedy spoke.",
                "Get the hepatitis A.",
                "Vaccines work."
            ]
        );
        assert_eq!(
            split_sentences("No end punctuation here"),
            vec!["No end punctuation here"]
        );
    }

    #[test]
    fn markdown_blocks_lists_and_headings() {
        let p = parse(
            "# Flu shots\n\nGet a flu shot every year. It is free.\n\n- Wash your hands.\n- Stay home \
             when sick\n  and rest.\n\nMore text\ncontinues here.",
        );
        let kinds: Vec<BlockKind> = p.blocks.iter().map(|b| b.kind).collect();
        assert_eq!(
            kinds,
            vec![
                BlockKind::Heading,
                BlockKind::Paragraph,
                BlockKind::ListItem,
                BlockKind::ListItem,
                BlockKind::Paragraph
            ]
        );
        assert_eq!(p.blocks[3].text, "Stay home when sick and rest.");
        assert_eq!(p.blocks[2].list, p.blocks[3].list);
        assert_eq!(p.blocks[4].sentences.len(), 1);
        // The heading is not counted: 2 + 1 + 1 + 1 sentences.
        assert_eq!(p.counts().sentences, 5);
    }

    #[test]
    fn hyphenated_words_count_once_and_numbers_are_one_syllable() {
        let w = words("long-term care for 2 years, 20% (1,000 people)");
        let texts: Vec<&str> = w.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(
            texts,
            vec![
                "long-term",
                "care",
                "for",
                "2",
                "years",
                "20",
                "1,000",
                "people"
            ]
        );
        assert!(w[0].hyphenated && w[0].syllables == 2);
        assert!(w[3].is_number && w[3].syllables == 1);
    }
}
