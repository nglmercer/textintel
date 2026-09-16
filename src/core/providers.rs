use std::sync::Arc;

use super::capabilities::{CapabilityLevel, ModelMetadata, ProviderCapabilities};
use super::error::ProviderError;
use super::types::{
    ComparisonResult, LanguageCandidate, MessageFingerprint, PatternMatch, PhoneticCandidate,
    SearchCandidateSet, SpamResult, SymbolConcept, SymbolReading,
};
use crate::cache::CacheDiagnostics;

/// Provider boundary for multilingual embeddings.  Providers may be local or
/// remote; the core never chooses a vendor or sends data implicitly.
pub trait EmbeddingProvider: Send + Sync {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError>;

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        self.embed(texts)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("embedding")
    }

    fn model_metadata(&self) -> Option<ModelMetadata> {
        None
    }

    fn health_check(&self) -> Result<(), ProviderError> {
        Ok(())
    }

    /// Cache state when this provider serves through a revision-aware cache,
    /// `None` when uncached. Counts only — never cached texts.
    fn cache_diagnostics(&self) -> Option<CacheDiagnostics> {
        None
    }
}

/// One script conversion of a message, preserved as an additional
/// fingerprint view. Transliteration never replaces [`MessageFingerprint::raw`](super::types::MessageFingerprint).
#[derive(Debug, Clone, PartialEq)]
pub struct Transliteration {
    /// Converted text.
    pub text: String,
    /// ISO 15924 script of `text` (`Latn`, `Cyrl`, `Arab`, `Hans`).
    pub target_script: String,
    /// Most likely language of the conversion, when the script implies one.
    pub language: Option<String>,
    /// Provider confidence in `[0.0, 1.0]`.
    pub confidence: f64,
}

impl Transliteration {
    pub fn new(
        text: impl Into<String>,
        target_script: impl Into<String>,
        language: Option<String>,
        confidence: f64,
    ) -> Self {
        Self {
            text: text.into(),
            target_script: target_script.into(),
            language,
            confidence: confidence.clamp(0.0, 1.0),
        }
    }
}

/// Provider boundary for bounded entity extraction. Providers return
/// [`EntityMention`](super::types::EntityMention) values with UTF-8 spans;
/// the engine stores them on the fingerprint as independent evidence.
pub trait EntityProvider: Send + Sync {
    fn extract(&self, text: &str, language: Option<&str>) -> Vec<super::types::EntityMention>;

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("entities").with_quality(CapabilityLevel::Basic)
    }
}

/// Provider boundary for script transliteration (e.g. `привет ↔ privet`).
/// Providers return zero or more converted views; the engine stores each as
/// a `transliteration:<script>` fingerprint view.
pub trait TransliterationProvider: Send + Sync {
    fn transliterate(&self, text: &str) -> Vec<Transliteration>;

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("transliteration").with_quality(CapabilityLevel::Basic)
    }
}

/// Provider boundary for grapheme-to-phoneme conversion.
pub trait G2PProvider: Send + Sync {
    fn phonemize(&self, text: &str, language: &str) -> Result<PhoneticCandidate, ProviderError>;

    fn phonemize_batch(
        &self,
        texts: &[String],
        language: &str,
    ) -> Result<Vec<PhoneticCandidate>, ProviderError> {
        texts
            .iter()
            .map(|text| self.phonemize(text, language))
            .collect()
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("g2p")
    }

    /// Cache state when this provider serves through a revision-aware cache,
    /// `None` when uncached. Counts only — never cached texts.
    fn cache_diagnostics(&self) -> Option<CacheDiagnostics> {
        None
    }
}

/// Provider boundary for probabilistic language detection.
pub trait LanguageDetectionProvider: Send + Sync {
    fn detect(&self, text: &str) -> Result<Vec<LanguageCandidate>, ProviderError>;

    fn detect_batch(&self, texts: &[String]) -> Result<Vec<Vec<LanguageCandidate>>, ProviderError> {
        texts.iter().map(|text| self.detect(text)).collect()
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("language").with_quality(CapabilityLevel::Basic)
    }

    /// Cache state when this provider serves through a revision-aware cache,
    /// `None` when uncached. Counts only — never cached texts.
    fn cache_diagnostics(&self) -> Option<CacheDiagnostics> {
        None
    }
}

/// Optional language-aware lemmatization.  The built-in light stemmer remains
/// available when this provider is not configured.
pub trait LemmatizerProvider: Send + Sync {
    fn lemmatize(
        &self,
        tokens: &[String],
        language: Option<&str>,
    ) -> Result<Vec<String>, ProviderError>;

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("lemmatizer").with_quality(CapabilityLevel::Basic)
    }
}

