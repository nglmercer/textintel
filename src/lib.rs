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

pub use core::config::{EngineConfig, SimilarityWeights};
pub use core::error::{ProviderError, TextIntelError};
pub use core::providers::{
    EmbeddingProvider, G2PProvider, LanguageDetectionProvider, LemmatizerProvider,
    LexiconProvider, RerankerProvider, SymbolKnowledgeProvider, VectorStore,
};
pub use core::types::*;
pub use engine::TextIntelligence;
pub use phonetic::{NullG2PProvider, RuleBasedG2PProvider};
pub use semantic::{NullEmbeddingProvider, StaticEmbeddingProvider};
pub use storage::MemoryStore;
pub use resources::{
    embedded_common, DefaultLexiconProvider, LanguagePack, LexiconEntry, ResourceError,
    ResourceLoader, SymbolPack, SymbolResource,
};
pub use symbols::DefaultSymbolKnowledge;
