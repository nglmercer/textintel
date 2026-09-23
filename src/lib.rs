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
//! preset (resource packs, trained models when present, bounded caches),
//! and [`TextIntelligence::builder`] allows explicit provider, store, and
//! model configuration.  See `docs/API.md` for the narrative API
//! reference, `resources/README.md` for the resource-pack format, and the
//! `examples/` directory for runnable end-to-end flows.

pub mod cache;
pub mod cli;
pub mod comparison;
pub mod core;
pub mod decision;
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
pub mod storage;
pub mod symbols;
pub mod transliteration;
pub mod visual;

pub use cache::{
    CacheDiagnostics, CachedG2PProvider, CachedLanguageDetectionProvider, CachedRebusDecoder,
    RevisionCache, rebus_cache_key, resource_revision,
};
pub use comparison::{
    ChannelRerankWeights, ChannelScoreReranker, LogisticSimilarityScorer, ProfileSimilarityScorer,
    RerankerModelArtifact, SimilarityModelArtifact, SimilarityProfile,
    TRAINING_FEATURE_SCHEMA_VERSION, TRAINING_FEATURES, balanced_sample_weights,
    language_agreement, logistic_step, logistic_step_weighted, mean_channel_confidence,
    rerank_score, score_fingerprints_with_profile, sigmoid, training_features,
};
pub use core::capabilities::{CapabilityLevel, ModelMetadata, ProviderCapabilities};
pub use core::config::{CacheLimits, EngineConfig, RebusWeights, SimilarityWeights};
pub use core::error::{ProviderError, TextIntelError};
pub use core::providers::{
    AbbreviationProvider, EmbeddingProvider, EntityProvider, G2PProvider, GeneratedText,
    GenerationOptions, GenerativeProvider, LanguageDetectionProvider, LemmatizerProvider,
    LexiconProvider, RerankerProvider, SharedAbbreviationProvider, SimilarityScorer, SpamPredictor,
    SymbolKnowledgeProvider, Transliteration, TransliterationProvider, VectorStore,
    VectorStoreCapabilities,
};
pub use core::types::*;
#[cfg(feature = "decision-transformer")]
pub use decision::TransformerBackbone;
pub use decision::{
    ADAPTER_ACCEPT_THRESHOLD, ARCHITECTURE_CANDIDATE_CROSS_ENCODER,
    ARCHITECTURE_STATE_CANDIDATE_INTERACTION, BACKBONE_MODEL_TYPE_BERT, COVERAGE_LEVELS,
    CalibrationSample, CandidatePrompt, CoveragePoint, CrossEncoderHeadConfig,
    DECISION_ARTIFACT_KIND, DECISION_ARTIFACT_VERSION, DECISION_SCHEMA_VERSION, Decision,
    DecisionAnswer, DecisionArtifact, DecisionBackbone, DecisionCalibration, DecisionDataset,
    DecisionDatasetRef, DecisionEvalReport, DecisionExample, DecisionHead, DecisionModelInfo,
    DecisionProvider, DecisionQuestion, DecisionRequest, DecisionResponse, DecisionSplit,
    FUSION_FEATURE_SCHEMA_VERSION, FUSION_FEATURES, HeadTrainExample, HeadTrainer,
    INTERACTION_ARTIFACT_KIND, INTERACTION_ARTIFACT_VERSION, INTERACTION_EMBEDDING_CACHE,
    INTERACTION_TEMPERATURE, InteractionArtifact, InteractionDecisionProvider, InteractionHead,
    MAX_DECISION_BATCH, MAX_DECISION_CANDIDATES, MAX_DECISION_STATE_CHARS, NONE_OF_THE_ABOVE,
    PROBABILITY_SUM_TOLERANCE, SharedDecisionProvider, SimilarityDecisionProvider,
    SpamDecisionProvider, SplitMix64, TaskCalibration, TemperatureBias, TemperatureFit,
    TemperatureScaling, brier_score, candidate_prompt, check_decision_gates, energy_score, entropy,
    evaluate_decisions, expected_calibration_error, expected_score, fit_temperature,
    fusion_feature_vector, fusion_features, gelu, gelu_prime, head_loss_accuracy, init_head_xavier,
    interaction_features, is_ood_by_energy, margin, max_probability, nll_loss,
    plan_candidate_batch, selective_decision, softmax, softmax_with_temperature,
    validate_cross_encoder_request, validate_distribution, validate_request,
};
pub use detection::{
    HeuristicSpamPredictor, SPAM_FEATURE_SCHEMA_VERSION, SPAM_FEATURES, SpamModelArtifact,
    TrainedSpamPredictor, duplicate_result, match_pattern, predict_spam, spam_feature_vector,
    spam_features,
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
#[cfg(feature = "semantic-http")]
pub use semantic::HttpEmbeddingProvider;
pub use semantic::{
    ARCTIC_EMBED_XS, CachedEmbeddingProvider, FallbackEmbeddingProvider,
    FeatureHashEmbeddingProvider, LFM2_5_230M, LFM2_5_350M, LIQUID_DEFAULT_ENDPOINT,
    LIQUID_DEFAULT_MODEL, LiquidInstructProvider, MODERN_EMBEDDING_MODELS,
    MODERN_GENERATIVE_MODELS, MULTILINGUAL_E5_SMALL, MXBAI_EMBED_XSMALL, NullEmbeddingProvider,
    StaticEmbeddingProvider, embedding_model, generative_model,
};
#[cfg(feature = "semantic-transformer")]
pub use semantic::{EncodedBatch, TransformerEmbeddingProvider, TransformerPooling};
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
