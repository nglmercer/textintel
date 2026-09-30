//! Explicit local encoder selection for training and serving decisions.
use std::sync::Arc;

use crate::core::capabilities::{CapabilityLevel, ModelMetadata, ProviderCapabilities};
use crate::core::error::ProviderError;
use crate::core::providers::EmbeddingProvider;
use crate::normalization::unicode::casefold_text;

/// Signed word unigram/bigram encoder. This is a lexical baseline, not a
/// pretrained semantic model. Its identity differs from feature-hash-v1.
pub struct WordHashEmbeddingProvider {
    dimensions: usize,
}

impl WordHashEmbeddingProvider {
    pub fn new(dimensions: usize) -> Result<Self, String> {
        if !(1..=4096).contains(&dimensions) {
            return Err("word hash dimensions must be within 1..=4096".to_string());
        }
        Ok(Self { dimensions })
    }

    fn vectorize(&self, text: &str) -> Vec<f32> {
        let folded = casefold_text(text);
        let words: Vec<&str> = folded
            .split(|ch: char| !ch.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .collect();
        let mut vector = vec![0.0f32; self.dimensions];
        let mut add = |bytes: &[u8], weight: f32| {
            let mut hash = 0xcbf29ce484222325u64;
            for byte in bytes {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
            // Independent sign bit: low-bit signs would be fixed per bucket
            // for power-of-two dimensions, defeating signed collision mixing.
            vector[hash as usize % self.dimensions] +=
                if hash >> 63 == 0 { weight } else { -weight };
        };
        for word in &words {
            add(word.as_bytes(), 1.0);
        }
        for pair in words.windows(2) {
            add(format!("{}\0{}", pair[0], pair[1]).as_bytes(), 0.5);
        }
        let norm = vector
            .iter()
            .map(|v| f64::from(*v).powi(2))
            .sum::<f64>()
            .sqrt() as f32;
        if norm > 0.0 {
            for value in &mut vector {
                *value /= norm;
            }
        }
        vector
    }
}

impl EmbeddingProvider for WordHashEmbeddingProvider {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        Ok(texts.iter().map(|text| self.vectorize(text)).collect())
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("word_hash_embedding")
            .with_dimensions(self.dimensions)
            .with_quality(CapabilityLevel::Basic)
            .with_fallback("lexical hashes have no pretrained contextual semantics")
    }

    fn model_metadata(&self) -> Option<ModelMetadata> {
        Some(ModelMetadata {
            model_id: "word-hash-v1".to_string(),
            revision: Some("stable".to_string()),
            dimensions: self.dimensions,
            normalized: true,
            languages: Vec::new(),
            source: Some("local signed word unigram/bigram encoder".to_string()),
            license: Some("MIT".to_string()),
        })
    }
}

/// Load an explicitly selected local backbone. `hash:N` and `wordhash:N`
/// require no model downloads; other values denote transformer directories.
pub fn open_decision_embeddings(source: &str) -> Result<Arc<dyn EmbeddingProvider>, String> {
    if let Some(dimensions) = source.strip_prefix("hash:") {
        let dimensions: usize = dimensions.parse().map_err(|_| "invalid hash dimensions")?;
        if !(1..=4096).contains(&dimensions) {
            return Err("hash dimensions must be within 1..=4096".to_string());
        }
        return Ok(Arc::new(
            crate::semantic::FeatureHashEmbeddingProvider::new(dimensions)
                .map_err(|e| e.to_string())?,
        ));
    }
    if let Some(dimensions) = source.strip_prefix("wordhash:") {
        return Ok(Arc::new(WordHashEmbeddingProvider::new(
            dimensions
                .parse()
                .map_err(|_| "invalid word hash dimensions")?,
        )?));
    }
    #[cfg(feature = "semantic-transformer")]
    {
        Ok(Arc::new(
            crate::semantic::TransformerEmbeddingProvider::open(source)
                .map_err(|e| e.to_string())?,
        ))
    }
    #[cfg(not(feature = "semantic-transformer"))]
    {
        Err("transformer directories require semantic-transformer; use hash:N or wordhash:N for offline training".to_string())
    }
}
