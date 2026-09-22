//! Decision API: request validation, provider dispatch, adapters, and
//! response invariants through the real engine.

use std::collections::BTreeMap;
use std::sync::Arc;

use textintel::decision::{
    DecisionAnswer, DecisionProvider, DecisionQuestion, DecisionRequest,
    SimilarityDecisionProvider, SpamDecisionProvider,
};
use textintel::{HeuristicSpamPredictor, ProfileSimilarityScorer, TextIntelligence};

fn routing_question() -> DecisionQuestion {
    DecisionQuestion::Choice {
        instructions: "Which team should handle this message?".to_string(),
        criteria: BTreeMap::from([
            (
                "billing".to_string(),
                "Payment, invoice and refund problems".to_string(),
            ),
            (
                "technical".to_string(),
                "Problems operating the software".to_string(),
            ),
            (
                "sales".to_string(),
                "Questions about purchasing".to_string(),
            ),
        ]),
    }
}

fn similarity_engine() -> TextIntelligence {
    TextIntelligence::default().with_decision_provider(SimilarityDecisionProvider::new(Arc::new(
        ProfileSimilarityScorer::default(),
    )))
}

fn spam_engine() -> TextIntelligence {
    TextIntelligence::default()
        .with_decision_provider(SpamDecisionProvider::new(Arc::new(HeuristicSpamPredictor)))
}

#[test]
fn choice_decision_is_valid_and_deterministic() {
    let engine = similarity_engine();
    let request = DecisionRequest::new("I was charged twice, refund me", routing_question());
    let first = engine.decide(&request).expect("decide");
    first.validate_against(&request).expect("valid answer");
    let second = engine.decide(&request).expect("decide again");
    assert_eq!(first, second);
    assert_eq!(first.provider, "similarity_decision_adapter");
    let DecisionAnswer::Choice {
        choice,
        probabilities,
        ..
    } = &first.answer
    else {
        panic!("expected a choice answer");
    };
    assert!(["billing", "technical", "sales"].contains(&choice.as_str()));
    let sum: f64 = probabilities.values().sum();
    assert!((sum - 1.0).abs() < 1e-6, "probabilities sum to {sum}");
}

#[test]
fn binary_spam_decision_reports_probabilities() {
    let engine = spam_engine();
    let question = || DecisionQuestion::Binary {
        statement: "This message is spam.".to_string(),
    };
    let spam = engine
        .decide(&DecisionRequest::new(
            "WIN PRIZE!!! click http://spam.example now now now",
            question(),
        ))
        .expect("decide");
    spam.validate_against(&DecisionRequest::new("x", question()))
        .expect("valid answer");
    let DecisionAnswer::Binary {
        probability_true,
        probability_false,
        confidence,
    } = spam.answer
    else {
        panic!("expected a binary answer");
    };
    assert!((probability_true + probability_false - 1.0).abs() < 1e-9);
    assert!((confidence - probability_true.max(probability_false)).abs() < 1e-9);
    // The heuristic is weak in absolute terms; the robust property is the
    // ordering: spammy text outscores benign text.
    let ham = engine
        .decide(&DecisionRequest::new("hello, see you tomorrow", question()))
        .expect("decide");
    let ham_spam = ham.answer.probabilities_in_order()[1];
    assert!(
        probability_true > ham_spam && ham.answer.predicted_label() == "false",
        "spam ({probability_true:.3}) should outscore ham ({ham_spam:.3})"
    );
}

#[test]
fn spam_adapter_answers_spam_ham_choices() {
    let engine = spam_engine();
    let request = DecisionRequest::new(
        "hello, see you tomorrow",
        DecisionQuestion::Choice {
            instructions: "Is this spam?".to_string(),
            criteria: BTreeMap::from([
                ("spam".to_string(), "Unwanted bulk message".to_string()),
                ("ham".to_string(), "Legitimate message".to_string()),
            ]),
        },
    );
    let response = engine.decide(&request).expect("decide");
    response.validate_against(&request).expect("valid answer");
    assert_eq!(
        response.answer.predicted_label(),
        "ham",
        "benign text should read ham"
    );
}

#[test]
fn decide_without_provider_errors() {
    let engine = TextIntelligence::default();
    let request = DecisionRequest::new("hello", routing_question());
    let error = engine
        .decide(&request)
        .expect_err("must fail without provider");
    assert!(error.to_string().contains("no decision provider"));
}

#[test]
fn malformed_requests_are_rejected() {
    let engine = similarity_engine();
    for (name, request) in [
        ("empty state", DecisionRequest::new("", routing_question())),
        (
            "blank state",
            DecisionRequest::new("   ", routing_question()),
        ),
        (
            "single criterion",
            DecisionRequest::new(
                "hello",
                DecisionQuestion::Choice {
                    instructions: "Pick one.".to_string(),
                    criteria: BTreeMap::from([("only".to_string(), "The only option".to_string())]),
                },
            ),
        ),
        (
            "blank description",
            DecisionRequest::new(
                "hello",
                DecisionQuestion::Choice {
                    instructions: "Pick one.".to_string(),
                    criteria: BTreeMap::from([
                        ("a".to_string(), "Fine".to_string()),
                        ("b".to_string(), "  ".to_string()),
                    ]),
                },
            ),
        ),
        (
            "blank task",
            DecisionRequest::new("hello", routing_question()).with_task("  "),
        ),
    ] {
        assert!(engine.decide(&request).is_err(), "{name} must fail");
        assert!(request.validate().is_err(), "{name} must not validate");
    }
}

