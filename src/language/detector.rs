use crate::core::error::ProviderError;
use crate::core::providers::LanguageDetectionProvider;
use crate::core::types::LanguageCandidate;
use crate::resources::embedded_common;

/// Detect languages with the embedded resource index. Applications that load
/// their own packs can inject `ResourceLoader` as a provider.
pub fn detect_languages(text: &str) -> Vec<LanguageCandidate> {
    embedded_common().detect_languages(text)
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultLanguageDetector;

impl LanguageDetectionProvider for DefaultLanguageDetector {
    fn detect(&self, text: &str) -> Result<Vec<LanguageCandidate>, ProviderError> {
        Ok(detect_languages(text))
    }
}
