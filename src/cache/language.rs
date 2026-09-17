//! Revision-aware cache for language detection.

use std::sync::Mutex;

use crate::core::error::ProviderError;
use crate::core::providers::LanguageDetectionProvider as LanguageDetectionTrait;
use crate::core::types::LanguageCandidate;

use super::core::{CacheDiagnostics, RevisionCache, lock_error};

/// Revision-aware language-detection cache keyed by
/// `(provider, revision, text)`.
pub struct CachedLanguageDetectionProvider<P> {
    inner: P,
    cache: Mutex<RevisionCache<(String, String, String), Vec<LanguageCandidate>>>,
}

impl<P> std::fmt::Debug for CachedLanguageDetectionProvider<P>
where
    P: std::fmt::Debug,
{
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CachedLanguageDetectionProvider")
            .field("inner", &self.inner)
            .field(
                "cache_size",
                &self.cache.lock().map(|cache| cache.len()).unwrap_or(0),
            )
            .finish()
    }
}

impl<P> CachedLanguageDetectionProvider<P>
where
    P: LanguageDetectionTrait,
{
    pub fn new(inner: P, max_entries: usize) -> Self {
        let capabilities = inner.capabilities();
        let revision = format!(
            "{}@{}",
            capabilities.provider,
            capabilities.model_revision.unwrap_or_default()
        );
        Self {
            inner,
            cache: Mutex::new(RevisionCache::new(revision, max_entries)),
        }
    }

    pub fn inner(&self) -> &P {
        &self.inner
    }

    pub fn invalidate(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.invalidate();
        }
    }

    pub fn len(&self) -> usize {
        self.cache.lock().map(|cache| cache.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn capacity(&self) -> usize {
        self.cache.lock().map(|cache| cache.capacity()).unwrap_or(0)
    }

    /// Observable state: counts only, never cached texts.
    pub fn diagnostics(&self) -> CacheDiagnostics {
        self.cache
            .lock()
            .map(|cache| cache.diagnostics())
            .unwrap_or_default()
    }

    fn key(&self, text: &str) -> (String, String, String) {
        let capabilities = self.inner.capabilities();
        (
            capabilities.provider,
            capabilities.model_revision.unwrap_or_default(),
            text.to_string(),
        )
    }
}

impl<P> LanguageDetectionTrait for CachedLanguageDetectionProvider<P>
where
    P: LanguageDetectionTrait,
{
    fn detect(&self, text: &str) -> Result<Vec<LanguageCandidate>, ProviderError> {
        let key = self.key(text);
        {
            let mut cache = self
                .cache
                .lock()
                .map_err(|_| lock_error("language_cache"))?;
            cache.set_revision(&format!("{}@{}", key.0, key.1));
            if let Some(hit) = cache.get(&key) {
                return Ok(hit);
            }
        }
        let candidates = self.inner.detect(text)?;
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| lock_error("language_cache"))?;
        cache.set_revision(&format!("{}@{}", key.0, key.1));
        cache.put(key, candidates.clone());
        Ok(candidates)
    }

    fn capabilities(&self) -> crate::core::capabilities::ProviderCapabilities {
        self.inner.capabilities()
    }

    fn cache_diagnostics(&self) -> Option<CacheDiagnostics> {
        Some(self.diagnostics())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_cache_is_keyed_by_text() {
        use crate::language::NgramLanguageDetector;
        use crate::resources::ResourceLoader;
        let resources = ResourceLoader::common().unwrap();
        let detector = NgramLanguageDetector::from_resources(&resources);
        let cached = CachedLanguageDetectionProvider::new(detector, 16);
        let first = cached.detect("hello world").unwrap();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached.detect("hello world").unwrap(), first);
        assert_eq!(cached.len(), 1);
    }
}
