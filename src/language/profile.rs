use crate::core::capabilities::{CapabilityLevel, ProviderCapabilities};
use crate::core::error::ProviderError;
use crate::core::providers::LanguageDetectionProvider;
use crate::core::types::LanguageCandidate;
use crate::resources::ResourceLoader;

/// Deterministic local language detector backed by versioned resource
/// profiles and Unicode script hints. It is a zero-network fallback for
/// applications that cannot ship a neural language model.
#[derive(Debug, Clone)]
pub struct ProfileLanguageDetector {
    resources: ResourceLoader,
    max_candidates: usize,
}

impl ProfileLanguageDetector {
    pub fn new(resources: ResourceLoader) -> Self {
        Self {
            resources,
            max_candidates: 8,
        }
    }

    pub fn with_max_candidates(mut self, max_candidates: usize) -> Self {
        self.max_candidates = max_candidates.max(1);
        self
    }

    pub fn resources(&self) -> &ResourceLoader {
        &self.resources
    }
}

impl LanguageDetectionProvider for ProfileLanguageDetector {
    fn detect(&self, text: &str) -> Result<Vec<LanguageCandidate>, ProviderError> {
        Ok(self
            .resources
            .detect_languages_with_limit(text, self.max_candidates))
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("profile_language_detector")
            .with_languages(self.resources.languages())
            .with_quality(CapabilityLevel::Basic)
    }
}
