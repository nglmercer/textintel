use serde::{Deserialize, Serialize};

/// Stable description of what a provider can do. Providers return this
/// metadata without performing network or model work.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderCapabilities {
    pub provider: String,
    pub version: Option<String>,
    pub local: bool,
    pub batch: bool,
    pub languages: Vec<String>,
    pub dimensions: Option<usize>,
}

impl ProviderCapabilities {
    pub fn new(provider: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            version: None,
            local: true,
            batch: true,
            languages: Vec::new(),
            dimensions: None,
        }
    }

    pub fn with_languages<I, S>(mut self, languages: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.languages = languages.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_dimensions(mut self, dimensions: usize) -> Self {
        self.dimensions = Some(dimensions);
        self
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    pub fn remote(mut self) -> Self {
        self.local = false;
        self
    }
}

/// Metadata that makes embedding vectors reproducible and safely comparable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelMetadata {
    pub model_id: String,
    pub revision: Option<String>,
    pub dimensions: usize,
    pub normalized: bool,
    pub languages: Vec<String>,
    pub source: Option<String>,
    pub license: Option<String>,
}

impl ModelMetadata {
    pub fn validate_vector(&self, vector: &[f32]) -> Result<(), String> {
        if vector.len() != self.dimensions {
            return Err(format!(
                "model {} returned dimension {}, expected {}",
                self.model_id,
                vector.len(),
                self.dimensions
            ));
        }
        if vector.iter().any(|value| !value.is_finite()) {
            return Err(format!(
                "model {} returned a non-finite vector",
                self.model_id
            ));
        }
        Ok(())
    }
}
