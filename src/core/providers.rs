use std::sync::Arc;

use super::capabilities::{CapabilityLevel, ModelMetadata, ProviderCapabilities};
use super::error::ProviderError;
use super::types::{
    ComparisonResult, LanguageCandidate, MessageFingerprint, PatternMatch, PhoneticCandidate,
    SearchCandidateSet, SpamResult, SymbolConcept, SymbolReading,
};

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
}

pub type SharedAbbreviationProvider = Arc<dyn AbbreviationProvider>;
pub type SharedEmbeddingProvider = Arc<dyn EmbeddingProvider>;
pub type SharedG2PProvider = Arc<dyn G2PProvider>;
pub type SharedLanguageProvider = Arc<dyn LanguageDetectionProvider>;
pub type SharedLemmatizerProvider = Arc<dyn LemmatizerProvider>;
pub type SharedLexiconProvider = Arc<dyn LexiconProvider>;
pub type SharedSymbolProvider = Arc<dyn SymbolKnowledgeProvider>;
pub type SharedRerankerProvider = Arc<dyn RerankerProvider>;
pub type SharedSpamPredictor = Arc<dyn SpamPredictor>;
pub type SharedSimilarityScorer = Arc<dyn SimilarityScorer>;
