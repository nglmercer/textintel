//! Unigram-tokenizer transformer mechanics (modern e5-style layout).
//!
//! Requires the `semantic-transformer` feature and the tiny deterministic
//! fixture in `tests/fixtures/mini-unigram` (random weights, `bert.`-prefixed
//! tensor names: mechanics only, no semantics). These tests prove a
//! `tokenizer.json` (SentencePiece-Unigram) checkpoint opens, embeds
//! deterministically, honors text prefixes, and reports truncation — the
//! loading path real `multilingual-e5-small` directories take.
#![cfg(feature = "semantic-transformer")]

use textintel::core::capabilities::CapabilityLevel;
use textintel::core::providers::EmbeddingProvider;
use textintel::{TransformerEmbeddingProvider, TransformerPooling};

const FIXTURE: &str = "tests/fixtures/mini-unigram";

fn provider() -> TransformerEmbeddingProvider {
    TransformerEmbeddingProvider::open(FIXTURE).expect("fixture must open")
}

fn norm(vector: &[f32]) -> f32 {
    vector.iter().map(|value| value * value).sum::<f32>().sqrt()
}

#[test]
fn unigram_fixture_opens_with_validated_metadata() {
    let provider = provider();
    assert_eq!(provider.dimensions(), 8);
    assert_eq!(provider.max_token_length(), 16);
    assert_eq!(provider.vocabulary_size(), 10);
    assert_eq!(provider.tokenizer_kind(), "unigram");
    assert_eq!(provider.model_id(), "textintel-mini-unigram-fixture");
    let capabilities = provider.capabilities();
    assert_eq!(capabilities.quality, CapabilityLevel::Production);
    assert!(capabilities.local);
    assert_eq!(capabilities.dimensions, Some(8));
    let metadata = provider.model_metadata().expect("metadata");
    assert_eq!(metadata.dimensions, 8);
    assert!(metadata.normalized);
    provider.health_check().expect("health check");
}

#[test]
fn unigram_embeds_are_deterministic_and_normalized() {
    let provider = provider();
    let texts = vec!["hello world".to_string(), "hello".to_string()];
    let first = provider.embed(&texts).expect("embed");
    let second = provider.embed(&texts).expect("embed again");
    assert_eq!(first, second);
    assert_eq!(first.len(), 2);
    for vector in &first {
        assert_eq!(vector.len(), 8);
        assert!(vector.iter().all(|value| value.is_finite()));
        assert!((norm(vector) - 1.0).abs() < 1e-4, "L2-normalized");
    }
    // Different inputs take different Viterbi paths: vectors must differ.
    assert_ne!(first[0], first[1]);
}

#[test]
fn unigram_mean_pooling_and_prefix_change_vectors() {
    let default = provider();
    let mean = TransformerEmbeddingProvider::open(FIXTURE)
        .expect("fixture must open")
        .with_pooling(TransformerPooling::Mean);
    let texts = vec!["hello world".to_string()];
    let cls = default.embed(&texts).expect("cls embed");
    let pooled = mean.embed(&texts).expect("mean embed");
    assert_ne!(cls, pooled, "pooling must change the sentence vector");

    let prefixed = TransformerEmbeddingProvider::open(FIXTURE)
        .expect("fixture must open")
        .with_text_prefix("query: ");
    let plain = default.embed(&texts).expect("plain embed");
    let with_prefix = prefixed.embed(&texts).expect("prefixed embed");
    assert_ne!(plain, with_prefix, "prefix must change the encoding");
    // Empty prefixes are ignored: same vectors as no prefix.
    let empty = TransformerEmbeddingProvider::open(FIXTURE)
        .expect("fixture must open")
        .with_text_prefix("");
    assert_eq!(plain, empty.embed(&texts).expect("empty-prefix embed"));
}

#[test]
fn unigram_reports_truncation() {
    let provider = provider().with_max_batch(1);
    let detailed = provider
        .encode_detailed(&["hello ".repeat(100)])
        .expect("encode");
    assert_eq!(detailed.truncated, vec![true]);
    assert_eq!(detailed.token_counts, vec![16]);
    assert_eq!(detailed.vectors.len(), 1);
    let detailed = provider
        .encode_detailed(&["hello".to_string()])
        .expect("encode");
    assert_eq!(detailed.truncated, vec![false]);
    // <s> ▁hello </s> is three pieces.
    assert_eq!(detailed.token_counts, vec![3]);
}

#[test]
fn unigram_rejects_mismatched_and_missing_tokenizers() {
    let dir = std::env::temp_dir().join(format!("textintel-unigram-bad-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for file in ["config.json", "model.safetensors"] {
        std::fs::copy(format!("{FIXTURE}/{file}"), dir.join(file)).unwrap();
    }
    // Neither tokenizer layout present: explicit layout error.
    let error = TransformerEmbeddingProvider::open(&dir).unwrap_err();
    assert!(error.to_string().contains("no tokenizer"), "{error}");
    // BPE tokenizer.json present: explicit subset error.
    std::fs::write(
        dir.join("tokenizer.json"),
        r#"{"model": {"type": "BPE", "vocab": {}, "unk_id": 0}}"#,
    )
    .unwrap();
    let error = TransformerEmbeddingProvider::open(&dir).unwrap_err();
    assert!(error.to_string().contains("Unigram"), "{error}");
    // Oversized tokenizer (10 pieces, 9 declared rows): explicit mismatch
    // error, since ids would index out of bounds. The reverse — a
    // shorter tokenizer than the embedding rows, as in real
    // multilingual-e5-small — loads with the extra rows unused.
    std::fs::copy(
        format!("{FIXTURE}/tokenizer.json"),
        dir.join("tokenizer.json"),
    )
    .unwrap();
    let config = std::fs::read_to_string(dir.join("config.json")).unwrap();
    std::fs::write(
        dir.join("config.json"),
        config.replace("\"vocab_size\": 10", "\"vocab_size\": 9"),
    )
    .unwrap();
    let error = TransformerEmbeddingProvider::open(&dir).unwrap_err();
    assert!(error.to_string().contains("vocab_size"), "{error}");
    let _ = std::fs::remove_dir_all(&dir);
}