#[test]
fn oversized_state_is_rejected() {
    let engine = similarity_engine();
    let state = "x".repeat(textintel::MAX_DECISION_STATE_CHARS + 1);
    let request = DecisionRequest::new(state, routing_question());
    assert!(request.validate().is_err());
    assert!(engine.decide(&request).is_err());
}

#[test]
fn adapters_reject_unsupported_questions() {
    let similarity = SimilarityDecisionProvider::new(Arc::new(ProfileSimilarityScorer::default()));
    let binary = DecisionRequest::new(
        "hello",
        DecisionQuestion::Binary {
            statement: "This is spam.".to_string(),
        },
    );
    assert!(similarity.decide(&binary).is_err());

    let spam = SpamDecisionProvider::new(Arc::new(HeuristicSpamPredictor));
    let score = DecisionRequest::new(
        "hello",
        DecisionQuestion::Score {
            instructions: "Rate severity.".to_string(),
            levels: vec!["none".to_string(), "low".to_string(), "high".to_string()],
        },
    );
    assert!(spam.decide(&score).is_err());
}

#[test]
fn adapters_require_prepared_evidence() {
    // Calling a provider directly (bypassing the engine) without analyzed
    // fingerprints must fail with an explicit error, never panic.
    let similarity = SimilarityDecisionProvider::new(Arc::new(ProfileSimilarityScorer::default()));
    let request = DecisionRequest::new("hello", routing_question());
    let error = similarity.decide(&request).expect_err("needs evidence");
    assert!(error.message.contains("fingerprint"));
}

#[test]
fn spam_labels_must_be_valid() {
    let predictor = Arc::new(HeuristicSpamPredictor);
    assert!(
        SpamDecisionProvider::new(predictor.clone())
            .with_labels("spam", "ham")
            .is_ok()
    );
    assert!(
        SpamDecisionProvider::new(predictor.clone())
            .with_labels("same", "same")
            .is_err()
    );
    assert!(
        SpamDecisionProvider::new(predictor)
            .with_labels("", "ham")
            .is_err()
    );
}

#[test]
fn fingerprint_cache_serves_identical_evidence() {
    let mut config = textintel::EngineConfig::default();
    config.cache.decision = 64;
    let engine = TextIntelligence::new(config).with_decision_provider(
        SimilarityDecisionProvider::new(Arc::new(ProfileSimilarityScorer::default())),
    );
    let request = DecisionRequest::new("I was charged twice, refund me", routing_question());
    let first = engine.decide(&request).expect("decide");
    let diagnostics = engine.diagnostics();
    let misses = diagnostics.caches["decision"].misses;
    assert!(misses >= 4, "first pass populates: {misses} misses");
    let second = engine.decide(&request).expect("decide again");
    assert_eq!(first, second, "cached evidence decides identically");
    let diagnostics = engine.diagnostics();
    assert!(
        diagnostics.caches["decision"].hits >= 4,
        "second pass hits: {:?}",
        diagnostics.caches["decision"]
    );
}

#[test]
fn provider_swaps_invalidate_fingerprint_cache() {
    let mut config = textintel::EngineConfig::default();
    config.cache.decision = 64;
    let engine = TextIntelligence::new(config);
    let mut request = DecisionRequest::new("hello world", routing_question());
    engine
        .prepare_decision_request(&mut request)
        .expect("prepare");
    assert!(engine.diagnostics().caches["decision"].entries > 0);
    // Swapping an analysis-affecting provider drops cached fingerprints.
    let swapped = engine.without_entities();
    assert_eq!(swapped.diagnostics().caches["decision"].entries, 0);
}

#[test]
fn interaction_provider_caches_criterion_encodings() {
    use textintel::decision::{InteractionArtifact, InteractionDecisionProvider, init_head_xavier};

    // Deterministic 8-dim backbone needs no weights.
    let backbone = Arc::new(textintel::FeatureHashEmbeddingProvider::new(8).expect("dims"));
    let head = init_head_xavier(8 * 4 + textintel::FUSION_FEATURES.len(), 4, 7).expect("head");
    let artifact =
        InteractionArtifact::from_head(&head, 8, textintel::FUSION_FEATURES.len(), "test")
            .expect("artifact");
    let provider = InteractionDecisionProvider::new(backbone, &artifact).expect("provider");
    let engine = TextIntelligence::default().with_decision_provider(provider);
    let request = DecisionRequest::new("I was charged twice, refund me", routing_question());
    let first = engine.decide(&request).expect("decide");
    first.validate_against(&request).expect("valid answer");
    let second = engine.decide(&request).expect("decide again");
    assert_eq!(first, second, "cached encodings decide identically");
}

#[test]
fn decision_request_json_roundtrip_skips_evidence() {
    let request = DecisionRequest::new("I was charged twice", routing_question())
        .with_task("support-routing");
    let json = serde_json::to_string(&request).expect("serialize");
    assert!(!json.contains("fingerprint"));
    let parsed: DecisionRequest = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed.state, request.state);
    assert_eq!(parsed.question, request.question);
    assert_eq!(parsed.task, request.task);
}
