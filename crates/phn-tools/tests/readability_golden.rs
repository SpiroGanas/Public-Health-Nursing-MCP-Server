//! Golden tests for the readability engine (COMM-01 `check_readability`)
//! against published worked examples: the SMOG conversion table and the Fry
//! short-document example in CDC's *Simply Put*, the passive-voice examples in
//! *Simply Put* and ODPHP's *Health Literacy Online*, CDC's plain-language
//! swaps, and the CDC Clear Communication Index score-sheet arithmetic.
//! Sources are cited in `fixtures/readability_golden.json`.

use std::collections::BTreeMap;

use phn_tools::comms::readability::{
    AnalyzeOptions, analyze, flesch_kincaid_grade, fry_averages, score_cci, smog_table_grade,
};
use phn_tools::comms::textstats::{self, word_syllables};
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/readability_golden.json")).unwrap()
}

#[test]
fn smog_conversion_table_matches_simply_put() {
    let f = fixture();
    for case in f["smog_table"]["cases"].as_array().unwrap() {
        let count = case[0].as_u64().unwrap() as usize;
        let grade = case[1].as_u64().unwrap() as u32;
        assert_eq!(
            smog_table_grade(count),
            Some(grade),
            "{count} polysyllables"
        );
        // McLaughlin's rule: 3 + the square root of the nearest perfect square.
        let root = (count as f64).sqrt().round() as u32;
        let nearest = (root.saturating_sub(1)..=root + 1)
            .min_by_key(|r| (i64::from(r * r) - count as i64).abs())
            .unwrap();
        assert_eq!(3 + nearest.max(1), grade, "{count}");
    }
    assert_eq!(smog_table_grade(111), None, "beyond the published table");
}

#[test]
fn fry_short_document_example_matches_published_counts() {
    let f = fixture();
    let ex = &f["fry_short_document"];
    let text = ex["text"].as_str().unwrap();
    let a = analyze(text, &AnalyzeOptions::default());
    assert_eq!(
        a.counts.words as u64,
        ex["published_words"].as_u64().unwrap()
    );
    assert_eq!(
        a.counts.sentences as u64,
        ex["published_sentences"].as_u64().unwrap()
    );
    // The published per-100-word averages follow from the published counts.
    let fry = fry_averages(82, 6, 177);
    assert_eq!(
        fry.sentences_per_100_words,
        ex["published_sentences_per_100_words"].as_f64().unwrap()
    );
    assert_eq!(
        fry.syllables_per_100_words.round(),
        ex["published_syllables_per_100_words"].as_f64().unwrap()
    );
    // Syllables: compare with dictionary counts word by word.
    let dict = ex["dictionary_words"].as_object().unwrap();
    let mut agree = 0;
    let mut total = 0;
    let mut ours = 0;
    for s in textstats::parse(text).counted_sentences() {
        for w in &s.words {
            let key = w.text.to_lowercase();
            let expected = dict[&key].as_u64().unwrap() as usize;
            total += 1;
            ours += w.syllables;
            if w.syllables == expected {
                agree += 1;
            }
        }
    }
    let dictionary_total = ex["dictionary_syllables"].as_u64().unwrap() as usize;
    assert_eq!(total, 82);
    assert!(agree * 100 >= total * 95, "{agree} of {total} words agree");
    assert!(
        ours.abs_diff(dictionary_total) <= 3,
        "{ours} vs {dictionary_total}"
    );
    assert_eq!(a.counts.syllables, ours);
    // Formula check at the published counts: FK = 0.39 x 82/6 + 11.8 x 177/82 - 15.59 = 15.21.
    let c = textstats::Counts {
        words: 82,
        sentences: 6,
        syllables: 177,
        ..Default::default()
    };
    assert!((flesch_kincaid_grade(&c) - 15.2107).abs() < 1e-3);
}

#[test]
fn passive_voice_examples_are_classified() {
    let f = fixture();
    for s in f["passive_voice"]["passive"].as_array().unwrap() {
        let a = analyze(s.as_str().unwrap(), &AnalyzeOptions::default());
        assert_eq!(a.flags.passive_voice_count, 1, "{s}");
    }
    for s in f["passive_voice"]["active"].as_array().unwrap() {
        let a = analyze(s.as_str().unwrap(), &AnalyzeOptions::default());
        assert_eq!(a.flags.passive_voice_count, 0, "{s}");
    }
}

#[test]
fn plain_language_swaps_come_from_cdc() {
    let f = fixture();
    for case in f["plain_language_swaps"]["cases"].as_array().unwrap() {
        let word = case[0].as_str().unwrap();
        let plain = case[1].as_str().unwrap();
        let a = analyze(
            &format!("Read about {word} today."),
            &AnalyzeOptions::default(),
        );
        let j = a
            .flags
            .jargon
            .first()
            .unwrap_or_else(|| panic!("{word} not flagged"));
        assert!(
            j.plain_alternative
                .as_deref()
                .unwrap_or_default()
                .contains(plain),
            "{word}: {:?}",
            j.plain_alternative
        );
    }
}

#[test]
fn clear_communication_index_scores_follow_the_score_sheet() {
    let f = fixture();
    for case in f["cci_scoring"]["cases"].as_array().unwrap() {
        let answers: BTreeMap<String, String> =
            serde_json::from_value(case["answers"].clone()).unwrap();
        let r = score_cci(&answers, None).unwrap();
        let name = case["name"].as_str().unwrap();
        assert_eq!(
            r.points_earned,
            Some(case["points_earned"].as_u64().unwrap() as u32),
            "{name}: {r:#?}"
        );
        assert_eq!(
            r.points_possible,
            Some(case["points_possible"].as_u64().unwrap() as u32),
            "{name}"
        );
        assert_eq!(r.score, Some(case["score"].as_f64().unwrap()), "{name}");
        assert_eq!(
            r.passing,
            Some(case["passing"].as_bool().unwrap()),
            "{name}"
        );
    }
}

#[test]
fn syllable_heuristic_handles_the_dictionary_list() {
    // A small sanity check that the public syllable function is the one used.
    assert_eq!(word_syllables("hepatitis"), 4);
    assert_eq!(word_syllables("sanitation"), 4);
}
