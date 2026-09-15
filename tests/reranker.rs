//! A real reranker (§12): bounded logistic rescoring over channel evidence.
//! The reranker reorders but never drops candidates.

use textintel::core::providers::RerankerProvider;
use textintel::engine::TextIntelligence;
use textintel::{ChannelRerankWeights, ChannelScoreReranker, RerankerModelArtifact};

fn engine() -> TextIntelligence {
    TextIntelligence::default()
}

#[test]
fn channel_evidence_corrects_an_inverted_retrieval_order() {
    let engine = engine();
    let query = engine.analyze("gana dinero").expect("analyze query");
    let close = engine.analyze("gana dinero ahora").expect("analyze close");
    let far = engine.analyze("el cielo es azul").expect("analyze far");

    // Simulate a retrieval stage that returned the wrong order.
    let candidates = vec![
        ("far".to_string(), far, 0.85),
        ("close".to_string(), close, 0.25),
    ];
    let reranker =
        ChannelScoreReranker::new(ChannelRerankWeights::default(), 10).expect("valid weights");
    let reranked = reranker.rerank(&query, candidates).expect("rerank");
    assert_eq!(reranked.len(), 2, "reranking must not drop candidates");
    assert_eq!(
        reranked[0].0, "close",
        "channels must outvote the wrong order"
    );
    assert_eq!(reranked[1].0, "far");
    assert!(
        reranked[0].2 > 0.25,
        "corrected score should rise, got {}",
        reranked[0].2
    );
    assert!(
        reranked[1].2 < 0.85,
        "wrong leader should fall, got {}",
        reranked[1].2
    );
    for (_, _, score) in &reranked {
        assert!(score.is_finite() && (0.0..=1.0).contains(score));
    }
}

#[test]
fn reranking_is_bounded_and_total() {
    let engine = engine();
    let query = engine.analyze("hola mundo").expect("analyze query");
    let candidates: Vec<(String, textintel::core::types::MessageFingerprint, f64)> = (0..5)
        .map(|index| {
            let fingerprint = engine
                .analyze(&format!("documento numero {index}"))
                .expect("analyze doc");
            (
                format!("doc{index}"),
                fingerprint,
                0.9 - f64::from(index) * 0.1,
            )
        })
        .collect();
    let reranker =
        ChannelScoreReranker::new(ChannelRerankWeights::default(), 2).expect("valid weights");
    let reranked = reranker.rerank(&query, candidates).expect("rerank");
    assert_eq!(reranked.len(), 5, "tail candidates must pass through");
    // Tail keeps original scores verbatim (only the head is rescored).
    let tail: Vec<f64> = reranked[2..].iter().map(|item| item.2).collect();
    assert_eq!(tail.len(), 3);
    for score in tail {
        assert!(score.is_finite());
    }
}

#[test]
fn invalid_configurations_are_rejected() {
    let weights = ChannelRerankWeights {
        base: f64::NAN,
        ..ChannelRerankWeights::default()
    };
    assert!(ChannelScoreReranker::new(weights, 10).is_err());
    assert!(
        ChannelScoreReranker::new(ChannelRerankWeights::default(), 0).is_err(),
        "max_candidates must be positive"
    );
}

#[test]
fn empty_input_stays_empty() {
    let engine = engine();
    let query = engine.analyze("hola").expect("analyze query");
    let reranker =
        ChannelScoreReranker::new(ChannelRerankWeights::default(), 10).expect("valid weights");
    assert!(reranker
        .rerank(&query, Vec::new())
        .expect("rerank")
        .is_empty());
}

#[test]
fn deterministic_artifact_round_trips_with_baseline_revision() {
    let artifact = RerankerModelArtifact::deterministic("0.5.0");
    let loaded = RerankerModelArtifact::from_json(&artifact.to_json().unwrap()).unwrap();
    assert_eq!(loaded, artifact);
    assert_eq!(loaded.revision.as_deref(), Some("channel-reranker-v1"));
    let reranker = loaded.to_reranker(16).unwrap();
    assert_eq!(reranker.max_candidates(), 16);
    assert_eq!(
        reranker.capabilities().model_revision.as_deref(),
        Some("channel-reranker-v1")
    );
}

