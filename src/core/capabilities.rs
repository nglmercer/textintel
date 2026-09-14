use serde::{Deserialize, Serialize};

/// Quality tier of a provider implementation. Static heuristics and
/// feature hashes are `Basic`; model-backed local backends such as espeak-ng
/// or transformer embeddings are `Production`. Anything that cannot serve
/// (null backends, missing binaries) reports `Unavailable` instead of
/// pretending to work.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityLevel {
    /// The provider cannot serve (null backend, missing binary/model).
    #[default]
    Unavailable,
    /// Deterministic heuristic fallback with documented limits.
    Basic,
    /// Model-backed backend suitable for production use.
    Production,
}

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
    /// Quality tier of this implementation (see [`CapabilityLevel`]).
    #[serde(default)]
    pub quality: CapabilityLevel,
    /// Model or data revision backing these capabilities, when any.
    #[serde(default)]
    pub model_revision: Option<String>,
    /// Human-readable fallback note when serving degraded output
    /// (for example which production backend was unavailable).
    #[serde(default)]
    pub fallback: Option<String>,
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
            quality: CapabilityLevel::default(),
            model_revision: None,
            fallback: None,
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

    pub fn with_quality(mut self, quality: CapabilityLevel) -> Self {
        self.quality = quality;
        self
    }

    pub fn with_model_revision(mut self, revision: impl Into<String>) -> Self {
        self.model_revision = Some(revision.into());
        self
    }

    pub fn with_fallback(mut self, note: impl Into<String>) -> Self {
        self.fallback = Some(note.into());
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
