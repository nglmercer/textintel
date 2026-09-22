//! Batch decisions: ordering, limits, and error propagation.

use std::collections::BTreeMap;
use std::sync::Arc;

use textintel::decision::{
    DecisionProvider, DecisionQuestion, DecisionRequest, SimilarityDecisionProvider,
};
use textintel::{ProfileSimilarityScorer, TextIntelligence};

fn provider() -> SimilarityDecisionProvider {
    SimilarityDecisionProvider::new(Arc::new(ProfileSimilarityScorer::default()))
}

fn prepared(engine: &TextIntelligence, state: &str) -> DecisionRequest {
    let mut request = DecisionRequest::new(
        state.to_string(),
        DecisionQuestion::Choice {
            instructions: "Pick one.".to_string(),
            criteria: BTreeMap::from([
                ("billing".to_string(), "Payments and refunds.".to_string()),
                ("technical".to_string(), "Product problems.".to_string()),
            ]),
        },
    );
    engine
        .prepare_decision_request(&mut request)
        .expect("prepare");
    request
}

#[test]
fn batch_preserves_order_and_matches_single_calls() {
    let engine = TextIntelligence::default();
    let provider = provider();
    let requests = vec![
        prepared(&engine, "refund my payment"),
        prepared(&engine, "the app crashes on login"),
        prepared(&engine, "refund my payment"),
    ];
    let batched = provider.decide_batch(&requests).expect("batch");
    assert_eq!(batched.len(), 3);
    for (request, response) in requests.iter().zip(&batched) {
        response.validate_against(request).expect("valid answer");
        let single = provider.decide(request).expect("single");
        assert_eq!(*response, single);
    }
    // Deterministic: identical inputs give identical outputs.
    assert_eq!(batched[0].answer, batched[2].answer);
}

#[test]
fn empty_batch_returns_empty() {
    let responses = provider().decide_batch(&[]).expect("empty batch");
    assert!(responses.is_empty());
}

#[test]
fn oversized_batch_is_rejected() {
    let engine = TextIntelligence::default();
    let requests = vec![prepared(&engine, "hello"); textintel::MAX_DECISION_BATCH + 1];
    let error = provider().decide_batch(&requests).expect_err("must fail");
    assert!(error.message.contains("exceeds maximum"));
}

#[test]
fn batch_stops_on_first_invalid_request() {
    let engine = TextIntelligence::default();
    let requests = vec![
        prepared(&engine, "hello"),
        DecisionRequest::new(
            "",
            DecisionQuestion::Binary {
                statement: "This is spam.".to_string(),
            },
        ),
    ];
    assert!(provider().decide_batch(&requests).is_err());
}

#[test]
fn arc_providers_forward_batch_and_info() {
    let engine = TextIntelligence::default();
    let shared: Arc<dyn DecisionProvider> = Arc::new(provider());
    let requests = vec![prepared(&engine, "refund my payment")];
    let responses = shared.decide_batch(&requests).expect("batch");
    assert_eq!(responses.len(), 1);
    assert_eq!(
        shared.capabilities().provider,
        "similarity_decision_adapter"
    );
    assert_eq!(shared.model_info().provider, "similarity_decision_adapter");
    assert!(shared.health_check().is_ok());
}
