//! Local transformer sentence embeddings (BERT family, offline CPU).
//!
//! ```sh
//! cargo run --features semantic-transformer --example transformer_embeddings
//! ```
//! Point `TransformerEmbeddingProvider::open` at any BERT-wiring WordPiece
//! checkpoint directory (`config.json`, `vocab.txt`, `model.safetensors`).
//! The demo below uses the tiny mechanics fixture; quality evaluation needs
//! a real checkpoint (paraphrase-multilingual-MiniLM, LaBSE, ...).
#![cfg(feature = "semantic-transformer")]

use textintel::core::providers::EmbeddingProvider;
use textintel::semantic::similarity::cosine;
use textintel::{TransformerEmbeddingProvider, TransformerPooling};

fn main() -> Result<(), textintel::TextIntelError> {
    let provider = TransformerEmbeddingProvider::open("tests/fixtures/mini-transformer")
        .map_err(|error| textintel::TextIntelError::InvalidConfiguration(error.to_string()))?;
    println!(
        "model: {} ({} dims, {} tokens max)",
        provider.model_id(),
        provider.dimensions(),
        provider.max_token_length()
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
    Ok(())
}
