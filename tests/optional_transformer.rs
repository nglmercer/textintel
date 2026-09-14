//! Transformer embedding mechanics (§37, §87, §10). Requires the
//! `semantic-transformer` feature and the tiny deterministic fixture in
//! `tests/fixtures/mini-transformer` (random weights: mechanics only, no
//! semantics). Paraphrase quality needs a real checkpoint and stays
//! optional/local.
#![cfg(feature = "semantic-transformer")]

use textintel::core::capabilities::CapabilityLevel;
use textintel::core::providers::EmbeddingProvider;
use textintel::{TransformerEmbeddingProvider, TransformerPooling};

const FIXTURE: &str = "tests/fixtures/mini-transformer";

fn provider() -> TransformerEmbeddingProvider {
    TransformerEmbeddingProvider::open(FIXTURE).expect("fixture must open")
}

fn norm(vector: &[f32]) -> f32 {
    vector.iter().map(|value| value * value).sum::<f32>().sqrt()
}

#[test]
fn fixture_opens_with_validated_metadata() {
    let provider = provider();
    assert_eq!(provider.dimensions(), 16);
    assert_eq!(provider.max_token_length(), 32);
    assert_eq!(provider.vocabulary_size(), 64);
    assert_eq!(provider.model_id(), "textintel-mini-bert-fixture");
    let capabilities = provider.capabilities();
    assert_eq!(capabilities.quality, CapabilityLevel::Production);
    assert!(capabilities.local);
    assert_eq!(capabilities.dimensions, Some(16));
    assert_eq!(capabilities.model_revision.as_deref(), Some("fixture-1"));
    let metadata = provider.model_metadata().expect("metadata");
    assert_eq!(metadata.dimensions, 16);
    assert!(metadata.normalized);
    assert_eq!(metadata.model_id, "textintel-mini-bert-fixture");
    provider.health_check().expect("health check");
}

#[test]
fn vectors_are_normalized_finite_and_deterministic() {
    let provider = provider();
    let texts = vec![
        "hello world".to_string(),
        "buy the ticket now".to_string(),
        "你好".to_string(),
        String::new(),
    ];
    let first = provider.embed(&texts).expect("embed");
    let second = provider.embed(&texts).expect("embed again");
    assert_eq!(first, second, "inference must be deterministic");
    for vector in &first {
        assert_eq!(vector.len(), 16);
        assert!(vector.iter().all(|value| value.is_finite()));
        assert!((norm(vector) - 1.0).abs() < 1e-4, "must be L2-normalized");
    }
    // Distinct inputs give distinct vectors.
    assert_ne!(first[0], first[1]);
}

#[test]
fn padding_does_not_leak_into_pooled_output() {
    let provider = provider();
    // Same text padded to different batch widths must encode identically:
    // masked positions contribute exactly zero through attention.
    let alone = provider.embed(&["hello world".to_string()]).expect("embed");
    let padded = provider
        .embed(&[
            "hello world".to_string(),
            "buy the ticket now today with words and some more text here".to_string(),
        ])
        .expect("embed batch");
    // Masked positions contribute exactly zero; only f32 summation-order
    // noise (~1e-8) may remain.
    for (left, right) in alone[0].iter().zip(padded[0].iter()) {
        assert!(
            (left - right).abs() < 1e-5,
            "padding leaked into the CLS vector: {left} vs {right}"
        );
    }
}

#[test]
fn order_changes_output_and_pooling_matters() {
    let provider = provider();
    let forward = provider
        .embed(&["buy ticket now".to_string()])
        .expect("embed");
    let backward = provider
        .embed(&["now ticket buy".to_string()])
        .expect("embed");
    assert_ne!(
        forward[0], backward[0],
        "position embeddings must make order matter"
    );
    let mean = TransformerEmbeddingProvider::open(FIXTURE)
        .expect("fixture must open")
        .with_pooling(TransformerPooling::Mean);
    let cls_vector = provider
        .embed(&["hello world test".to_string()])
        .expect("embed");
    let mean_vector = mean
        .embed(&["hello world test".to_string()])
        .expect("embed");
    assert_ne!(cls_vector[0], mean_vector[0]);
}

#[test]
fn truncation_is_reported_not_silent() {
    let provider = provider();
    let long = "word ".repeat(200);
    let batch = provider
        .encode_detailed(&[long, "short".to_string()])
        .expect("detailed");
    assert!(batch.truncated[0], "long input must flag truncation");
    assert!(!batch.truncated[1]);
    assert_eq!(batch.token_counts[0], provider.max_token_length());
    assert_eq!(batch.vectors.len(), 2);
    // embed() still serves a vector for truncated input.
    let vectors = provider.embed(&["word ".repeat(200)]).expect("embed");
    assert_eq!(vectors.len(), 1);
    assert_eq!(vectors[0].len(), provider.dimensions());
}

#[test]
fn batching_chunks_large_inputs() {
    let provider = provider().with_max_batch(2);
    let texts: Vec<String> = (0..5).map(|index| format!("test text {index}")).collect();
    let batched = provider.embed(&texts).expect("chunked embed");
    let single = TransformerEmbeddingProvider::open(FIXTURE)
        .expect("fixture must open")
        .embed(&texts)
        .expect("plain embed");
    assert_eq!(batched, single, "chunking must not change results");
}

#[test]
fn incompatible_models_fail_loudly() {
    assert!(TransformerEmbeddingProvider::open("tests/fixtures/does-not-exist").is_err());

    let dir = std::env::temp_dir().join("textintel-bad-transformer");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    std::fs::write(
        dir.join("config.json"),
        r#"{"model_type": "gpt2", "hidden_size": 8, "num_hidden_layers": 1,
            "num_attention_heads": 2, "intermediate_size": 8,
            "max_position_embeddings": 8, "vocab_size": 8}"#,
    )
    .expect("config write");
    let error = TransformerEmbeddingProvider::open(&dir).expect_err("non-BERT must fail");
    assert!(error.to_string().contains("gpt2"), "unexpected: {error}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn transformer_drives_engine_semantics() {
    let engine = textintel::TextIntelligence::builder()
        .semantic_provider(provider())
        .config(textintel::EngineConfig {
            semantic: true,
            ..Default::default()
        })
        .build()
        .expect("engine build");
    let fingerprint = engine.analyze("hello world").expect("analyze");
    assert!(
        !fingerprint.semantic_embeddings.is_empty(),
        "transformer vectors must land in the fingerprint"
    );
    let comparison = engine
        .compare("hello world", "hello world")
        .expect("compare");
    assert!(
        comparison.semantic.unwrap_or(0.0) > 0.99,
        "identical texts must be semantically identical"
    );
}
