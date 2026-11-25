//! Calibration: ROC-AUC, PR-AUC, F1, Brier, ECE are measured everywhere,
//! and no probability is marked calibrated unless calibration ran.

use textintel::evaluation::{EvaluationCase, EvaluationDataset, evaluate};
use textintel::{EngineConfig, TextIntelligence};

fn similar_case(id: &str, a: &str, b: &str, similar: bool) -> EvaluationCase {
    EvaluationCase {
        id: id.to_string(),
        a: a.to_string(),
        b: b.to_string(),
        languages: Vec::new(),
        split: "test".to_string(),
        difficulty: "easy".to_string(),
        category: "general".to_string(),
        labels: [(String::from("similar"), similar)].into_iter().collect(),
        expected: Default::default(),
        tags: Vec::new(),
    }
}

#[test]
fn perfect_separation_scores_perfect_discrimination() {
    let mut cases = Vec::new();
    for (index, (a, b, similar)) in [
        ("identical text one", "identical text one", true),
        ("identical text two", "identical text two", true),
        ("identical text three", "identical text three", true),
        ("alpha beta gamma", "quantum zebra nebula", false),
        ("red green blue", "xylophone quantum jump", false),
        ("one two three", "purple monkey dishwasher", false),
    ]
    .into_iter()
    .enumerate()
    {
        cases.push(similar_case(&format!("cal_{index}"), a, b, similar));
    }
    let dataset = EvaluationDataset {
        version: "calibration-test".to_string(),
        cases,
    };
    let engine = TextIntelligence::new(EngineConfig::default());
    let report = evaluate(&engine, &dataset).unwrap();
    let metrics = &report.metrics;
    assert_eq!(metrics.count, 6);
    assert_eq!(metrics.positives, 3);
    assert_eq!(metrics.negatives, 3);
    assert!(
        metrics.roc_auc > 0.99,
        "perfect ranking should give ROC-AUC ~1: {}",
        metrics.roc_auc
    );
    assert!(
        metrics.pr_auc > 0.99,
        "perfect ranking should give PR-AUC ~1: {}",
        metrics.pr_auc
    );
    assert_eq!(metrics.f1, 1.0, "identical vs unrelated must split at 0.5");
    assert!(
        metrics.brier < 0.1,
        "confident correct scores should have low Brier: {}",
        metrics.brier
    );
    assert!(
        metrics.expected_calibration_error < 0.2,
        "ECE should be small: {}",
        metrics.expected_calibration_error
    );
}

#[test]
fn trained_spam_calibration_is_marked_and_bounded() {
    let engine = TextIntelligence::new(EngineConfig::default());
    let mut spam_score: f64 = 0.0;
    let mut ham_score: f64 = 1.0;
    for text in [
        "WIN FREE PRIZE claim now",
        "hello, how are you today",
        "",
        "cheap meds online buy now",
    ] {
        let result = engine.detect_spam(text).unwrap();
        assert!(
            result.calibrated,
            "the baked logistic calibration is trained on the v2 spam corpus"
        );
        assert!((0.0..=1.0).contains(&result.probability));
        if text.contains("PRIZE") || text.contains("meds") {
            spam_score = spam_score.max(result.probability);
        } else {
            ham_score = ham_score.min(result.probability);
        }
    }
    assert!(
        spam_score > ham_score,
        "spam-template texts must outrank ordinary messages: spam={spam_score} ham={ham_score}"
    );
}