#[test]
fn artifact_rejects_bad_versions_and_weights() {
    let mut bad_version =
        serde_json::to_value(RerankerModelArtifact::deterministic("0.5.0")).unwrap();
    bad_version["artifact_version"] = serde_json::json!(999);
    assert!(RerankerModelArtifact::from_json(&bad_version.to_string()).is_err());
    let mut bad_kind = serde_json::to_value(RerankerModelArtifact::deterministic("0.5.0")).unwrap();
    bad_kind["kind"] = serde_json::json!("something_else");
    assert!(RerankerModelArtifact::from_json(&bad_kind.to_string()).is_err());
    // Non-finite weights cannot arrive via JSON (serde_json maps them to
    // null, which fails f64 deserialization); a wrong type must also fail.
    let mut bad_weights =
        serde_json::to_value(RerankerModelArtifact::deterministic("0.5.0")).unwrap();
    bad_weights["weights"]["base"] = serde_json::json!("nan");
    assert!(RerankerModelArtifact::from_json(&bad_weights.to_string()).is_err());
    let overflow = RerankerModelArtifact::new(
        "0.5.0",
        ChannelRerankWeights {
            base: f64::INFINITY,
            ..ChannelRerankWeights::default()
        },
    );
    assert!(overflow.to_reranker(8).is_err());
}

#[test]
fn trained_weights_load_through_the_builder() {
    let path = std::env::temp_dir().join(format!("textintel-reranker-{}.json", std::process::id()));
    let artifact = RerankerModelArtifact::new(
        "0.5.0",
        ChannelRerankWeights {
            base: 2.0,
            ..ChannelRerankWeights::default()
        },
    )
    .with_revision("trained-test");
    std::fs::write(&path, artifact.to_json().unwrap()).unwrap();
    let engine = TextIntelligence::builder()
        .trained_reranker_model(&path)
        .reranker_max_candidates(4)
        .build()
        .unwrap();
    let diagnostics = engine.diagnostics();
    let reranker = diagnostics.reranker.expect("reranker must load");
    assert_eq!(reranker.provider, "channel_score_reranker");
    assert_eq!(reranker.model_revision.as_deref(), Some("trained-test"));
    assert!(
        diagnostics
            .degraded
            .iter()
            .all(|item| item.capability != "reranker"),
        "loaded reranker must clear the degradation note"
    );
    let _ = std::fs::remove_file(&path);

    let missing = TextIntelligence::builder()
        .trained_reranker_model("models/does-not-exist.json")
        .build();
    assert!(missing.is_err(), "missing required reranker must fail");
}

#[test]
fn reranked_search_never_bypasses_full_comparison() {
    // Every reranked hit must be a fully-compared store record: no injected
    // ids, bounded output, finite scores, intact channel evidence.
    let path = std::env::temp_dir().join(format!(
        "textintel-reranker-search-{}.json",
        std::process::id()
    ));
    let artifact = RerankerModelArtifact::deterministic("0.5.0");
    std::fs::write(&path, artifact.to_json().unwrap()).unwrap();
    let engine = TextIntelligence::builder()
        .trained_reranker_model(&path)
        .reranker_max_candidates(2)
        .build()
        .unwrap();
    for (id, text) in [
        ("a", "gana dinero ahora"),
        ("b", "el cielo es azul"),
        ("c", "gana dinero rapido"),
        ("d", "la luna brilla"),
    ] {
        engine.add_document(id, text).unwrap();
    }
    let results = engine.find_similar("gana dinero", 10).unwrap();
    assert!(!results.is_empty());
    for result in &results {
        assert!(["a", "b", "c", "d"].contains(&result.id.as_str()));
        assert!(result.score.is_finite() && (0.0..=1.0).contains(&result.score));
        assert!(
            result.comparison.decoded_similarity.is_some(),
            "full comparison channels must survive reranking"
        );
    }
    let _ = std::fs::remove_file(&path);
}
