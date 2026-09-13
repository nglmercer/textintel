use std::sync::Arc;

use super::error::ProviderError;
use super::types::{LanguageCandidate, MessageFingerprint, PhoneticCandidate, SymbolReading};

/// Provider boundary for multilingual embeddings.  Providers may be local or
/// remote; the core never chooses a vendor or sends data implicitly.
pub trait EmbeddingProvider: Send + Sync {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError>;
}

/// Provider boundary for grapheme-to-phoneme conversion.
pub trait G2PProvider: Send + Sync {
    fn phonemize(&self, text: &str, language: &str) -> Result<PhoneticCandidate, ProviderError>;
}

/// Provider boundary for probabilistic language detection.
pub trait LanguageDetectionProvider: Send + Sync {
    fn detect(&self, text: &str) -> Result<Vec<LanguageCandidate>, ProviderError>;
}

/// Optional language-aware lemmatization.  The built-in light stemmer remains
/// available when this provider is not configured.
pub trait LemmatizerProvider: Send + Sync {
    fn lemmatize(&self, tokens: &[String], language: Option<&str>) -> Result<Vec<String>, ProviderError>;
}

/// Optional knowledge source for symbols, emoji, and number readings.
pub trait SymbolKnowledgeProvider: Send + Sync {
    fn readings(&self, token: &str, max_readings: usize) -> Vec<SymbolReading>;
}

/// Optional reranker for a retrieved candidate set.  Returning the input is a
/// valid no-op implementation.
pub trait RerankerProvider: Send + Sync {
    fn rerank(
        &self,
        query: &MessageFingerprint,
        candidates: Vec<(String, MessageFingerprint, f64)>,
    ) -> Result<Vec<(String, MessageFingerprint, f64)>, ProviderError>;
}

/// Storage abstraction for searchable fingerprints.  The in-memory store is
/// the default; vector databases can implement this trait without changing the
/// analyzer or comparison code.
pub trait VectorStore: Send + Sync {
    fn upsert(&mut self, id: String, fingerprint: MessageFingerprint) -> Result<(), String>;
    fn remove(&mut self, id: &str) -> Result<bool, String>;
    fn len(&self) -> usize;
    fn records(&self) -> Vec<(String, MessageFingerprint)>;
}

pub type SharedEmbeddingProvider = Arc<dyn EmbeddingProvider>;
pub type SharedG2PProvider = Arc<dyn G2PProvider>;
pub type SharedLanguageProvider = Arc<dyn LanguageDetectionProvider>;
pub type SharedLemmatizerProvider = Arc<dyn LemmatizerProvider>;
pub type SharedSymbolProvider = Arc<dyn SymbolKnowledgeProvider>;
pub type SharedRerankerProvider = Arc<dyn RerankerProvider>;

