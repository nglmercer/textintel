//! Revision-aware caching: hits, explicit invalidation, revision-change
//! invalidation, and bounded eviction for embeddings, G2P, language
//! detection, and rebus decoding.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use textintel::core::capabilities::{CapabilityLevel, ProviderCapabilities};
use textintel::core::error::ProviderError;
use textintel::core::providers::{EmbeddingProvider, G2PProvider, LanguageDetectionProvider};
use textintel::semantic::embeddings::CachedEmbeddingProvider;
use textintel::{
    CachedG2PProvider, CachedLanguageDetectionProvider, CachedRebusDecoder, RevisionCache,
};
use textintel::{EngineConfig, TextIntelligence};

#[test]
fn revision_cache_basics() {
    let mut cache = RevisionCache::new("r1", 2);
    assert!(cache.is_empty());
    cache.put("a".to_string(), 1);
    cache.put("b".to_string(), 2);
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.get(&"a".to_string()), Some(1));
    // Bounded: inserting past capacity evicts deterministically.
    cache.put("c".to_string(), 3);
    assert_eq!(cache.len(), 2);
    // Same revision keeps entries; a new one clears them.
    assert!(!cache.set_revision("r1"));
    assert_eq!(cache.len(), 2);
    assert!(cache.set_revision("r2"));
    assert!(cache.is_empty());
    assert_eq!(cache.revision(), "r2");
    cache.put("a".to_string(), 1);
    cache.invalidate();
    assert!(cache.is_empty());
}

/// Embedding backend with a mutable revision and a call counter so tests can
/// observe recomputation.
struct RevisionedEmbedding {
    revision: Mutex<String>,
    calls: AtomicUsize,
    dimensions: usize,
}

impl RevisionedEmbedding {
    fn new(revision: &str, dimensions: usize) -> Self {
        Self {
            revision: Mutex::new(revision.to_string()),
            calls: AtomicUsize::new(0),
            dimensions,
        }
    }

    fn set_revision(&self, revision: &str) {
        *self.revision.lock().unwrap() = revision.to_string();
    }
}

impl EmbeddingProvider for RevisionedEmbedding {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(texts
            .iter()
            .map(|text| {
                let mut vector = vec![0.0; self.dimensions];
                vector[0] = text.len() as f32;
                vector
            })
            .collect())
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("revisioned_test_embedding").with_quality(CapabilityLevel::Basic)
    }

    fn model_metadata(&self) -> Option<textintel::core::capabilities::ModelMetadata> {
        Some(textintel::core::capabilities::ModelMetadata {
            model_id: "revisioned-test".to_string(),
            revision: Some(self.revision.lock().unwrap().clone()),
            dimensions: self.dimensions,
            normalized: false,
            languages: Vec::new(),
            source: None,
            license: None,
        })
    }
}

#[test]
fn embedding_cache_serves_hits_and_observes_revisions() {
    let inner = RevisionedEmbedding::new("r1", 4);
    let cached = CachedEmbeddingProvider::new(inner, 16);
    let texts = vec!["hello".to_string()];
    let first = cached.embed(&texts).unwrap();
    assert_eq!(cached.len(), 1);
    let second = cached.embed(&texts).unwrap();
    assert_eq!(first, second, "repeat call must hit");
    assert_eq!(cached.inner().calls.load(Ordering::SeqCst), 1);
    // A revision change invalidates before reading: the next call recomputes.
    cached.inner().set_revision("r2");
    let third = cached.embed(&texts).unwrap();
    assert_eq!(third, first, "vectors are content-identical here");
    assert_eq!(cached.inner().calls.load(Ordering::SeqCst), 2);
    assert_eq!(cached.len(), 1);
    cached.invalidate();
    assert!(cached.is_empty());
}

#[test]
fn embedding_cache_is_bounded() {
    let inner = RevisionedEmbedding::new("r1", 4);
    let cached = CachedEmbeddingProvider::new(inner, 2);
    for word in ["a", "b", "c"] {
        cached.embed(&[word.to_string()]).unwrap();
    }
    assert_eq!(cached.len(), 2);
}

#[test]
fn g2p_cache_serves_hits_and_invalidates() {
    let cached = CachedG2PProvider::new(textintel::phonetic::RuleBasedG2PProvider, 16);
    let first = cached.phonemize("hola", "es").unwrap();
    assert!(!cached.is_empty());
    assert_eq!(cached.phonemize("hola", "es").unwrap(), first);
    assert_eq!(cached.len(), 1);
    // Batch calls share the cache.
    let batch = cached
        .phonemize_batch(&["hola".to_string(), "adios".to_string()], "es")
        .unwrap();
    assert_eq!(batch[0], first);
    assert_eq!(cached.len(), 2);
    cached.invalidate();
    assert!(cached.is_empty());
}

#[test]
fn language_cache_keys_include_text() {
    use textintel::language::NgramLanguageDetector;
    use textintel::resources::ResourceLoader;
    let resources = ResourceLoader::common().unwrap();
    let cached =
        CachedLanguageDetectionProvider::new(NgramLanguageDetector::from_resources(&resources), 16);
    let first = cached.detect("hello world").unwrap();
    assert_eq!(cached.detect("hello world").unwrap(), first);
    assert_eq!(cached.len(), 1);
    cached.detect("bonjour monde").unwrap();
    assert_eq!(cached.len(), 2);
    cached.invalidate();
    assert!(cached.is_empty());
}

#[test]
fn rebus_cache_covers_languages_and_revisions() {
    use std::sync::Arc;
    use textintel::phonetic::RuleBasedG2PProvider;
    use textintel::rebus::RebusDecoder;
    use textintel::resources::DefaultLexiconProvider;
    use textintel::symbols::DefaultSymbolKnowledge;
    let decoder = CachedRebusDecoder::new(
        RebusDecoder::new(EngineConfig::default()),
        Arc::new(DefaultSymbolKnowledge),
        Arc::new(DefaultLexiconProvider),
        Arc::new(RuleBasedG2PProvider),
        None,
        "resources:v1",
        16,
    );
    let es = vec!["es".to_string()];
    let first = decoder.decode_cached("Fra🏠do", Some(&es), None);
    assert!(!first.is_empty());
    assert_eq!(decoder.decode_cached("Fra🏠do", Some(&es), None), first);
    assert_eq!(decoder.len(), 1);
    // Different languages are different keys.
    let en = vec!["en".to_string()];
    decoder.decode_cached("Fra🏠do", Some(&en), None);
    assert_eq!(decoder.len(), 2);
    // Resource revisions invalidate.
    assert_eq!(decoder.revision(), "resources:v1");
    assert!(decoder.set_revision("resources:v2"));
    assert!(decoder.is_empty());
    assert!(!decoder.set_revision("resources:v2"));
}

#[test]
fn caches_plug_into_the_engine() {
    // The cached providers implement the provider traits, so they compose
    // with `TextIntelligence` like any backend.
    let config = EngineConfig {
        phonetic: true,
        ..Default::default()
    };
    let engine = TextIntelligence::new(config).with_g2p_provider(CachedG2PProvider::new(
        textintel::phonetic::RuleBasedG2PProvider,
        64,
    ));
    let fingerprint = engine.analyze("hello").unwrap();
    assert!(!fingerprint.phonetic_candidates.is_empty());
}
