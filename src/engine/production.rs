//! Production preset, ergonomic builder, and engine diagnostics.
//!
//! [`TextIntelligence::default`](crate::engine::TextIntelligence::default)
//! stays lightweight and deterministic. Everything production-grade lives
//! here: [`EngineBuilder`] wires explicit providers,
//! [`EngineBuilder::production_local`] assembles the local production stack
//! (resource packs, espeak-ng G2P with a
//! rule-based fallback), and [`EngineDiagnostics`] reports exactly what is
//! serving and what was degraded. No network access happens unless a remote
//! provider is configured explicitly.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::core::capabilities::ProviderCapabilities;
use crate::core::config::EngineConfig;
use crate::core::error::TextIntelError;
use crate::core::providers::{
    AbbreviationProvider, EmbeddingProvider, EntityProvider, G2PProvider,
    LanguageDetectionProvider, LemmatizerProvider, LexiconProvider, RerankerProvider,
    SimilarityScorer, SpamPredictor, SymbolKnowledgeProvider, TransliterationProvider,
};
use crate::resources::{ResourceLoader, ResourcePackInfo};

/// One capability that is serving below production quality (or not at all).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DegradedCapability {
    /// Channel or subsystem, e.g. `"phonetic"`, `"semantic"`, `"spam"`.
    pub capability: String,
    /// What is actually serving, e.g. `"rule_based_g2p"`, `"disabled"`.
    pub configured: String,
    /// What production quality would use, e.g. `"espeak_ng_g2p"`.
    pub wanted: String,
    /// Why, and how to fix it.
    pub detail: String,
}

/// Retrieval candidate budgets behind the candidate union: per-channel
/// contribution caps, the semantic-ANN channel cap, and the final union cut.
/// Counts only — safe to log and export.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CandidateBudgets {
    pub max_search_candidates: usize,
    pub max_per_channel_candidates: usize,
    pub max_ann_candidates: usize,
}

/// Reproducibility and deployment report for an engine instance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EngineDiagnostics {
    pub api_version: String,
    pub fingerprint_schema: u32,
    pub resource_languages: Vec<String>,
    pub abbreviation_languages: Vec<String>,
    pub symbol_languages: Vec<String>,
    pub symbol_tokens: usize,
    pub embedding: ProviderCapabilities,
    /// Embedding model identity (model id, revision, dimensions, source
    /// path) when the provider reports one. Counts and names only — never
    /// user text or vectors.
    #[serde(default)]
    pub embedding_model: Option<crate::core::capabilities::ModelMetadata>,
    pub g2p: ProviderCapabilities,
    pub language: ProviderCapabilities,
    pub lexicon: ProviderCapabilities,
    pub symbols: ProviderCapabilities,
    pub abbreviations: Option<ProviderCapabilities>,
    #[serde(default)]
    pub transliteration: Option<ProviderCapabilities>,
    #[serde(default)]
    pub entities: Option<ProviderCapabilities>,
    pub spam: ProviderCapabilities,
    pub similarity: Option<ProviderCapabilities>,
    pub reranker: Option<ProviderCapabilities>,
    pub store: ProviderCapabilities,
    pub ann_enabled: bool,
    pub degraded: Vec<DegradedCapability>,
    pub resource_manifest: Vec<ResourcePackInfo>,
    /// Bounded revision-aware caches by subsystem (`embeddings`, `g2p`,
    /// `language`, `rebus`). Counts only — never cached texts.
    #[serde(default)]
    pub caches: std::collections::BTreeMap<String, crate::cache::CacheDiagnostics>,
    /// Explicit store capabilities: `memory`/`json`/`redb`, persistence, ANN
    /// status (enabled, dimensions, live entries), and indexed channels.
    #[serde(default)]
    pub store_capabilities: crate::core::providers::VectorStoreCapabilities,
    /// Retrieval candidate budgets (per-channel, ANN, union cut).
    #[serde(default)]
    pub candidate_budgets: CandidateBudgets,
}

/// Ergonomic engine construction. All providers are optional; anything left
/// unset falls back to the deterministic local defaults. Configuration
/// failures return errors, never panics.
#[derive(Default)]
pub struct EngineBuilder {
    pub(crate) config: EngineConfig,
    pub(crate) resources: Option<ResourceLoader>,
    pub(crate) embedding: Option<Arc<dyn EmbeddingProvider>>,
    pub(crate) g2p: Option<Arc<dyn G2PProvider>>,
    pub(crate) language: Option<Arc<dyn LanguageDetectionProvider>>,
    pub(crate) lexicon: Option<Arc<dyn LexiconProvider>>,
    pub(crate) lemmatizer: Option<Arc<dyn LemmatizerProvider>>,
    pub(crate) symbols: Option<Arc<dyn SymbolKnowledgeProvider>>,
    pub(crate) abbreviations: Option<Arc<dyn AbbreviationProvider>>,
    pub(crate) transliteration: Option<Arc<dyn TransliterationProvider>>,
    pub(crate) entity: Option<Arc<dyn EntityProvider>>,
    pub(crate) entity_disabled: bool,
    pub(crate) reranker: Option<Arc<dyn RerankerProvider>>,
    pub(crate) spam: Option<Arc<dyn SpamPredictor>>,
    pub(crate) similarity_scorer: Option<Arc<dyn SimilarityScorer>>,
    pub(crate) similarity_profile: Option<crate::comparison::SimilarityProfile>,
    pub(crate) json_store_path: Option<PathBuf>,
    pub(crate) json_store_ann: Option<(usize, usize)>,
}

