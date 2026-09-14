//! A real reranker (§12): bounded logistic rescoring over channel evidence.
//! The reranker reorders but never drops candidates.

use textintel::core::providers::RerankerProvider;
use textintel::engine::TextIntelligence;
use textintel::{ChannelRerankWeights, ChannelScoreReranker};

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
