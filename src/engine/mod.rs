//! Engine API and coordination: [`TextIntelligence`] plus one module per
//! responsibility (construction, analysis, comparison, patterns, search,
//! diagnostics, and the production preset). Method implementations live in
//! the submodules as `impl TextIntelligence` blocks; the struct lives here
//! so every submodule can reach its fields.

pub mod analyzer;
pub mod builder;
pub mod comparison;
pub mod diagnostics;
pub mod patterns;
pub mod production;
pub mod search;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, RwLock};

use crate::cache::RevisionCache;
use crate::comparison::model::SimilarityProfile;
use crate::core::config::EngineConfig;
use crate::core::providers::{
    AbbreviationProvider, EmbeddingProvider, EntityProvider, G2PProvider,
    LanguageDetectionProvider, LemmatizerProvider, LexiconProvider, RerankerProvider,
    SimilarityScorer, SpamPredictor, SymbolKnowledgeProvider, TransliterationProvider, VectorStore,
};
use crate::core::types::DecodedCandidate;
use crate::resources::ResourceLoader;

use self::patterns::RegisteredPattern;

/// Main high-level API.  The default instance is local-only and model-free;
/// optional providers can be injected through the `with_*` methods.
pub struct TextIntelligence {
    config: EngineConfig,
    resources: Arc<ResourceLoader>,
    embedding_provider: Arc<dyn EmbeddingProvider>,
    g2p_provider: Arc<dyn G2PProvider>,
    language_provider: Arc<dyn LanguageDetectionProvider>,
    lexicon_provider: Arc<dyn LexiconProvider>,
    lemmatizer_provider: Option<Arc<dyn LemmatizerProvider>>,
    symbol_provider: Arc<dyn SymbolKnowledgeProvider>,
    abbreviation_provider: Option<Arc<dyn AbbreviationProvider>>,
    transliteration_provider: Option<Arc<dyn TransliterationProvider>>,
    entity_provider: Option<Arc<dyn EntityProvider>>,
    reranker_provider: Option<Arc<dyn RerankerProvider>>,
    spam_predictor: Arc<dyn SpamPredictor>,
    similarity_scorer: Option<Arc<dyn SimilarityScorer>>,
    similarity_profile: Option<SimilarityProfile>,
    preset_fallbacks: Vec<crate::engine::production::DegradedCapability>,
    store: RwLock<Box<dyn VectorStore>>,
    patterns: RwLock<BTreeMap<String, RegisteredPattern>>,
    /// Bounded revision-aware rebus cache (`None` when
    /// `config.cache.rebus == 0`). Keys cover input, languages, candidate
    /// limit, scoring weights, and the semantic-rescoring marker; the
    /// revision tracks the loaded resource packs.
    rebus_cache: Option<Mutex<RevisionCache<String, Vec<DecodedCandidate>>>>,
}

impl Default for TextIntelligence {
    fn default() -> Self {
        Self::new(EngineConfig::default())
    }
}

pub use production::{
    preferred_similarity_artifact, preferred_similarity_artifact_in, preferred_spam_artifact,
    preferred_spam_artifact_in, DegradedCapability, EngineBuilder, EngineDiagnostics,
};