impl EngineBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn config(mut self, config: EngineConfig) -> Self {
        self.config = config;
        self
    }

    pub fn resources(mut self, resources: ResourceLoader) -> Self {
        self.resources = Some(resources);
        self
    }

    /// Spec-named alias for the embedding backend.
    pub fn semantic_provider<P: EmbeddingProvider + 'static>(mut self, provider: P) -> Self {
        self.embedding = Some(Arc::new(provider));
        self
    }

    pub fn g2p_provider<P: G2PProvider + 'static>(mut self, provider: P) -> Self {
        self.g2p = Some(Arc::new(provider));
        self
    }

    pub fn language_provider<P: LanguageDetectionProvider + 'static>(
        mut self,
        provider: P,
    ) -> Self {
        self.language = Some(Arc::new(provider));
        self
    }

    pub fn lexicon_provider<P: LexiconProvider + 'static>(mut self, provider: P) -> Self {
        self.lexicon = Some(Arc::new(provider));
        self
    }

    pub fn lemmatizer_provider<P: LemmatizerProvider + 'static>(mut self, provider: P) -> Self {
        self.lemmatizer = Some(Arc::new(provider));
        self
    }

    pub fn symbol_provider<P: SymbolKnowledgeProvider + 'static>(mut self, provider: P) -> Self {
        self.symbols = Some(Arc::new(provider));
        self
    }

    pub fn abbreviation_provider<P: AbbreviationProvider + 'static>(mut self, provider: P) -> Self {
        self.abbreviations = Some(Arc::new(provider));
        self
    }

    pub fn transliteration_provider<P: TransliterationProvider + 'static>(
        mut self,
        provider: P,
    ) -> Self {
        self.transliteration = Some(Arc::new(provider));
        self
    }

    pub fn entity_provider<P: EntityProvider + 'static>(mut self, provider: P) -> Self {
        self.entity = Some(Arc::new(provider));
        self.entity_disabled = false;
        self
    }

    /// Disable entity extraction for the built engine.
    pub fn without_entities(mut self) -> Self {
        self.entity = None;
        self.entity_disabled = true;
        self
    }

    pub fn reranker_provider<P: RerankerProvider + 'static>(mut self, provider: P) -> Self {
        self.reranker = Some(Arc::new(provider));
        self
    }

    pub fn spam_predictor<P: SpamPredictor + 'static>(mut self, predictor: P) -> Self {
        self.spam = Some(Arc::new(predictor));
        self
    }

    pub fn similarity_scorer<P: SimilarityScorer + 'static>(mut self, scorer: P) -> Self {
        self.similarity_scorer = Some(Arc::new(scorer));
        self
    }

    pub fn similarity_profile(mut self, profile: crate::comparison::SimilarityProfile) -> Self {
        self.similarity_profile = Some(profile);
        self
    }

    /// Persist fingerprints to a local JSON file store.
    pub fn json_store(mut self, path: impl Into<PathBuf>) -> Self {
        self.json_store_path = Some(path.into());
        self.json_store_ann = None;
        self
    }

    /// Persist fingerprints to a local JSON file store with an HNSW
    /// accelerator over `dimensions`-wide whole-text embeddings. The graph is
    /// rebuilt automatically from the loaded records on open; without the
    /// `ann-hnsw` feature the build fails instead of silently dropping ANN.
    pub fn json_store_with_ann(
        mut self,
        path: impl Into<PathBuf>,
        dimensions: usize,
        max_elements: usize,
    ) -> Self {
        self.json_store_path = Some(path.into());
        self.json_store_ann = Some((dimensions, max_elements));
        self
    }

    /// Enable deterministic local providers and bounded caches. Does not read
    /// model files, environment model paths, or contact remote services.
    pub fn production_local(mut self) -> Self {
        self.config.semantic = true;
        self.config.phonetic = true;
        self.config.cache = std::mem::take(&mut self.config.cache).with_production_defaults();
        #[cfg(feature = "phonetic-espeak")]
        if self.g2p.is_none()
            && let Ok(provider) = crate::phonetic::EspeakNgG2PProvider::auto_detect()
        {
            self.g2p = Some(Arc::new(provider));
        }
        if self.embedding.is_none() {
            self.embedding = Some(Arc::new(
                crate::semantic::FeatureHashEmbeddingProvider::default(),
            ));
        }
        self
    }
    /// Validate configuration and assemble the library engine.
    pub fn build(self) -> Result<crate::engine::TextIntelligence, TextIntelError> {
        crate::engine::TextIntelligence::assemble(self)
    }
}
