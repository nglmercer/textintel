//! Explicit HTTP embedding adapter.
//!
//! This module is feature-gated and never constructed by `TextIntelligence`.
//! Applications must opt in and inject it, so analyzing a message can never
//! accidentally exfiltrate input to a remote service.

use std::time::Duration;

use serde::Deserialize;

use crate::core::capabilities::{CapabilityLevel, ModelMetadata, ProviderCapabilities};
use crate::core::error::ProviderError;
use crate::core::providers::EmbeddingProvider;

#[derive(Debug, Clone)]
pub struct HttpEmbeddingProvider {
    endpoint: String,
    model: String,
    api_key: Option<String>,
    max_batch_size: usize,
    agent: ureq::Agent,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    #[serde(default)]
    data: Vec<EmbeddingItem>,
    #[serde(default)]
    embeddings: Vec<Vec<f32>>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingItem {
    embedding: Vec<f32>,
}

impl HttpEmbeddingProvider {
    pub fn new(
        endpoint: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, ProviderError> {
        let endpoint = endpoint.into();
        let model = model.into();
        if !(endpoint.starts_with("https://") || endpoint.starts_with("http://")) {
            return Err(ProviderError::new(
                "http_embedding",
                "endpoint must use http:// or https://",
            ));
        }
        if model.trim().is_empty() {
            return Err(ProviderError::new(
                "http_embedding",
                "model cannot be empty",
            ));
        }
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(Duration::from_secs(60))
            .timeout_write(Duration::from_secs(60))
            .build();
        Ok(Self {
            endpoint,
            model,
            api_key: None,
            max_batch_size: 64,
            agent,
        })
    }

    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    pub fn with_max_batch_size(mut self, max_batch_size: usize) -> Self {
        self.max_batch_size = max_batch_size.max(1);
        self
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    fn request(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        let mut request = self.agent.post(&self.endpoint);
        if let Some(api_key) = &self.api_key {
            request = request.set("Authorization", &format!("Bearer {api_key}"));
        }
        let response = request
            .send_json(ureq::json!({"model": self.model, "input": texts}))
            .map_err(|error| ProviderError::new("http_embedding", error.to_string()))?;
        let payload: EmbeddingResponse = response
            .into_json()
            .map_err(|error| ProviderError::new("http_embedding", error.to_string()))?;
        let mut values = payload
            .data
            .into_iter()
            .map(|item| item.embedding)
            .collect::<Vec<_>>();
        if values.is_empty() {
            values = payload.embeddings;
        }
        if values.len() != texts.len() {
            return Err(ProviderError::new(
                "http_embedding",
                format!(
                    "response returned {} vectors for {} inputs",
                    values.len(),
                    texts.len()
                ),
            ));
        }
        Ok(values)
    }
}

impl EmbeddingProvider for HttpEmbeddingProvider {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        if texts.len() > self.max_batch_size {
            return Err(ProviderError::new(
                "http_embedding",
                format!(
                    "batch size {} exceeds max_batch_size={}",
                    texts.len(),
                    self.max_batch_size
                ),
            ));
        }
        if texts.iter().any(|text| text.len() > 1_000_000) {
            return Err(ProviderError::new(
                "http_embedding",
                "input text exceeds remote adapter limit",
            ));
        }
        self.request(texts)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("http_embedding")
            .remote()
            .with_version("openai-compatible-v1")
            .with_quality(CapabilityLevel::Production)
    }

    fn model_metadata(&self) -> Option<ModelMetadata> {
        None
    }

    fn health_check(&self) -> Result<(), ProviderError> {
        // Health checks are explicit and therefore may perform a request.
        self.request(&["health check".to_string()]).map(|_| ())
    }
}
