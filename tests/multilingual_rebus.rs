//! Multilingual rebus: decoding across languages, mixed-language validity,
//! and hard negatives. No complete example strings are hardcoded: every
//! expansion comes from resource packs through the public decode path.

use textintel::{EngineConfig, TextIntelligence};

fn engine() -> TextIntelligence {
    TextIntelligence::new(EngineConfig::default())
}

fn top1(text: &str, languages: &[&str]) -> String {
    let languages: Vec<String> = languages.iter().map(|code| code.to_string()).collect();
    let candidates = engine()
        .decode_with_languages(text, Some(&languages), Some(5))
        .unwrap();
    assert!(!candidates.is_empty(), "no candidates for {text:?}");
    candidates[0].text.clone()
}

#[test]
fn slang_decodes_across_languages() {
    assert_eq!(top1("gr8", &["en"]), "great");
    assert_eq!(top1("b4", &["en"]), "before");
    assert_eq!(top1("m8", &["en"]), "mate");
    assert_eq!(top1("2morrow", &["en"]), "tomorrow");
    assert_eq!(top1("slt", &["fr"]), "salut");
    assert_eq!(top1("bj", &["pt"]), "beijo");
}

#[test]
fn symbols_decode_across_languages() {
    assert_eq!(top1("☕ break", &["en"]), "coffee break");
    assert_eq!(top1("Fra🏠do", &["es"]), "Fracasado");
    assert_eq!(top1("I ❤ you", &["en"]), "I love you");
    assert_eq!(top1("我❤你", &["zh"]), "我爱你");
}

#[test]
fn mixed_language_inputs_stay_valid() {
    let languages = vec!["en".to_string(), "es".to_string()];
    let candidates = engine()
        .decode_with_languages("I ❤️ casa", Some(&languages), Some(5))
        .unwrap();
    assert!(!candidates.is_empty(), "mixed input must decode");
    assert!(
        candidates
            .iter()
            .take(3)
            .any(|candidate| candidate.text == "I love casa"),
        "expected 'I love casa' near the top: {:?}",
        candidates
            .iter()
            .map(|candidate| &candidate.text)
            .collect::<Vec<_>>()
    );
    // Deeply mixed obfuscation also decodes instead of abstaining.
    let candidates = engine()
        .decode_with_languages("I ❤️ c4s4", Some(&languages), Some(5))
        .unwrap();
    assert!(!candidates.is_empty());
}

#[test]
fn mixed_language_comparison_beats_unrelated() {
    let engine = engine();
    let related = engine.compare("I ❤️ c4s4", "I love casa").unwrap();
    let unrelated = engine
        .compare("I ❤️ c4s4", "quantum physics lecture")
        .unwrap();
    assert!(
        related.score > unrelated.score,
        "mixed pair should outscore unrelated: {} vs {}",
        related.score,
        unrelated.score
    );
}

#[test]
fn rebus_hard_negatives() {
    let engine = engine();
    // Lookalikes score below the duplicate threshold.
    for (left, right) in [
        ("gr8", "grate"),
        ("m8", "made"),
        ("b4", "after"),
        ("Fra🏠do", "ferrocarril"),
    ] {
        let comparison = engine.compare(left, right).unwrap();
        assert!(
            comparison.score < 0.5,
            "{left} vs {right} must stay negative, got {}",
            comparison.score
        );
    }
    // Orthographic errors are out of scope for the rebus: `kasa` must not
    // silently become `casa` (no fuzzy spelling correction).
    assert_eq!(top1("kasa", &["es"]), "kasa");
}

#[test]
fn language_scoring_rewards_partial_coverage() {
    use textintel::rebus::scorer::RebusEvidence;
    let requested = ["en".to_string(), "es".to_string()];
    let full = RebusEvidence {
        candidate_language: None,
        candidate_languages: vec!["en".to_string(), "es".to_string()],
        ..Default::default()
    };
    assert_eq!(full.language_score(Some(&requested)), 1.0);
    let partial = RebusEvidence {
        candidate_languages: vec!["es".to_string(), "fr".to_string()],
        ..Default::default()
    };
    assert_eq!(partial.language_score(Some(&requested)), 0.8);
    let disjoint = RebusEvidence {
        candidate_languages: vec!["fr".to_string()],
        ..Default::default()
    };
    assert_eq!(disjoint.language_score(Some(&requested)), 0.4);
    // Legacy single-language evidence keeps working.
    let single = RebusEvidence {
        candidate_language: Some("en".to_string()),
        ..Default::default()
    };
    assert_eq!(single.language_score(Some(&requested)), 1.0);
    assert_eq!(
        RebusEvidence::default().language_score(Some(&requested)),
        0.7
    );
}
