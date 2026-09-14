use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::core::capabilities::{CapabilityLevel, ModelMetadata, ProviderCapabilities};
use crate::core::error::ProviderError;
use crate::core::providers::EmbeddingProvider as EmbeddingProviderTrait;
use crate::lexical::ngrams::character_ngrams;
use crate::normalization::unicode::casefold_text;

pub use crate::core::providers::EmbeddingProvider;

#[derive(Debug, Default, Clone, Copy)]
pub struct NullEmbeddingProvider;

impl EmbeddingProviderTrait for NullEmbeddingProvider {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        Ok(vec![Vec::new(); texts.len()])
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("null_embedding")
            .with_dimensions(0)
            .with_quality(CapabilityLevel::Unavailable)
            .with_fallback("no embedding backend configured; semantic channel is skipped")
    }
}

/// Useful for local tests and deterministic integrations.  It intentionally
/// maps exact strings; production applications can replace it with a model.
#[derive(Debug, Clone, Default)]
pub struct StaticEmbeddingProvider {
    values: BTreeMap<String, Vec<f32>>,
}

impl StaticEmbeddingProvider {
    pub fn new(values: BTreeMap<String, Vec<f32>>) -> Self {
        Self { values }
    }

    pub fn insert(&mut self, text: impl Into<String>, vector: Vec<f32>) {
        self.values.insert(text.into(), vector);
    }
}

impl EmbeddingProviderTrait for StaticEmbeddingProvider {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        Ok(texts
            .iter()
            .map(|text| self.values.get(text).cloned().unwrap_or_default())
            .collect())
    }

    fn capabilities(&self) -> ProviderCapabilities {
        let dimensions = self
            .values
            .values()
            .find(|value| !value.is_empty())
            .map(Vec::len);
        let mut capabilities = ProviderCapabilities::new("static_embedding");
        if let Some(dimensions) = dimensions {
            capabilities = capabilities.with_dimensions(dimensions);
        }
        capabilities.with_quality(CapabilityLevel::Basic)
    }

    fn model_metadata(&self) -> Option<ModelMetadata> {
        let dimensions = self.values.values().find(|value| !value.is_empty())?.len();
        Some(ModelMetadata {
            model_id: "static-v1".to_string(),
            revision: None,
            dimensions,
            normalized: false,
            languages: Vec::new(),
            source: Some("application".to_string()),
            license: None,
        })
    }
}

/// Deterministic local vectorizer used when a full neural model is not
/// available. It is useful for privacy-preserving deployments and as a stable
/// retrieval baseline; applications can replace it with a trained model.
#[derive(Debug, Clone)]
pub struct FeatureHashEmbeddingProvider {
    dimensions: usize,
}

impl Default for FeatureHashEmbeddingProvider {
    fn default() -> Self {
        Self { dimensions: 256 }
    }
}

impl FeatureHashEmbeddingProvider {
    pub fn new(dimensions: usize) -> Result<Self, ProviderError> {
        if dimensions == 0 {
            return Err(ProviderError::new(
                "feature_hash_embedding",
                "dimensions must be positive",
            ));
        }
        Ok(Self { dimensions })
    }

