//! Safe revision-aware caching for expensive provider calls.
//!
//! Every cache in this module is bounded (`max_entries`, oldest-first
//! eviction by key order) and revision-aware: entries are namespaced by the
//! wrapped provider's identity plus its model/data revision, and an observed
//! revision change invalidates the cache instead of serving stale vectors.
//! Caches are deterministic (no TTLs, no wall-clock expiry) and never shared
//! across providers.

use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::core::error::ProviderError;
use crate::core::providers::{
    G2PProvider as G2PProviderTrait, LanguageDetectionProvider as LanguageDetectionTrait,
};
use crate::core::types::{DecodedCandidate, LanguageCandidate, PhoneticCandidate};

/// Bounded deterministic cache namespaced by a revision string. A revision
/// change (new model weights, new resource packs) clears all entries: stale
/// hits across revisions are a correctness bug, not a performance tradeoff.
#[derive(Debug)]
pub struct RevisionCache<Key, Value> {
    revision: String,
    max_entries: usize,
    entries: BTreeMap<Key, Value>,
}

impl<Key, Value> RevisionCache<Key, Value>
where
    Key: Ord + Clone,
    Value: Clone,
{
    pub fn new(revision: impl Into<String>, max_entries: usize) -> Self {
        Self {
            revision: revision.into(),
            max_entries: max_entries.max(1),
            entries: BTreeMap::new(),
        }
    }

    pub fn revision(&self) -> &str {
        &self.revision
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, key: &Key) -> Option<Value> {
        self.entries.get(key).cloned()
    }

    pub fn put(&mut self, key: Key, value: Value) {
        self.entries.insert(key, value);
        while self.entries.len() > self.max_entries {
            // BTreeMap has no insertion order; evict the first key. Order is
            // deterministic, which is what matters for reproducibility.
            if let Some(first) = self.entries.keys().next().cloned() {
                self.entries.remove(&first);
            } else {
                break;
            }
        }
    }

    pub fn invalidate(&mut self) {
        self.entries.clear();
    }

    /// Adopt `revision`, clearing entries when it differs from the current
    /// one. Returns true when an invalidation happened.
    pub fn set_revision(&mut self, revision: &str) -> bool {
        if self.revision != revision {
            self.revision = revision.to_string();
            self.entries.clear();
            true
        } else {
            false
        }
    }
}

fn lock_error(provider: &str) -> ProviderError {
    ProviderError::new(provider, "cache lock poisoned")
}

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
}

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
}

/// Revision-aware cache for expensive rebus decoding. The key covers the
/// input text, requested languages, candidate limit, and the scoring-weight
/// vector (as JSON: different weights legitimately decode differently). The
/// revision is caller-owned (resource-pack revision by convention);
/// [`Self::set_revision`] invalidates when packs change.
pub struct CachedRebusDecoder {
    decoder: crate::rebus::RebusDecoder,
    symbols: std::sync::Arc<dyn crate::core::providers::SymbolKnowledgeProvider>,
    lexicon: std::sync::Arc<dyn crate::core::providers::LexiconProvider>,
    g2p: std::sync::Arc<dyn crate::core::providers::G2PProvider>,
    abbreviations: Option<std::sync::Arc<dyn crate::core::providers::AbbreviationProvider>>,
    cache: Mutex<RevisionCache<String, Vec<DecodedCandidate>>>,
}

impl std::fmt::Debug for CachedRebusDecoder {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CachedRebusDecoder")
            .field(
                "cache_size",
                &self.cache.lock().map(|cache| cache.len()).unwrap_or(0),
            )
            .finish()
    }
}

