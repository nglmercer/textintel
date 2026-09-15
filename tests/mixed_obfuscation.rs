//! Mixed obfuscation: leetspeak + symbols + case in one message.
//!
//! The flagship chain (`Fr4🏠d0 → fracasado`) is asserted step by step; hard
//! negatives guard the neighboring lookalikes.

use textintel::{EngineConfig, TextIntelligence};

fn engine() -> TextIntelligence {
    TextIntelligence::new(EngineConfig::default())
}

#[test]
fn flagship_chain_decodes_with_exact_steps() {
    let languages = vec!["es".to_string()];
    let candidates = engine()
        .decode_with_languages("Fr4🏠d0", Some(&languages), Some(5))
        .unwrap();
    assert_eq!(candidates[0].text, "Fracasado");
    let steps: Vec<(&str, &str)> = candidates[0]
        .transformations
        .iter()
        .map(|step| (step.source.as_str(), step.replacement.as_str()))
        .collect();
    assert_eq!(steps, vec![("4", "a"), ("🏠", "casa"), ("0", "o")]);
    // Every step points at its original UTF-8 span.
    for step in &candidates[0].transformations {
        let (start, end, span) = (
            step.start.expect("span start"),
            step.end.expect("span end"),
            step.span.as_deref().expect("span text"),
        );
        assert_eq!(&"Fr4🏠d0"[start..end], span);
        assert_eq!(&"Fr4🏠d0"[start..end], step.source);
    }
}

#[test]
fn spaced_leet_sentence_keeps_its_space() {
    let languages = vec!["es".to_string()];
    let candidates = engine()
        .decode_with_languages("c0mpr4 ah0r4", Some(&languages), Some(5))
        .unwrap();
    assert_eq!(candidates[0].text, "compra ahora");
}

#[test]
fn mixed_obfuscation_comparison_orders_correctly() {
    let engine = engine();
    let pairs = [
        ("Fr4🏠d0", "fracasado", "ferrocarril"),
        ("c0mpr4 ah0r4", "compra ahora", "compara hora"),
        ("gr8", "great", "grate"),
    ];
    for (obfuscated, plain, decoy) in pairs {
        let good = engine.compare(obfuscated, plain).unwrap().score;
        let bad = engine.compare(obfuscated, decoy).unwrap().score;
        assert!(
            good > bad,
            "{obfuscated}: {plain} ({good:.3}) must outscore {decoy} ({bad:.3})"
        );
    }
}

#[test]
fn mixed_hard_negatives_stay_below_threshold() {
    let engine = engine();
    for (left, right) in [
        ("Fra🏠do", "ferrocarril"),
        ("compra ahora", "compra hora"),
        ("4 you", "4 me"),
    ] {
        let comparison = engine.compare(left, right).unwrap();
        assert!(
            comparison.score < 0.85,
            "{left} vs {right} must not look like a duplicate: {}",
            comparison.score
        );
    }
    // ...while true plain matches outrank the decoys. Absolute totals for
    // short decoded-only pairs sit below 0.5 (documented threshold gap),
    // so the pinned property is the ranking, not the cutoff.
    let flagship = engine.compare("Fr4🏠d0", "fracasado").unwrap();
    let decoy = engine.compare("Fr4🏠d0", "ferrocarril").unwrap();
    assert!(
        flagship.score > decoy.score,
        "fracasado ({}) must outrank ferrocarril ({})",
        flagship.score,
        decoy.score
    );
    // With an `es` hint the reading is available at analyze time, so the
    // decoded channel matches exactly.
    let hinted = EngineConfig {
        language_hints: vec!["es".to_string()],
        ..Default::default()
    };
    let hinted = TextIntelligence::new(hinted);
    let flagship = hinted.compare("Fr4🏠d0", "fracasado").unwrap();
    assert_eq!(flagship.decoded_similarity, Some(1.0));
}
