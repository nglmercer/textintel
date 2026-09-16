//! Revision-aware cache for G2P phonemization.

use std::sync::Mutex;

use crate::core::error::ProviderError;
use crate::core::providers::G2PProvider as G2PProviderTrait;
use crate::core::types::PhoneticCandidate;

use super::core::{lock_error, CacheDiagnostics, RevisionCache};

/// Revision-aware G2P cache. Keys are `(provider, revision, language, text)`;
/// a provider that reports a new `model_revision` invalidates prior entries.
pub struct CachedG2PProvider<P> {
    inner: P,
    cache: Mutex<RevisionCache<(String, String, String, String), PhoneticCandidate>>,
}

impl<P> std::fmt::Debug for CachedG2PProvider<P>
where
    P: std::fmt::Debug,
{
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CachedG2PProvider")
            .field("inner", &self.inner)
            .field(
                "cache_size",
                &self.cache.lock().map(|cache| cache.len()).unwrap_or(0),
            )
            .finish()
    }
}

impl<P> CachedG2PProvider<P>
where
    P: G2PProviderTrait,
{
    pub fn new(inner: P, max_entries: usize) -> Self {
        let revision = current_g2p_revision(&inner);
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

    fn key(&self, text: &str, language: &str) -> (String, String, String, String) {
        let capabilities = self.inner.capabilities();
        (
            capabilities.provider,
            capabilities.model_revision.unwrap_or_default(),
            language.to_string(),
            text.to_string(),
        )
    }
}

fn current_g2p_revision<P: G2PProviderTrait>(inner: &P) -> String {
    let capabilities = inner.capabilities();
    format!(
        "{}@{}",
        capabilities.provider,
        capabilities.model_revision.unwrap_or_default()
    )
}

impl<P> G2PProviderTrait for CachedG2PProvider<P>
where
    P: G2PProviderTrait,
{
    fn phonemize(&self, text: &str, language: &str) -> Result<PhoneticCandidate, ProviderError> {
        let key = self.key(text, language);
        {
            let mut cache = self.cache.lock().map_err(|_| lock_error("g2p_cache"))?;
            cache.set_revision(&format!("{}@{}", key.0, key.1));
            if let Some(hit) = cache.get(&key) {
                return Ok(hit);
            }
        }
        let candidate = self.inner.phonemize(text, language)?;
        let mut cache = self.cache.lock().map_err(|_| lock_error("g2p_cache"))?;
        cache.set_revision(&format!("{}@{}", key.0, key.1));
        cache.put(key, candidate.clone());
        Ok(candidate)
    }

    fn phonemize_batch(
        &self,
        texts: &[String],
        language: &str,
    ) -> Result<Vec<PhoneticCandidate>, ProviderError> {
        let keys = texts
            .iter()
            .map(|text| self.key(text, language))
            .collect::<Vec<_>>();
        let mut output: Vec<Option<PhoneticCandidate>> = vec![None; texts.len()];
        let mut missing = Vec::new();
        let mut missing_positions = Vec::new();
        {
            let mut cache = self.cache.lock().map_err(|_| lock_error("g2p_cache"))?;
            if let Some(first) = keys.first() {
                cache.set_revision(&format!("{}@{}", first.0, first.1));
            }
            for (index, key) in keys.iter().enumerate() {
                if let Some(hit) = cache.get(key) {
                    output[index] = Some(hit);
                } else {
                    missing.push(texts[index].clone());
                    missing_positions.push(index);
                }
            }
        }
        if !missing.is_empty() {
            let values = self.inner.phonemize_batch(&missing, language)?;
            if values.len() != missing.len() {
                return Err(ProviderError::new(
                    "g2p_cache",
                    format!(
                        "wrapped provider returned {} candidates for {} inputs",
                        values.len(),
                        missing.len()
                    ),
                ));
            }
            let mut cache = self.cache.lock().map_err(|_| lock_error("g2p_cache"))?;
            for (position, candidate) in missing_positions.into_iter().zip(values) {
                cache.put(keys[position].clone(), candidate.clone());
                output[position] = Some(candidate);
            }
        }
        output
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| ProviderError::new("g2p_cache", "cache fill left a gap"))
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
    fn g2p_cache_serves_and_invalidates() {
        use crate::phonetic::RuleBasedG2PProvider;
        let cached = CachedG2PProvider::new(RuleBasedG2PProvider, 16);
        let first = cached.phonemize("hola", "es").unwrap();
        assert_eq!(cached.len(), 1);
        let second = cached.phonemize("hola", "es").unwrap();
        assert_eq!(first, second);
        cached.invalidate();
        assert!(cached.is_empty());
    }
}
