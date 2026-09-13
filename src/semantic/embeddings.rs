use std::collections::BTreeMap;

use crate::core::error::ProviderError;
use crate::core::providers::EmbeddingProvider as EmbeddingProviderTrait;

pub use crate::core::providers::EmbeddingProvider;

#[derive(Debug, Default, Clone, Copy)]
pub struct NullEmbeddingProvider;

impl EmbeddingProviderTrait for NullEmbeddingProvider {
    fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        Ok(Vec::new())
    }
}

/// Useful for local tests and deterministic integrations.  It intentionally
/// maps exact strings; production applications can replace it with a model.
#[derive(Debug, Clone, Default)]
pub struct StaticEmbeddingProvider {
    values: BTreeMap<String, Vec<f32>>,
}

impl StaticEmbeddingProvider {
    pub fn new(values: BTreeMap<String, Vec<f32>>) -> Self { Self { values } }

    pub fn insert(&mut self, text: impl Into<String>, vector: Vec<f32>) {
        self.values.insert(text.into(), vector);
    }
}

impl EmbeddingProviderTrait for StaticEmbeddingProvider {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        Ok(texts.iter().filter_map(|text| self.values.get(text).cloned()).collect())
    }
}