/// Language-independent boundary for word, lemma, and stop-word resources.
/// Implementations can be backed by JSON, SQLite, a compressed dictionary, or
/// a remote/local service without changing the analysis pipeline.
pub trait LexiconProvider: Send + Sync {
    fn contains(&self, word: &str, languages: Option<&[String]>) -> bool;
    fn starts_with(&self, prefix: &str, languages: Option<&[String]>) -> bool;
    fn is_stop_word(&self, word: &str, languages: Option<&[String]>) -> bool;
    fn lemma(&self, word: &str, languages: Option<&[String]>) -> Option<String>;

    fn frequency(&self, _word: &str, _languages: Option<&[String]>) -> Option<f64> {
        None
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("lexicon").with_quality(CapabilityLevel::Basic)
    }
}

/// Provider boundary for calibrated spam or abuse classification. The
/// deterministic feature extractor remains usable without this provider.
pub trait SpamPredictor: Send + Sync {
    fn predict(
        &self,
        fingerprint: &MessageFingerprint,
        patterns: &[PatternMatch],
    ) -> Result<SpamResult, ProviderError>;

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("spam_predictor")
    }
}

/// Provider boundary for learned or calibrated similarity scoring. The
/// weighted scorer is the deterministic default used by the engine.
pub trait SimilarityScorer: Send + Sync {
    fn score(&self, left: &MessageFingerprint, right: &MessageFingerprint) -> ComparisonResult;

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("similarity_scorer")
    }
}

/// Optional knowledge source for symbols, emoji, and number readings.
pub trait SymbolKnowledgeProvider: Send + Sync {
    fn readings(&self, token: &str, max_readings: usize) -> Vec<SymbolReading>;

    /// Language-filtered readings. The default implementation truncates
    /// before filtering (callers filter afterwards); providers with indexed
    /// packs should filter first so language-specific readings survive.
    fn readings_in_languages(
        &self,
        token: &str,
        max_readings: usize,
        languages: Option<&[String]>,
    ) -> Vec<SymbolReading> {
        let _ = languages;
        self.readings(token, max_readings)
    }

    fn concepts(&self, _token: &str) -> Vec<SymbolConcept> {
        Vec::new()
    }

    fn unicode_name(&self, _token: &str) -> Option<String> {
        None
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("symbols").with_quality(CapabilityLevel::Basic)
    }
}

/// Optional knowledge source for chat abbreviations and slang expansions.
///
/// Language-specific mappings live in versioned resource packs
/// (`resources/abbreviations/*.json`); generic core logic must stay
/// language-neutral and consult this provider instead of hardcoding tokens.
pub trait AbbreviationProvider: Send + Sync {
    fn abbreviation_readings(
        &self,
        token: &str,
        languages: Option<&[String]>,
        max_readings: usize,
    ) -> Vec<SymbolReading>;

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("abbreviations").with_quality(CapabilityLevel::Basic)
    }
}

/// Optional reranker for a retrieved candidate set.  Returning the input is a
/// valid no-op implementation.
pub trait RerankerProvider: Send + Sync {
    fn rerank(
        &self,
        query: &MessageFingerprint,
        candidates: Vec<(String, MessageFingerprint, f64)>,
    ) -> Result<Vec<(String, MessageFingerprint, f64)>, ProviderError>;

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("reranker")
    }
}

/// Explicit vector-store capabilities: persistence, ANN availability, and
/// indexed retrieval channels. Diagnostics must use this instead of inferring
/// behavior from provider names (a `MemoryStore` may contain an HNSW index
/// internally while reporting a plain `memory_store` provider).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]
pub struct VectorStoreCapabilities {
    /// `memory`, `json`, `redb`, or a custom store label.
    #[serde(default)]
    pub store_type: String,
    #[serde(default)]
    pub persistent: bool,
    /// True only when an ANN index is actually serving.
    #[serde(default)]
    pub ann_enabled: bool,
    /// ANN vector dimensions when enabled.
    #[serde(default)]
    pub ann_dimensions: Option<usize>,
    /// Live ANN entries (tombstoned removals excluded).
    #[serde(default)]
    pub ann_entries: usize,
    /// Embedding model backing the ANN vectors (`model_id@revision`) when
    /// the index tracks one. Queries from a different revision skip the ANN
    /// channel instead of comparing across revisions.
    #[serde(default)]
    pub ann_model: Option<String>,
    /// Retrieval channels the store can serve
    /// (`lexical`, `semantic_ann`, …).
    #[serde(default)]
    pub indexed_channels: Vec<String>,
}

