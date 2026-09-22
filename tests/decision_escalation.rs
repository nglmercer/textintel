//! Selective classification: accept/escalate verdicts and the
//! risk/coverage curve behind them.

use std::collections::BTreeMap;
use std::sync::Arc;

use textintel::decision::{
    COVERAGE_LEVELS, Decision, DecisionExample, DecisionQuestion, DecisionRequest,
    SimilarityDecisionProvider, check_decision_gates, evaluate_decisions,
};
use textintel::{ProfileSimilarityScorer, TextIntelligence};

fn routing_criteria() -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "billing".to_string(),
            "Payments, invoices and refunds.".to_string(),
        ),
        (
            "technical".to_string(),
            "Errors, crashes and login failures.".to_string(),
        ),
    ])
}

fn engine_with_threshold(threshold: f64) -> TextIntelligence {
    let provider = SimilarityDecisionProvider::new(Arc::new(ProfileSimilarityScorer::default()))
        .with_accept_threshold(threshold)
        .expect("threshold");
    TextIntelligence::default().with_decision_provider(provider)
}

#[test]
fn high_thresholds_escalate_uncertain_answers() {
    let request = DecisionRequest::new(
        "hello there",
        DecisionQuestion::Choice {
            instructions: "Which team?".to_string(),
            criteria: routing_criteria(),
        },
    );
    let lenient = engine_with_threshold(0.0).decide(&request).expect("decide");
    assert_eq!(lenient.decision, Decision::Accept);
    let strict = engine_with_threshold(1.0).decide(&request).expect("decide");
    assert_eq!(strict.decision, Decision::Escalate);
    assert_eq!(
        lenient.answer, strict.answer,
        "thresholds only move the verdict"
    );
}

#[test]
fn invalid_thresholds_are_rejected() {
    let provider = SimilarityDecisionProvider::new(Arc::new(ProfileSimilarityScorer::default()));
    assert!(provider.with_accept_threshold(1.5).is_err());
}

#[test]
fn coverage_curve_reports_risk_at_each_level() {
    let engine = TextIntelligence::default();
    let provider = SimilarityDecisionProvider::new(Arc::new(ProfileSimilarityScorer::default()));
    let examples = vec![
        ("refund my duplicate payment now", "billing"),
        ("invoice receipt subscription charge", "billing"),
        ("the app crashes on login", "technical"),
        ("error dialog crash bug report", "technical"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (state, gold))| DecisionExample {
        id: format!("escalation_{index}"),
        state: state.to_string(),
        question: DecisionQuestion::Choice {
            instructions: "Which team?".to_string(),
            criteria: routing_criteria(),
        },
        gold: gold.to_string(),
        teacher_probabilities: None,
        task: None,
    })
    .collect::<Vec<_>>();
    let report = evaluate_decisions(&engine, &provider, "test", "test", &examples).expect("eval");
    assert_eq!(report.count, 4);
    assert_eq!(report.skipped, 0);
    assert_eq!(report.coverage.len(), COVERAGE_LEVELS.len());
    let full = &report.coverage[0];
    assert!((full.coverage - 1.0).abs() < 1e-9);
    assert!((full.accuracy - report.accuracy).abs() < 1e-9);
    // Thresholds rise as coverage shrinks (top slice is most confident).
    let thresholds: Vec<f64> = report
        .coverage
        .iter()
        .map(|point| point.threshold)
        .collect();
    for window in thresholds.windows(2) {
        assert!(
            window[0] <= window[1] + 1e-9,
            "thresholds rise: {thresholds:?}"
        );
    }
}

#[test]
fn decision_gates_enforce_thresholds() {
    let engine = TextIntelligence::default();
    let provider = SimilarityDecisionProvider::new(Arc::new(ProfileSimilarityScorer::default()));
    let examples = vec![DecisionExample {
        id: "gate_0".to_string(),
        state: "refund my duplicate payment now".to_string(),
        question: DecisionQuestion::Choice {
            instructions: "Which team?".to_string(),
            criteria: routing_criteria(),
        },
        gold: "billing".to_string(),
        teacher_probabilities: None,
        task: None,
    }];
    let report = evaluate_decisions(&engine, &provider, "test", "test", &examples).expect("eval");
    let passing = serde_json::json!({
        "decision": {"accuracy_min": 0.0},
        "coverage_100": {"accuracy_min": 0.0},
        "unknown_section": {"whatever_min": 99.0},
    });
    assert!(check_decision_gates(&report, &passing).is_empty());
    let failing = serde_json::json!({
        "decision": {"accuracy_min": 1.01},
        "coverage_50": {"accuracy_min": 1.01},
    });
    let failures = check_decision_gates(&report, &failing);
    assert_eq!(failures.len(), 2);
    assert!(
        failures
            .iter()
            .any(|failure| failure.contains("decision.accuracy"))
    );
    assert!(
        failures
            .iter()
            .any(|failure| failure.contains("coverage_50"))
    );
    // Non-object gates files pass vacuously (forwards compatible).
    assert!(check_decision_gates(&report, &serde_json::json!([])).is_empty());
}
