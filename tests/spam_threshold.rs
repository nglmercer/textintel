//! Spam decision threshold: the artifact carries its operating point and
//! inference honors it instead of a hardcoded `0.5`.

use textintel::core::providers::SpamPredictor;
use textintel::detection::spam::SpamModelArtifact;
use textintel::{SPAM_FEATURES, TextIntelligence};

fn zero_weights() -> std::collections::BTreeMap<String, f64> {
    SPAM_FEATURES
        .iter()
        .map(|name| ((*name).to_string(), 0.0))
        .collect()
}

#[test]
fn artifact_defaults_to_half_and_round_trips_threshold() {
    let artifact = SpamModelArtifact::new("test", zero_weights(), 0.0);
    assert_eq!(artifact.decision_threshold, 0.5);
    assert!(!artifact.calibrated);
    let loaded = SpamModelArtifact::from_json(&artifact.to_json().unwrap()).unwrap();
    assert_eq!(loaded.decision_threshold, 0.5);

    let tuned = SpamModelArtifact::new("test", zero_weights(), 0.0)
        .with_decision_threshold(0.31)
        .with_calibrated(true);
    let loaded = SpamModelArtifact::from_json(&tuned.to_json().unwrap()).unwrap();
    assert_eq!(loaded.decision_threshold, 0.31);
    assert!(loaded.calibrated);
}

#[test]
fn legacy_artifacts_without_threshold_still_load_at_half() {
    let mut source =
        serde_json::to_value(SpamModelArtifact::new("test", zero_weights(), 0.0)).unwrap();
    source.as_object_mut().unwrap().remove("decision_threshold");
    let loaded = SpamModelArtifact::from_json(&source.to_string()).unwrap();
    assert_eq!(loaded.decision_threshold, 0.5);
}

#[test]
fn artifact_rejects_non_finite_and_out_of_range_thresholds() {
    for bad in [f64::NAN, f64::INFINITY, -0.1, 1.5] {
        let artifact =
            SpamModelArtifact::new("test", zero_weights(), 0.0).with_decision_threshold(bad);
        let source = serde_json::to_value(&artifact).unwrap().to_string();
        assert!(
            SpamModelArtifact::from_json(&source).is_err(),
            "threshold {bad} must be rejected"
        );
    }
}

#[test]
fn inference_uses_artifact_threshold_not_hardcoded_half() {
    // Zero weights + zero bias => probability exactly 0.5 on any input.
    // A hardcoded `>= 0.5` check would label spam under every threshold.
    let engine = TextIntelligence::default();
    let fingerprint = engine.analyze("see you at noon").unwrap();
    assert!(fingerprint.raw.contains("noon"));

    let at_half = SpamModelArtifact::new("test", zero_weights(), 0.0)
        .with_decision_threshold(0.5)
        .to_predictor();
    let strict = SpamModelArtifact::new("test", zero_weights(), 0.0)
        .with_decision_threshold(0.5001)
        .to_predictor();
    let lax = SpamModelArtifact::new("test", zero_weights(), 0.0)
        .with_decision_threshold(0.49)
        .to_predictor();

    let base = at_half.predict(&fingerprint, &[]).unwrap();
    assert_eq!(base.probability, 0.5);
    assert_eq!(base.labels, vec!["spam".to_string()]);
    assert_eq!(strict.decision_threshold(), 0.5001);

    let denied = strict.predict(&fingerprint, &[]).unwrap();
    assert_eq!(denied.probability, 0.5);
    assert!(
        denied.labels.is_empty(),
        "threshold 0.5001 must suppress the 0.5 label"
    );

    let allowed = lax.predict(&fingerprint, &[]).unwrap();
    assert_eq!(allowed.labels, vec!["spam".to_string()]);
}
