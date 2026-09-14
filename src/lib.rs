//! Local-first multilingual text intelligence.
//!
//! The crate deliberately keeps the analysis channels independent.  A
//! [`MessageFingerprint`] preserves the original message while exposing
//! Unicode, lexical, visual, symbolic, rebus, phonetic, semantic, and
//! obfuscation views.  Providers for expensive or remote capabilities are
//! optional and can be replaced without changing the engine API.

pub mod comparison;
pub mod core;
pub mod detection;
pub mod engine;
pub mod evaluation;
pub mod language;
pub mod lexical;
pub mod normalization;
pub mod obfuscation;
pub mod phonetic;
pub mod rebus;
pub mod resources;
pub mod semantic;
pub mod storage;
pub mod symbols;
pub mod visual;

pub use comparison::{
    score_fingerprints_with_profile, LogisticSimilarityScorer, SimilarityProfile,
};
pub use core::capabilities::{ModelMetadata, ProviderCapabilities};
pub use core::config::{EngineConfig, SimilarityWeights};
pub use core::error::{ProviderError, TextIntelError};
pub use core::providers::{
    EmbeddingProvider, G2PProvider, LanguageDetectionProvider, LemmatizerProvider, LexiconProvider,
    RerankerProvider, SimilarityScorer, SpamPredictor, SymbolKnowledgeProvider, VectorStore,
};
pub use core::types::*;
pub use detection::{duplicate_result, match_pattern, predict_spam, HeuristicSpamPredictor};
pub use engine::TextIntelligence;
pub use language::{NgramLanguageDetector, ProfileLanguageDetector};
#[cfg(feature = "phonetic-espeak")]
pub use phonetic::{parse_espeak_ipa, EspeakNgG2PProvider};
pub use phonetic::{NullG2PProvider, RuleBasedG2PProvider};
pub use resources::{
    embedded_common, embedded_resources, normalize_key, DefaultLexiconProvider, IndexKey,
    LanguageIndex, LanguagePack, LexiconEntry, LexiconLookup, LexiconRecord, LookupStatus,
    ResourceError, ResourceLimits, ResourceLoader, SymbolPack, SymbolResource,
    SUPPORTED_SCHEMA_VERSION,
};
#[cfg(feature = "semantic-candle")]
pub use semantic::CandleEmbeddingProvider;
#[cfg(feature = "semantic-http")]
pub use semantic::HttpEmbeddingProvider;
pub use semantic::{
    CachedEmbeddingProvider, FeatureHashEmbeddingProvider, NullEmbeddingProvider,
    StaticEmbeddingProvider,
};
pub use storage::{JsonFileStore, MemoryStore};
pub use symbols::DefaultSymbolKnowledge;