    fn vectorize(&self, text: &str) -> Vec<f32> {
        let folded = casefold_text(text);
        let mut vector = vec![0.0f32; self.dimensions];
        for token in folded.split_whitespace().filter(|token| !token.is_empty()) {
            add_hashed_feature(&mut vector, token.as_bytes(), 1.0);
        }
        for gram in character_ngrams(&folded, 3) {
            add_hashed_feature(&mut vector, gram.as_bytes(), 0.25);
        }
        let norm = vector
            .iter()
            .map(|value| f64::from(*value) * f64::from(*value))
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

impl EmbeddingProviderTrait for FeatureHashEmbeddingProvider {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        Ok(texts.iter().map(|text| self.vectorize(text)).collect())
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("feature_hash_embedding")
            .with_dimensions(self.dimensions)
            .with_quality(CapabilityLevel::Basic)
            .with_fallback("feature hashes are a fallback, not contextual multilingual embeddings")
    }

    fn model_metadata(&self) -> Option<ModelMetadata> {
        Some(ModelMetadata {
            model_id: "feature-hash-v1".to_string(),
            revision: Some("stable".to_string()),
            dimensions: self.dimensions,
            normalized: true,
            languages: vec!["multilingual".to_string()],
            source: Some("local deterministic feature encoder".to_string()),
            license: Some("MIT".to_string()),
        })
    }
}

fn add_hashed_feature(vector: &mut [f32], feature: &[u8], weight: f32) {
    let hash = stable_hash(feature);
    let index = (hash as usize) % vector.len();
    let sign = if hash & 1 == 0 { 1.0 } else { -1.0 };
    vector[index] += sign * weight;
}

fn stable_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Bounded, deterministic embedding cache keyed by model identity and folded
/// text. Missing values are fetched in one batch from the wrapped provider.
pub struct CachedEmbeddingProvider<P> {
    inner: P,
    max_entries: usize,
    cache: Mutex<BTreeMap<(String, String), Vec<f32>>>,
}

impl<P> std::fmt::Debug for CachedEmbeddingProvider<P>
where
    P: std::fmt::Debug,
{
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CachedEmbeddingProvider")
            .field("inner", &self.inner)
            .field("max_entries", &self.max_entries)
            .field(
                "cache_size",
                &self.cache.lock().map(|cache| cache.len()).unwrap_or(0),
            )
            .finish()
    }
}

impl<P> CachedEmbeddingProvider<P>
where
    P: EmbeddingProviderTrait,
{
    pub fn new(inner: P, max_entries: usize) -> Self {
        Self {
            inner,
            max_entries: max_entries.max(1),
            cache: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn inner(&self) -> &P {
        &self.inner
    }
}

impl<P> EmbeddingProviderTrait for CachedEmbeddingProvider<P>
where
    P: EmbeddingProviderTrait,
{
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        let model = self
            .inner
            .model_metadata()
            .map(|metadata| metadata.model_id)
            .unwrap_or_else(|| self.inner.capabilities().provider);
        let keys = texts
            .iter()
            .map(|text| (model.clone(), casefold_text(text)))
            .collect::<Vec<_>>();
        let mut output = vec![None; texts.len()];
        let mut missing = Vec::new();
        let mut missing_positions = Vec::new();
        {
            let cache = self
                .cache
                .lock()
                .map_err(|_| ProviderError::new("embedding_cache", "cache lock poisoned"))?;
            for (index, key) in keys.iter().enumerate() {
                if let Some(vector) = cache.get(key) {
                    output[index] = Some(vector.clone());
                } else {
                    missing.push(texts[index].clone());
                    missing_positions.push(index);
                }
            }
        }
        if !missing.is_empty() {
            let values = self.inner.embed_batch(&missing)?;
            if values.len() != missing.len() {
                return Err(ProviderError::new(
                    "embedding_cache",
                    format!(
                        "wrapped provider returned {} vectors for {} inputs",
                        values.len(),
                        missing.len()
                    ),
                ));
            }
            let mut cache = self
                .cache
                .lock()
                .map_err(|_| ProviderError::new("embedding_cache", "cache lock poisoned"))?;
            for (position, vector) in missing_positions.into_iter().zip(values) {
                cache.insert(keys[position].clone(), vector.clone());
                output[position] = Some(vector);
            }
            while cache.len() > self.max_entries {
                if let Some(key) = cache.keys().next().cloned() {
                    cache.remove(&key);
                }
            }
        }
        Ok(output.into_iter().map(Option::unwrap_or_default).collect())
    }

    fn capabilities(&self) -> ProviderCapabilities {
        self.inner.capabilities()
    }

    fn model_metadata(&self) -> Option<ModelMetadata> {
        self.inner.model_metadata()
    }

    fn health_check(&self) -> Result<(), ProviderError> {
        self.inner.health_check()
    }
}