/// Storage abstraction for searchable fingerprints.  The in-memory store is
/// the default; vector databases can implement this trait without changing the
/// analyzer or comparison code.
pub trait VectorStore: Send + Sync {
    fn upsert(&mut self, id: String, fingerprint: MessageFingerprint) -> Result<(), String>;
    fn remove(&mut self, id: &str) -> Result<bool, String>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn records(&self) -> Vec<(String, MessageFingerprint)>;

    fn search_candidates(
        &self,
        _query: &MessageFingerprint,
        limit: usize,
    ) -> Result<Vec<(String, MessageFingerprint)>, String> {
        let mut records = self.records();
        records.truncate(limit);
        Ok(records)
    }

    fn search_candidates_with_metadata(
        &self,
        query: &MessageFingerprint,
        limit: usize,
    ) -> Result<SearchCandidateSet, String> {
        Ok(SearchCandidateSet {
            records: self.search_candidates(query, limit)?,
            channels: vec!["document_scan_fallback".to_string()],
        })
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("vector_store").with_quality(CapabilityLevel::Basic)
    }

    /// Explicit store capabilities for diagnostics. Custom stores inherit a
    /// non-persistent, non-ANN report; override when the store persists or
    /// serves an ANN index.
    fn store_capabilities(&self) -> VectorStoreCapabilities {
        VectorStoreCapabilities::default()
    }
}

/// Shared ownership preserves provider behavior: every method (including
/// capability and cache introspection) forwards to the inner provider. This
/// lets generic wrappers such as the revision-aware caches hold
/// `Arc<dyn EmbeddingProvider>` without changing what they report.
impl<T: EmbeddingProvider + ?Sized> EmbeddingProvider for Arc<T> {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        (**self).embed(texts)
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        (**self).embed_batch(texts)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        (**self).capabilities()
    }

    fn model_metadata(&self) -> Option<ModelMetadata> {
        (**self).model_metadata()
    }

    fn health_check(&self) -> Result<(), ProviderError> {
        (**self).health_check()
    }

    fn cache_diagnostics(&self) -> Option<CacheDiagnostics> {
        (**self).cache_diagnostics()
    }
}

impl<T: G2PProvider + ?Sized> G2PProvider for Arc<T> {
    fn phonemize(&self, text: &str, language: &str) -> Result<PhoneticCandidate, ProviderError> {
        (**self).phonemize(text, language)
    }

    fn phonemize_batch(
        &self,
        texts: &[String],
        language: &str,
    ) -> Result<Vec<PhoneticCandidate>, ProviderError> {
        (**self).phonemize_batch(texts, language)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        (**self).capabilities()
    }

    fn cache_diagnostics(&self) -> Option<CacheDiagnostics> {
        (**self).cache_diagnostics()
    }
}

impl<T: LanguageDetectionProvider + ?Sized> LanguageDetectionProvider for Arc<T> {
    fn detect(&self, text: &str) -> Result<Vec<LanguageCandidate>, ProviderError> {
        (**self).detect(text)
    }

    fn detect_batch(&self, texts: &[String]) -> Result<Vec<Vec<LanguageCandidate>>, ProviderError> {
        (**self).detect_batch(texts)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        (**self).capabilities()
    }

    fn cache_diagnostics(&self) -> Option<CacheDiagnostics> {
        (**self).cache_diagnostics()
    }
}

pub type SharedAbbreviationProvider = Arc<dyn AbbreviationProvider>;
pub type SharedEmbeddingProvider = Arc<dyn EmbeddingProvider>;
pub type SharedEntityProvider = Arc<dyn EntityProvider>;
pub type SharedG2PProvider = Arc<dyn G2PProvider>;
pub type SharedLanguageProvider = Arc<dyn LanguageDetectionProvider>;
pub type SharedLemmatizerProvider = Arc<dyn LemmatizerProvider>;
pub type SharedLexiconProvider = Arc<dyn LexiconProvider>;
pub type SharedSymbolProvider = Arc<dyn SymbolKnowledgeProvider>;
pub type SharedRerankerProvider = Arc<dyn RerankerProvider>;
pub type SharedSpamPredictor = Arc<dyn SpamPredictor>;
pub type SharedSimilarityScorer = Arc<dyn SimilarityScorer>;
