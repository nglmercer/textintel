//! Transliteration: script conversion as additive fingerprint views.
//!
//! Covers `privet ↔ привет`, `ni hao ↔ 你好`, `salam ↔ سلام`; `raw` is never
//! replaced, and hard negatives pin the documented same-script threshold gap
//! (decoded channel separates even when the 0.5 total does not).

use textintel::core::providers::TransliterationProvider;
use textintel::{RuleBasedTransliterationProvider, TextIntelligence};

fn views(text: &str) -> Vec<(String, String)> {
    RuleBasedTransliterationProvider
        .transliterate(text)
        .into_iter()
        .map(|view| (view.target_script, view.text))
        .collect()
}

#[test]
fn russian_links_privet() {
    let latin = views("привет");
    assert!(
        latin
            .iter()
            .any(|(script, text)| script == "Latn" && text == "privet"),
        "missing привет → privet: {latin:?}"
    );
    let cyrillic = views("privet");
    assert!(
        cyrillic
            .iter()
            .any(|(script, text)| script == "Cyrl" && text == "привет"),
        "missing privet → привет: {cyrillic:?}"
    );
}

#[test]
fn chinese_links_ni_hao() {
    let latin = views("你好");
    assert!(
        latin
            .iter()
            .any(|(script, text)| script == "Latn" && text == "ni hao"),
        "missing 你好 → ni hao: {latin:?}"
    );
    let hans = views("ni hao");
    assert!(
        hans.iter()
            .any(|(script, text)| script == "Hans" && text == "你好"),
        "missing ni hao → 你好: {hans:?}"
    );
}

#[test]
fn arabic_links_salam() {
    // Short vowels are unwritten: the forward fold is `slam`, while the
    // reverse direction links `salam` back to `سلام` exactly.
    let latin = views("سلام");
    assert!(
        latin.iter().any(|(script, _)| script == "Latn"),
        "missing سلام → Latn view: {latin:?}"
    );
    let arab = views("salam");
    assert!(
        arab.iter()
            .any(|(script, text)| script == "Arab" && text == "سلام"),
        "missing salam → سلام: {arab:?}"
    );
}

#[test]
fn fingerprint_preserves_views_and_raw() {
    let engine = TextIntelligence::default();
    let fingerprint = engine.analyze("привет").unwrap();
    assert_eq!(fingerprint.raw, "привет");
    assert_eq!(
        fingerprint
            .normalization_views
            .get("transliteration:latn")
            .map(String::as_str),
        Some("privet")
    );
    // Pure-emoji input yields no transliteration views rather than junk.
    let emoji = engine.analyze("🏠🔥").unwrap();
    assert_eq!(emoji.raw, "🏠🔥");
    assert!(
        emoji
            .normalization_views
            .keys()
            .all(|name| !name.starts_with("transliteration:")),
        "unexpected views: {:?}",
        emoji.normalization_views
    );
}

#[test]
fn decoded_channel_matches_cross_script_pairs() {
    let engine = TextIntelligence::default();
    for (left, right) in [("privet", "привет"), ("ni hao", "你好"), ("salam", "سلام")] {
        let comparison = engine.compare(left, right).unwrap();
        assert_eq!(
            comparison.decoded_similarity,
            Some(1.0),
            "{left} ↔ {right} must match through transliteration views"
        );
        assert!(
            comparison
                .explanations
                .iter()
                .any(|line| line.contains("transliteration=")),
            "missing transliteration evidence: {:?}",
            comparison.explanations
        );
    }
}

#[test]
fn hard_negatives_pin_the_threshold_gap() {
    let engine = TextIntelligence::default();
    // Same-script lookalikes outscore cross-script matches on the 0.5 total
    // (documented gap: script-bound channels dominate the default weights).
    // The decoded channel still separates correctly.
    let lookalike = engine.compare("salam", "salami").unwrap();
    let cross_script = engine.compare("salam", "سلام").unwrap();
    assert!(
        lookalike.decoded_similarity.unwrap_or(1.0) < 1.0,
        "salami must not match exactly"
    );
    assert_eq!(cross_script.decoded_similarity, Some(1.0));
    assert!(
        lookalike.score > cross_script.score,
        "documents the known inversion: lookalike={} cross-script={}",
        lookalike.score,
        cross_script.score
    );
    // Unrelated cross-script pairs stay at the floor (no same-side leak).
    let unrelated = engine.compare("你好", "再见").unwrap();
    assert!(unrelated.decoded_similarity.unwrap_or(1.0) < 0.5);
    assert!(unrelated.score < 0.5);
}
