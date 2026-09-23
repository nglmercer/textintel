//! Decision-path benchmarks: analysis, embedding, and end-to-end
//! decisions. Transformer benches skip gracefully when the (gitignored)
//! backbone weights are absent, so CI stays green.

use std::collections::BTreeMap;
use std::hint::black_box;
use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};

use textintel::decision::{DecisionQuestion, DecisionRequest, SimilarityDecisionProvider};
#[cfg(feature = "semantic-transformer")]
use textintel::decision::{InteractionArtifact, InteractionDecisionProvider};
use textintel::{ProfileSimilarityScorer, TextIntelligence};

const STATE: &str = "I was charged twice for my subscription. Please refund the duplicate payment.";
const BACKBONE: &str = "models/e5-small-decision";
const HEAD: &str = "models/decision-s1-v1.json";

fn routing_question() -> DecisionQuestion {
    DecisionQuestion::Choice {
        instructions: "Which team should handle this message?".to_string(),
        criteria: BTreeMap::from([
            (
                "billing".to_string(),
                "Payment, invoice, billing and refund problems.".to_string(),
            ),
            (
                "sales".to_string(),
                "Purchasing and pricing questions.".to_string(),
            ),
            (
                "technical".to_string(),
                "Product or technical problems.".to_string(),
            ),
        ]),
    }
}

fn transformer_ready() -> bool {
    std::path::Path::new(BACKBONE)
        .join("model.safetensors")
        .is_file()
        && std::path::Path::new(HEAD).is_file()
}

fn decision_benches(c: &mut Criterion) {
    let engine = TextIntelligence::default();
    c.bench_function("decision_analyze_state", |bench| {
        bench.iter(|| engine.analyze(black_box(STATE)).expect("analyze"));
    });
    c.bench_function("decision_prepare_request", |bench| {
        bench.iter(|| {
            let mut request = DecisionRequest::new(STATE.to_string(), routing_question());
            engine
                .prepare_decision_request(&mut request)
                .expect("prepare");
            black_box(request);
        });
    });

    let similarity = TextIntelligence::default().with_decision_provider(
        SimilarityDecisionProvider::new(Arc::new(ProfileSimilarityScorer::default())),
    );
    c.bench_function("decision_similarity_end_to_end", |bench| {
        bench.iter(|| {
            let request = DecisionRequest::new(STATE.to_string(), routing_question());
            black_box(similarity.decide(&request).expect("decide"));
        });
    });

    if transformer_ready() {
        #[cfg(feature = "semantic-transformer")]
        {
            use textintel::TransformerEmbeddingProvider;
            use textintel::core::providers::EmbeddingProvider;

            let backbone = TransformerEmbeddingProvider::open(BACKBONE).expect("backbone opens");
            c.bench_function("decision_embed_single", |bench| {
                let texts = vec![STATE.to_string()];
                bench.iter(|| black_box(backbone.embed(&texts).expect("embed")));
            });
            c.bench_function("decision_embed_batch5", |bench| {
                let texts = vec![STATE.to_string(); 5];
                bench.iter(|| black_box(backbone.embed(&texts).expect("embed")));
            });

            let artifact = InteractionArtifact::from_file(HEAD).expect("head loads");
            // True cold baseline: a fresh provider (empty embedding cache)
            // and an engine with the fingerprint cache disabled, so every
            // iteration re-analyzes and re-encodes state + all criteria.
            let cold_backbone: Arc<TransformerEmbeddingProvider> = Arc::new(backbone);
            let mut cold_config = textintel::EngineConfig::default();
            cold_config.cache.decision = 0;
            c.bench_function("decision_interaction_cold", |bench| {
                bench.iter(|| {
                    let provider =
                        InteractionDecisionProvider::new(cold_backbone.clone(), &artifact)
                            .expect("provider");
                    let engine =
                        TextIntelligence::new(cold_config.clone()).with_decision_provider(provider);
                    let request = DecisionRequest::new(STATE.to_string(), routing_question());
                    black_box(engine.decide(&request).expect("decide"));
                });
            });
            // Fully hot path: repeated identical requests with the
            // fingerprint cache enabled (task replay / duplicate traffic).
            let mut cached_config = textintel::EngineConfig::default();
            cached_config.cache.decision = 256;
            let cached_engine = TextIntelligence::new(cached_config).with_decision_provider(
                InteractionDecisionProvider::new(
                    Arc::new(TransformerEmbeddingProvider::open(BACKBONE).expect("backbone")),
                    &artifact,
                )
                .expect("provider"),
            );
            c.bench_function("decision_interaction_hot", |bench| {
                bench.iter(|| {
                    let request = DecisionRequest::new(STATE.to_string(), routing_question());
                    black_box(cached_engine.decide(&request).expect("decide"));
                });
            });
            // Realistic repeated-task flow: every state is fresh but the
            // criteria stay fixed, so candidate evidence resolves from caches
            // while states analyze and encode for real.
            // Static counter: criterion re-invokes the outer routine per
            // phase, so a local counter would repeat texts across phases
            // and turn this bench into cache hits.
            static FRESH_COUNTER: std::sync::atomic::AtomicUsize =
                std::sync::atomic::AtomicUsize::new(0);
            c.bench_function("decision_interaction_fresh_states", |bench| {
                bench.iter(|| {
                    let counter = FRESH_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let state = format!("Customer message {counter}: {STATE}");
                    let request = DecisionRequest::new(state, routing_question());
                    black_box(cached_engine.decide(&request).expect("decide"));
                });
            });
        }
    } else {
        eprintln!("decision benches: transformer weights/head missing, skipping e5 benches");
    }
}

criterion_group!(benches, decision_benches);
criterion_main!(benches);
