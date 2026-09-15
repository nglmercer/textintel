//! Local-first multilingual text intelligence.
//!
//! The crate deliberately keeps the analysis channels independent.  A
//! [`MessageFingerprint`] preserves the original message while exposing
//! Unicode, lexical, visual, symbolic, rebus, phonetic, semantic, and
//! obfuscation views.  Providers for expensive or remote capabilities are
//! optional and can be replaced without changing the engine API.

pub mod cache;
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
pub mod transliteration;
pub mod visual;

pub use cache::{
    rebus_cache_key, resource_revision, CacheDiagnostics, CachedG2PProvider,
    CachedLanguageDetectionProvider, CachedRebusDecoder, RevisionCache,
};
pub use comparison::{
    balanced_sample_weights, language_agreement, logistic_step, logistic_step_weighted,
    rerank_score, score_fingerprints_with_profile, sigmoid, training_features,
    ChannelRerankWeights, ChannelScoreReranker, LogisticSimilarityScorer, RerankerModelArtifact,
    SimilarityModelArtifact, SimilarityProfile, TRAINING_FEATURES, TRAINING_FEATURE_SCHEMA_VERSION,
};
pub use core::capabilities::{CapabilityLevel, ModelMetadata, ProviderCapabilities};
pub use core::config::{CacheLimits, EngineConfig, RebusWeights, SimilarityWeights};
pub use core::error::{ProviderError, TextIntelError};
pub use core::providers::{
    AbbreviationProvider, EmbeddingProvider, G2PProvider, LanguageDetectionProvider,
    LemmatizerProvider, LexiconProvider, RerankerProvider, SharedAbbreviationProvider,
    SimilarityScorer, SpamPredictor, SymbolKnowledgeProvider, Transliteration,
    TransliterationProvider, VectorStore, VectorStoreCapabilities,
};
pub use core::types::*;
pub use detection::{
    duplicate_result, match_pattern, predict_spam, spam_feature_vector, spam_features,
    HeuristicSpamPredictor, SpamModelArtifact, TrainedSpamPredictor, SPAM_FEATURES,
    SPAM_FEATURE_SCHEMA_VERSION,
};
pub use engine::production::{DegradedCapability, EngineBuilder, EngineDiagnostics};
pub use engine::TextIntelligence;
pub use language::{NgramLanguageDetector, ProfileLanguageDetector};
#[cfg(feature = "phonetic-espeak")]
pub use phonetic::{
    parse_espeak_ipa, parse_voices_table, primary_stress_syllables, EspeakNgG2PProvider,
    EspeakVoice, DEFAULT_ESPEAK_TIMEOUT,
};
pub use phonetic::{NullG2PProvider, RuleBasedG2PProvider};
pub use resources::{
    embedded_common, embedded_resources, normalize_key, AbbreviationEntry, AbbreviationPack,
    AbbreviationReading, DefaultLexiconProvider, IndexKey, LanguageIndex, LanguagePack,
    LexiconEntry, LexiconLookup, LexiconRecord, LookupStatus, ResourceError, ResourceLimits,
    ResourceLoader, ResourcePackInfo, SymbolPack, SymbolResource, SUPPORTED_SCHEMA_VERSION,
};
#[cfg(feature = "semantic-candle")]
pub use semantic::CandleEmbeddingProvider;
#[cfg(feature = "semantic-http")]
pub use semantic::HttpEmbeddingProvider;
pub use semantic::{
    CachedEmbeddingProvider, FeatureHashEmbeddingProvider, NullEmbeddingProvider,
    StaticEmbeddingProvider,
};
#[cfg(feature = "semantic-transformer")]
pub use semantic::{EncodedBatch, TransformerEmbeddingProvider, TransformerPooling};
#[cfg(feature = "ann-hnsw")]
pub use storage::HnswVectorIndex;
#[cfg(feature = "persist-redb")]
pub use storage::RedbStore;
pub use storage::{
    migrate_fingerprint_bytes, JsonFileStore, MemoryStore, MigratedFingerprint,
    OLDEST_SUPPORTED_FINGERPRINT_VERSION, PATTERN_STORE_SCHEMA_VERSION,
};
pub use symbols::DefaultSymbolKnowledge;
pub use transliteration::{
    transliteration_evidence, transliteration_similarity, RuleBasedTransliterationProvider,
    TransliterationEvidence,
};
