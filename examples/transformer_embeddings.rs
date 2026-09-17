//! Local transformer sentence embeddings (modern BERT-wiring family, offline CPU).
//!
//! ```sh
//! cargo run --example transformer_embeddings
//! ```
//! Point `TransformerEmbeddingProvider::open` at a curated checkpoint
//! directory (`config.json`, `tokenizer.json` or `vocab.txt`,
//! `model.safetensors`): multilingual-e5-small (multilingual default),
//! snowflake-arctic-embed-xs or mxbai-embed-xsmall-v1 (English CPU).
//! The demo below uses the tiny mechanics fixtures; quality evaluation
//! needs a real checkpoint (see `models/README.md`).
#![cfg(feature = "semantic-transformer")]

use textintel::core::providers::EmbeddingProvider;
use textintel::semantic::similarity::cosine;
use textintel::{TransformerEmbeddingProvider, TransformerPooling};

fn main() -> Result<(), textintel::TextIntelError> {
    let provider = TransformerEmbeddingProvider::open("tests/fixtures/mini-transformer")
        .map_err(|error| textintel::TextIntelError::InvalidConfiguration(error.to_string()))?;
    println!(
        "model: {} ({} dims, {} tokens max, {} tokenizer)",
        provider.model_id(),
        provider.dimensions(),
        provider.max_token_length(),
        provider.tokenizer_kind(),
    );
    let texts = vec![
        "buy the ticket now".to_string(),
        "purchase your ticket today".to_string(),
    ];
    let vectors = provider
        .embed(&texts)
        .map_err(textintel::TextIntelError::from)?;
    println!("cosine = {:.4}", cosine(&vectors[0], &vectors[1]));
    let detailed = provider
        .encode_detailed(&["word ".repeat(500)])
        .map_err(textintel::TextIntelError::from)?;
    println!(
        "truncated: {:?}, tokens: {:?}",
        detailed.truncated, detailed.token_counts
    );
    let mean = TransformerEmbeddingProvider::open("tests/fixtures/mini-transformer")
        .map_err(|error| textintel::TextIntelError::InvalidConfiguration(error.to_string()))?
        .with_pooling(TransformerPooling::Mean);
    let mean_vectors = mean
        .embed(&texts)
        .map_err(textintel::TextIntelError::from)?;
    println!(
        "mean-pooled cosine = {:.4}",
        cosine(&mean_vectors[0], &mean_vectors[1])
    );
    // Modern e5-style layout: Unigram tokenizer.json, mean pooling, and the
    // model-card query prefix.
    let modern = TransformerEmbeddingProvider::open("tests/fixtures/mini-unigram")
        .map_err(|error| textintel::TextIntelError::InvalidConfiguration(error.to_string()))?
        .with_pooling(TransformerPooling::Mean)
        .with_text_prefix("query: ");
    println!("modern fixture tokenizer: {}", modern.tokenizer_kind());
    let modern_vectors = modern
        .embed(&texts)
        .map_err(textintel::TextIntelError::from)?;
    println!(
        "modern-layout cosine = {:.4}",
        cosine(&modern_vectors[0], &modern_vectors[1])
    );
    Ok(())
}
