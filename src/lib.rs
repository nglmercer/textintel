//! Local-first multilingual text intelligence.
//!
//! The crate deliberately keeps the analysis channels independent.  A
//! [`MessageFingerprint`] preserves the original message while exposing
//! Unicode, lexical, visual, symbolic, rebus, phonetic, semantic, and
//! obfuscation views.  Providers for expensive or remote capabilities are
//! optional and can be replaced without changing the engine API.
//!
//! # Quick start
//!
//! ```rust
//! use textintel::TextIntelligence;
//!
//! let engine = TextIntelligence::default();
//! let fingerprint = engine.analyze("Fra🏠do")?;
//! let comparison = engine.compare("Fra🏠do", "fracasado")?;
//! let candidates = engine.decode("salU2")?;
//!
//! assert_eq!(fingerprint.raw, "Fra🏠do");
//! assert!(candidates.iter().any(|candidate| candidate.text.eq_ignore_ascii_case("saludos")));
//! # Ok::<(), textintel::TextIntelError>(())
//! ```
//!
//! [`TextIntelligence::production_local`] selects the local production
//! preset (resource packs, deterministic providers, bounded caches),
//! and [`TextIntelligence::builder`] configures providers and storage.  See `docs/API.md` for the narrative API
//! reference, `resources/README.md` for the resource-pack format, and the
//! `examples/` directory for runnable end-to-end flows.

pub mod cache;
pub mod cli;
pub mod comparison;
pub mod core;
pub mod detection;
pub mod engine;
pub mod entities;
pub mod evaluation;
pub mod language;
pub mod lexical;
pub mod normalization;
pub mod obfuscation;
pub mod phonetic;
pub mod rebus;
pub mod resources;
pub mod semantic;
pub mod speech;
pub mod storage;
pub mod symbols;
pub mod transliteration;
pub mod visual;

pub use cache::{
    CacheDiagnostics, CachedG2PProvider, CachedLanguageDetectionProvider, CachedRebusDecoder,
    RevisionCache, rebus_cache_key, resource_revision,
};
pub use comparison::{ProfileSimilarityScorer, SimilarityProfile, score_fingerprints_with_profile};
pub use core::capabilities::{CapabilityLevel, ModelMetadata, ProviderCapabilities};
pub use core::config::{CacheLimits, EngineConfig, RebusWeights, SimilarityWeights};
pub use core::error::{ProviderError, TextIntelError};
pub use core::providers::{
    AbbreviationProvider, EmbeddingProvider, EntityProvider, G2PProvider,
    LanguageDetectionProvider, LemmatizerProvider, LexiconProvider, RerankerProvider,
    SharedAbbreviationProvider, SimilarityScorer, SpamPredictor, SymbolKnowledgeProvider,
    Transliteration, TransliterationProvider, VectorStore, VectorStoreCapabilities,
};
pub use core::types::*;
pub use detection::{
    HeuristicSpamPredictor, duplicate_result, match_pattern, predict_spam, spam_features,
};
pub use engine::TextIntelligence;
pub use engine::production::{
    CandidateBudgets, DegradedCapability, EngineBuilder, EngineDiagnostics,
};
pub use entities::{
    DEFAULT_MAX_ENTITIES, DEFAULT_MAX_ENTITY_SPAN, RuleBasedEntityProvider, entity_agreement,
    entity_conflict,
};
pub use language::{NgramLanguageDetector, ProfileLanguageDetector};
#[cfg(feature = "phonetic-espeak")]
pub use phonetic::{
    DEFAULT_ESPEAK_TIMEOUT, EspeakNgG2PProvider, EspeakVoice, parse_espeak_ipa, parse_voices_table,
    primary_stress_syllables,
};
pub use phonetic::{NullG2PProvider, RuleBasedG2PProvider};
pub use resources::{
    AbbreviationEntry, AbbreviationPack, AbbreviationReading, DefaultLexiconProvider, IndexKey,
    LanguageIndex, LanguagePack, LexiconEntry, LexiconLookup, LexiconRecord, LookupStatus,
    ResourceError, ResourceLimits, ResourceLoader, ResourcePackInfo, SUPPORTED_SCHEMA_VERSION,
    SymbolPack, SymbolResource, embedded_common, embedded_resources, normalize_key,
};
pub use semantic::{
    CachedEmbeddingProvider, FallbackEmbeddingProvider, FeatureHashEmbeddingProvider,
    NullEmbeddingProvider, StaticEmbeddingProvider,
};
#[cfg(feature = "ann-hnsw")]
pub use storage::HnswVectorIndex;
#[cfg(feature = "persist-redb")]
pub use storage::RedbStore;
pub use storage::{
    JsonFileStore, MemoryStore, MigratedFingerprint, OLDEST_SUPPORTED_FINGERPRINT_VERSION,
    PATTERN_STORE_SCHEMA_VERSION, migrate_fingerprint_bytes,
};
pub use symbols::DefaultSymbolKnowledge;
pub use transliteration::{
    RuleBasedTransliterationProvider, TransliterationEvidence, effective_transliteration_evidence,
    transliteration_compatibility, transliteration_evidence, transliteration_similarity,
};

pub use speech::{
    ClockPolicy, CurrencyNaming, DateOrder, InvalidDatePolicy, NumberNotation, NumberReading,
    NumberValue, ReadingStyle, SpeechDocument, SpeechInterpretation, SpeechLocale,
    SpeechNormalization, SpeechOptions, SpeechReading, SpeechSpan, SpeechValue, parse_for_speech,
};

pub use speech::{normalize_for_speech, verbalize_document, verbalize_document_with_options};
