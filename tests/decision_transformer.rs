//! v1 cross-encoder support: prompt format, candidate batch planning,
//! head configuration, and backbone validation.

use std::collections::BTreeMap;

use textintel::decision::{
    CrossEncoderHeadConfig, DecisionArtifact, DecisionQuestion, DecisionRequest, candidate_prompt,
    plan_candidate_batch,
};

fn routing_request() -> DecisionRequest {
    DecisionRequest::new(
        "I was charged twice",
        DecisionQuestion::Choice {
            instructions: "Which category applies?".to_string(),
            criteria: BTreeMap::from([
                ("billing".to_string(), "Payments and refunds.".to_string()),
                ("technical".to_string(), "Product problems.".to_string()),
            ]),
        },
    )
}

#[test]
fn prompt_carries_question_option_and_state() {
    let prompt = candidate_prompt(
        "Which category applies?",
        "billing",
        "Payments, invoices and refunds.",
        "I was charged twice",
    )
    .expect("prompt");
    for section in ["QUESTION:", "OPTION:", "STATE:"] {
        assert!(prompt.contains(section), "missing {section}:\n{prompt}");
    }
    assert!(prompt.contains("billing"));
    assert!(prompt.contains("Payments, invoices and refunds."));
    assert!(prompt.contains("I was charged twice"));
    // Fixed section order.
    let question = prompt.find("QUESTION:").expect("question");
    let option = prompt.find("OPTION:").expect("option");
    let state = prompt.find("STATE:").expect("state");
    assert!(question < option && option < state);
}

#[test]
fn prompt_rejects_blank_inputs() {
    assert!(candidate_prompt("", "a", "desc", "state").is_err());
    assert!(candidate_prompt("q", "", "desc", "state").is_err());
    assert!(candidate_prompt("q", "a", "  ", "state").is_err());
    assert!(candidate_prompt("q", "a", "desc", "").is_err());
}

#[test]
fn batch_plans_one_prompt_per_criterion_in_id_order() {
    let batch = plan_candidate_batch(&routing_request()).expect("batch");
    assert_eq!(batch.len(), 2);
    assert_eq!(batch[0].option_id, "billing");
    assert_eq!(batch[1].option_id, "technical");
    assert!(batch[0].prompt.contains("Payments and refunds."));
    assert!(batch[1].prompt.contains("Product problems."));
    assert!(
        batch
            .iter()
            .all(|candidate| candidate.prompt.contains("I was charged twice"))
    );
}

#[test]
fn batch_rejects_non_choice_questions() {
    let binary = DecisionRequest::new(
        "hello",
        DecisionQuestion::Binary {
            statement: "This is spam.".to_string(),
        },
    );
    assert!(plan_candidate_batch(&binary).is_err());
    let invalid = DecisionRequest::new("", routing_request().question.clone());
    assert!(plan_candidate_batch(&invalid).is_err());
}

#[test]
fn head_config_comes_from_valid_artifacts() {
    let artifact = DecisionArtifact::from_json(
        &serde_json::json!({
            "artifact_version": 1,
            "kind": "textintel_decision",
            "architecture": "candidate_cross_encoder",
            "backbone": {"model_id": "x", "model_type": "bert", "hidden_size": 16},
            "decision_schema_version": 1,
            "feature_schema_version": 1,
            "head": {"hidden_size": 32, "output_size": 1},
        })
        .to_string(),
    )
    .expect("artifact");
    let head = CrossEncoderHeadConfig::from_artifact(&artifact).expect("head");
    assert_eq!(head.hidden_size, 32);
    assert_eq!(head.output_size, 1);
}

#[cfg(feature = "decision-transformer")]
#[test]
fn backbone_opens_the_mini_fixture() {
    use textintel::decision::TransformerBackbone;

    let backbone =
        TransformerBackbone::open("tests/fixtures/mini-transformer").expect("fixture opens");
    let metadata = backbone.model_metadata().expect("metadata");
    assert_eq!(metadata.dimensions, 16);
    assert_eq!(metadata.model_id, "textintel-mini-bert-fixture");
}

#[cfg(feature = "decision-transformer")]
#[test]
fn backbone_opens_the_unigram_fixture() {
    use textintel::decision::TransformerBackbone;

    // Second tokenizer path (`tokenizer.json` instead of `vocab.txt`).
    let backbone = TransformerBackbone::open("tests/fixtures/mini-unigram").expect("fixture opens");
    let metadata = backbone.model_metadata().expect("metadata");
    assert_eq!(metadata.dimensions, 8);
    assert_eq!(metadata.model_id, "textintel-mini-unigram-fixture");
}

#[cfg(feature = "decision-transformer")]
#[test]
fn backbone_rejects_bad_directories() {
    use textintel::decision::TransformerBackbone;

    assert!(TransformerBackbone::open("tests/fixtures").is_err());
    assert!(TransformerBackbone::open("tests/fixtures/does-not-exist").is_err());
}

#[cfg(feature = "decision-transformer")]
#[test]
fn backbone_checks_artifact_hidden_size() {
    use textintel::decision::TransformerBackbone;

    let backbone =
        TransformerBackbone::open("tests/fixtures/mini-transformer").expect("fixture opens");
    let matching = DecisionArtifact::from_json(
        &serde_json::json!({
            "artifact_version": 1,
            "kind": "textintel_decision",
            "architecture": "candidate_cross_encoder",
            "backbone": {"model_id": "fixture", "model_type": "bert", "hidden_size": 16},
            "decision_schema_version": 1,
            "feature_schema_version": 1,
            "head": {"hidden_size": 8, "output_size": 1},
        })
        .to_string(),
    )
    .expect("artifact");
    assert!(backbone.check_artifact(&matching).is_ok());
    let mismatched = DecisionArtifact::from_json(
        &serde_json::json!({
            "artifact_version": 1,
            "kind": "textintel_decision",
            "architecture": "candidate_cross_encoder",
            "backbone": {"model_id": "fixture", "model_type": "bert", "hidden_size": 384},
            "decision_schema_version": 1,
            "feature_schema_version": 1,
            "head": {"hidden_size": 8, "output_size": 1},
        })
        .to_string(),
    )
    .expect("artifact");
    let error = backbone.check_artifact(&mismatched).expect_err("must fail");
    assert!(error.contains("hidden size"));
}