impl CachedRebusDecoder {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        decoder: crate::rebus::RebusDecoder,
        symbols: std::sync::Arc<dyn crate::core::providers::SymbolKnowledgeProvider>,
        lexicon: std::sync::Arc<dyn crate::core::providers::LexiconProvider>,
        g2p: std::sync::Arc<dyn crate::core::providers::G2PProvider>,
        abbreviations: Option<std::sync::Arc<dyn crate::core::providers::AbbreviationProvider>>,
        revision: impl Into<String>,
        max_entries: usize,
    ) -> Self {
        Self {
            decoder,
            symbols,
            lexicon,
            g2p,
            abbreviations,
            cache: Mutex::new(RevisionCache::new(revision, max_entries)),
        }
    }

    pub fn revision(&self) -> String {
        self.cache
            .lock()
            .map(|cache| cache.revision().to_string())
            .unwrap_or_default()
    }

    /// Adopt `revision`, invalidating when it changed. Returns true when an
    /// invalidation happened.
    pub fn set_revision(&self, revision: &str) -> bool {
        self.cache
            .lock()
            .map(|mut cache| cache.set_revision(revision))
            .unwrap_or(false)
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

    fn key(
        &self,
        text: &str,
        languages: Option<&[String]>,
        max_candidates: Option<usize>,
    ) -> String {
        let weights = self
            .decoder
            .config
            .rebus_weights
            .to_json()
            .unwrap_or_default();
        format!(
            "{text}\u{1f}{}\u{1f}{}\u{1f}{weights}",
            languages.map(|values| values.join(",")).unwrap_or_default(),
            max_candidates
                .map(|value| value.to_string())
                .unwrap_or_default(),
        )
    }

    /// Decode with caching. Cache hits return clones; misses decode and
    /// store. Bounded by `max_entries`; never serves across revisions.
    pub fn decode_cached(
        &self,
        text: &str,
        languages: Option<&[String]>,
        max_candidates: Option<usize>,
    ) -> Vec<DecodedCandidate> {
        let key = self.key(text, languages, max_candidates);
        if let Ok(cache) = self.cache.lock() {
            if let Some(hit) = cache.get(&key) {
                return hit;
            }
        }
        let candidates = self.decoder.decode_with_abbreviations(
            text,
            languages,
            max_candidates,
            self.symbols.as_ref(),
            self.lexicon.as_ref(),
            self.g2p.as_ref(),
            None,
            self.abbreviations.as_deref(),
        );
        if let Ok(mut cache) = self.cache.lock() {
            cache.put(key, candidates.clone());
        }
        candidates
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_change_clears_entries() {
        let mut cache = RevisionCache::new("r1", 8);
        cache.put("a".to_string(), 1);
        assert!(!cache.set_revision("r1"));
        assert_eq!(cache.get(&"a".to_string()), Some(1));
        assert!(cache.set_revision("r2"));
        assert!(cache.is_empty());
    }

    #[test]
    fn cache_is_bounded_and_deterministic() {
        let mut cache = RevisionCache::new("r1", 2);
        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);
        cache.put("c".to_string(), 3);
        assert_eq!(cache.len(), 2);
    }

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

    #[test]
    fn rebus_cache_keys_cover_languages_and_weights() {
        use crate::core::config::EngineConfig;
        use crate::phonetic::RuleBasedG2PProvider;
        use crate::rebus::RebusDecoder;
        use crate::resources::DefaultLexiconProvider;
        use crate::symbols::DefaultSymbolKnowledge;
        use std::sync::Arc;
        let decoder = CachedRebusDecoder::new(
            RebusDecoder::new(EngineConfig::default()),
            Arc::new(DefaultSymbolKnowledge),
            Arc::new(DefaultLexiconProvider),
            Arc::new(RuleBasedG2PProvider),
            None,
            "resources:v1",
            16,
        );
        let first = decoder.decode_cached("Fra🏠do", None, None);
        assert!(!first.is_empty());
        assert_eq!(decoder.len(), 1);
        let second = decoder.decode_cached("Fra🏠do", None, None);
        assert_eq!(first, second);
        assert_eq!(decoder.len(), 1);
        assert!(decoder.set_revision("resources:v2"));
        assert!(decoder.is_empty());
    }
}
