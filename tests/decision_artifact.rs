//! Decision artifacts: versioned envelopes load when they match the
//! build and fail loudly otherwise.

use textintel::decision::{
    ARCHITECTURE_CANDIDATE_CROSS_ENCODER, DECISION_ARTIFACT_KIND, DECISION_ARTIFACT_VERSION,
    DECISION_SCHEMA_VERSION, DecisionArtifact, FUSION_FEATURE_SCHEMA_VERSION,
};

fn valid_artifact_json() -> serde_json::Value {
    serde_json::json!({
        "artifact_version": DECISION_ARTIFACT_VERSION,
        "kind": DECISION_ARTIFACT_KIND,
        "architecture": ARCHITECTURE_CANDIDATE_CROSS_ENCODER,
        "backbone": {
            "model_id": "intfloat/multilingual-e5-small",
            "revision": "test-revision",
            "model_type": "bert",
            "hidden_size": 384
        },
        "decision_schema_version": DECISION_SCHEMA_VERSION,
        "feature_schema_version": FUSION_FEATURE_SCHEMA_VERSION,
        "head": {"hidden_size": 128, "output_size": 1},
        "calibration": {"method": "temperature_bias", "temperature": 0.87},
        "dataset": {"version": "0.1.0-seed"}
    })
}

#[test]
fn valid_artifact_loads_and_roundtrips() {
    let artifact =
        DecisionArtifact::from_json(&valid_artifact_json().to_string()).expect("valid artifact");
    assert_eq!(artifact.backbone.model_id, "intfloat/multilingual-e5-small");
    assert_eq!(artifact.head.output_size, 1);
    let bias = artifact.temperature_bias().expect("calibration");
    assert!((bias.temperature - 0.87).abs() < 1e-12);
    let roundtrip =
        DecisionArtifact::from_json(&artifact.to_json().expect("serialize")).expect("roundtrip");
    assert_eq!(roundtrip, artifact);
}

#[test]
fn mismatched_envelopes_are_rejected() {
    for name in [
        "kind",
        "architecture",
        "model_type",
        "decision_schema",
        "feature_schema",
    ] {
        let mut json = valid_artifact_json();
        match name {
            "kind" => json["kind"] = serde_json::json!("something_else"),
            "architecture" => json["architecture"] = serde_json::json!("unknown_head"),
            "model_type" => json["backbone"]["model_type"] = serde_json::json!("modernbert"),
            "decision_schema" => json["decision_schema_version"] = serde_json::json!(999),
            "feature_schema" => json["feature_schema_version"] = serde_json::json!(999),
            _ => unreachable!(),
        }
        assert!(
            DecisionArtifact::from_json(&json.to_string()).is_err(),
            "{name} mismatch must fail"
        );
    }
    let mut version = valid_artifact_json();
    version["artifact_version"] = serde_json::json!(999);
    assert!(DecisionArtifact::from_json(&version.to_string()).is_err());
}

#[test]
fn invalid_calibration_and_heads_are_rejected() {
    let mut bad_temperature = valid_artifact_json();
    bad_temperature["calibration"]["temperature"] = serde_json::json!(0.0);
    assert!(DecisionArtifact::from_json(&bad_temperature.to_string()).is_err());

    let mut bad_method = valid_artifact_json();
    bad_method["calibration"]["method"] = serde_json::json!("alchemy");
    assert!(DecisionArtifact::from_json(&bad_method.to_string()).is_err());

    let mut bad_head = valid_artifact_json();
    bad_head["head"]["output_size"] = serde_json::json!(3);
    assert!(DecisionArtifact::from_json(&bad_head.to_string()).is_err());

    let mut empty_backbone = valid_artifact_json();
    empty_backbone["backbone"]["model_id"] = serde_json::json!("  ");
    assert!(DecisionArtifact::from_json(&empty_backbone.to_string()).is_err());
}

#[test]
fn missing_calibration_reads_identity() {
    let mut json = valid_artifact_json();
    json.as_object_mut().expect("object").remove("calibration");
    let artifact = DecisionArtifact::from_json(&json.to_string()).expect("valid");
    let bias = artifact.temperature_bias().expect("identity");
    assert!((bias.temperature - 1.0).abs() < 1e-12);
    assert!(bias.biases.is_empty());
}

#[test]
fn directory_loading_applies_calibration_override() {
    let dir = std::env::temp_dir().join(format!("textintel-decision-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("fixture dir");
    std::fs::write(dir.join("config.json"), valid_artifact_json().to_string()).expect("config");
    let plain = DecisionArtifact::from_dir(&dir).expect("load");
    assert!((plain.temperature_bias().expect("bias").temperature - 0.87).abs() < 1e-12);

    std::fs::write(
        dir.join("calibration.json"),
        r#"{"method": "temperature", "temperature": 0.5}"#,
    )
    .expect("override");
    let overridden = DecisionArtifact::from_dir(&dir).expect("load");
    assert!(
        (overridden.temperature_bias().expect("bias").temperature - 0.5).abs() < 1e-12,
        "calibration.json overrides the envelope"
    );

    let layout = DecisionArtifact::expected_layout(&dir);
    assert_eq!(layout.len(), 5);
    assert!(layout.iter().any(|path| path.ends_with("config.json")));
    assert!(
        layout
            .iter()
            .any(|path| path.ends_with("model.safetensors"))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_config_fails_with_path() {
    let missing = std::env::temp_dir().join("textintel-decision-definitely-missing");
    let error = DecisionArtifact::from_dir(&missing).expect_err("must fail");
    assert!(error.contains("config.json"));
}
