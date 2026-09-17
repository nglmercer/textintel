//! Configurable rebus weights: scoring blends, penalties, and JSON loading.

use textintel::rebus::scorer::{
    RebusEvidence, score_candidate_with_evidence, score_candidate_with_evidence_and_weights,
};
use textintel::{EngineConfig, RebusWeights, TextIntelligence};

fn default_engine_and_support() -> (
    TextIntelligence,
    textintel::resources::DefaultLexiconProvider,
    textintel::phonetic::RuleBasedG2PProvider,
) {
    (
        TextIntelligence::new(EngineConfig::default()),
        textintel::resources::DefaultLexiconProvider,
        textintel::phonetic::RuleBasedG2PProvider,
    )
}

#[test]
fn default_weights_match_legacy_scores() {
    let (_engine, lexicon, g2p) = default_engine_and_support();
    let evidence = RebusEvidence::default();
    let legacy = score_candidate_with_evidence(
        "fracasado",
        "Fr4🏠d0",
        0.5,
        4,
        None,
        &lexicon,
        &g2p,
        &evidence,
    );
    let weighted = score_candidate_with_evidence_and_weights(
        "fracasado",
        "Fr4🏠d0",
        0.5,
        4,
        None,
        &lexicon,
        &g2p,
        &evidence,
        &RebusWeights::default(),
    );
    assert_eq!(legacy, weighted);
}

#[test]
fn weights_change_scores_and_penalties() {
    let (_engine, lexicon, g2p) = default_engine_and_support();
    let evidence = RebusEvidence {
        transformation_types: vec!["leetspeak".to_string(), "symbol_reading".to_string()],
        ..Default::default()
    };
    let default_total = score_candidate_with_evidence_and_weights(
        "fracasado",
        "Fr4🏠d0",
        0.5,
        4,
        None,
        &lexicon,
        &g2p,
        &evidence,
        &RebusWeights::default(),
    )
    .0;
    // Zeroing the transformation costs must raise the total (penalty gone).
    let forgiving = RebusWeights {
        cost_leet: 0.0,
        cost_symbol: 0.0,
        cost_other: 0.0,
        ..Default::default()
    };
    let forgiving_total = score_candidate_with_evidence_and_weights(
        "fracasado",
        "Fr4🏠d0",
        0.5,
        4,
        None,
        &lexicon,
        &g2p,
        &evidence,
        &forgiving,
    )
    .0;
    assert!(
        forgiving_total > default_total,
        "{forgiving_total} should exceed {default_total}"
    );
    // Emphasizing one channel over the others moves the total.
    let lexical_heavy = RebusWeights {
        lexical: 10.0,
        phonetic: 0.01,
        ..Default::default()
    };
    let lexical_total = score_candidate_with_evidence_and_weights(
        "fracasado",
        "Fr4🏠d0",
        0.5,
        4,
        None,
        &lexicon,
        &g2p,
        &evidence,
        &lexical_heavy,
    )
    .0;
    assert!((lexical_total - default_total).abs() > 1e-9);
    // The semantic blend is honored: full blend returns the evidence value.
    let semantic_only = RebusWeights {
        semantic: 1.0,
        ..Default::default()
    };
    let evidence = RebusEvidence {
        semantic_similarity: Some(0.25),
        ..Default::default()
    };
    let total = score_candidate_with_evidence_and_weights(
        "fracasado",
        "Fr4🏠d0",
        0.5,
        4,
        None,
        &lexicon,
        &g2p,
        &evidence,
        &semantic_only,
    )
    .0;
    assert!((total - 0.25).abs() < 1e-9, "got {total}");
}

#[test]
fn weights_load_from_json_and_validate() {
    let json = serde_json::json!({
        "lexical": 1.0,
        "phonetic": 1.0,
        "symbol": 1.0,
        "language": 1.0,
        "context": 1.0,
        "semantic": 0.2,
    });
    let weights = RebusWeights::from_json(&json.to_string()).unwrap();
    assert_eq!(weights.lexical, 1.0);
    assert_eq!(weights.semantic, 0.2);
    weights.validate().unwrap();
    // Round-trip through the serializer.
    let roundtrip = RebusWeights::from_json(&weights.to_json().unwrap()).unwrap();
    assert_eq!(roundtrip, weights);
    // Invalid vectors fail loudly (trained weights must pass validation).
    for bad in [
        serde_json::json!({"lexical": -1.0}),
        serde_json::json!({"transformation_penalty_cap": 1.5}),
        serde_json::json!({"semantic": 2.0}),
        serde_json::json!({"in_word_digit_discount": 0.0}),
        serde_json::json!({
            "lexical": 0.0, "phonetic": 0.0, "symbol": 0.0,
            "language": 0.0, "context": 0.0
        }),
    ] {
        assert!(
            RebusWeights::from_json(&bad.to_string()).is_err(),
            "should reject {bad}"
        );
    }
    assert!(RebusWeights::from_json("not json").is_err());
}

#[test]
fn in_word_digit_discount_controls_leet_vs_number_names() {
    let languages = vec!["es".to_string()];
    // Default: the leet path survives and `Fracasado` wins.
    let engine = TextIntelligence::new(EngineConfig::default());
    let top = &engine
        .decode_with_languages("Fr4🏠d0", Some(&languages), Some(5))
        .unwrap()[0];
    assert_eq!(top.text, "Fracasado");
    // Disabled (1.0): number names flood the beam and the leet path is lost.
    let mut config = EngineConfig::default();
    config.rebus_weights.in_word_digit_discount = 1.0;
    let engine = TextIntelligence::new(config);
    let candidates = engine
        .decode_with_languages("Fr4🏠d0", Some(&languages), Some(5))
        .unwrap();
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.text != "Fracasado"),
        "without the discount the leet path should be crowded out: {:?}",
        candidates
            .iter()
            .map(|candidate| &candidate.text)
            .collect::<Vec<_>>()
    );
}
