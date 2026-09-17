//! Revision-aware cache for expensive rebus decoding.

use std::sync::Mutex;

use crate::core::types::DecodedCandidate;

use super::core::{CacheDiagnostics, RevisionCache};

/// Revision-cache key for one rebus decoding: input text, requested
/// languages, candidate limit, scoring weights, and the semantic-rescoring
/// marker (`sem:off` when rescoring is disabled, otherwise the embedding
/// model identity). Different weights or models legitimately decode
/// differently, so all of them key the cache.
pub fn rebus_cache_key(
    text: &str,
    languages: Option<&[String]>,
    max_candidates: Option<usize>,
    weights: &crate::core::config::RebusWeights,
    semantic: &str,
) -> String {
    let weights = weights.to_json().unwrap_or_default();
    format!(
        "{text}\u{1f}{}\u{1f}{}\u{1f}{weights}\u{1f}{semantic}",
        languages.map(|values| values.join(",")).unwrap_or_default(),
        max_candidates
            .map(|value| value.to_string())
            .unwrap_or_default(),
    )
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

    fn key(
        &self,
        text: &str,
        languages: Option<&[String]>,
        max_candidates: Option<usize>,
    ) -> String {
        // This decoder never uses semantic rescoring, so the semantic marker
        // is fixed; see [`rebus_cache_key`].
        rebus_cache_key(
            text,
            languages,
            max_candidates,
            &self.decoder.config.rebus_weights,
            "sem:off",
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
        if let Ok(mut cache) = self.cache.lock()
            && let Some(hit) = cache.get(&key)
        {
            return hit;
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
