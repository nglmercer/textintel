//! Engine construction: [`TextIntelligence`] constructors,
//! [`EngineBuilder`] assembly, the `with_*` provider setters, and cache
//! installation.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, RwLock};

use crate::cache::{
    resource_revision, CachedG2PProvider, CachedLanguageDetectionProvider, RevisionCache,
};
use crate::comparison::model::SimilarityProfile;
use crate::core::config::EngineConfig;
use crate::core::error::TextIntelError;
use crate::core::providers::{
    AbbreviationProvider, EmbeddingProvider, EntityProvider, G2PProvider,
    LanguageDetectionProvider, LemmatizerProvider, LexiconProvider, RerankerProvider,
    SimilarityScorer, SpamPredictor, SymbolKnowledgeProvider, TransliterationProvider, VectorStore,
};
use crate::detection::spam::HeuristicSpamPredictor;
use crate::engine::production::EngineBuilder;
use crate::engine::TextIntelligence;
use crate::language::NgramLanguageDetector;
use crate::phonetic::g2p::RuleBasedG2PProvider;
use crate::resources::ResourceLoader;
use crate::semantic::embeddings::{CachedEmbeddingProvider, NullEmbeddingProvider};
use crate::storage::{JsonFileStore, MemoryStore};
use crate::transliteration::RuleBasedTransliterationProvider;

impl TextIntelligence {
    /// Construct an engine.  Invalid limits or weights are programmer errors;
    /// use [`Self::try_new`] when configuration comes from an untrusted file.
    pub fn new(config: EngineConfig) -> Self {
        Self::try_new(config).expect("invalid TextIntelligence configuration")
    }

    pub fn try_new(config: EngineConfig) -> Result<Self, TextIntelError> {
        config
            .validate()
            .map_err(TextIntelError::InvalidConfiguration)?;
        let resources = Arc::new(
            ResourceLoader::common()
                .map_err(|error| TextIntelError::Serialization(error.to_string()))?,
        );
        Ok(Self::from_parts(config, resources))
    }

    fn from_parts(config: EngineConfig, resources: Arc<ResourceLoader>) -> Self {
        let language_detector = NgramLanguageDetector::from_resources(&resources);
        let mut engine = Self {
            config,
            resources: resources.clone(),
            // `NullEmbeddingProvider` keeps the default engine dependency-free;
            // inject `FeatureHashEmbeddingProvider` (or a model) for local
            // semantic evidence. See `with_embedding_provider`.
            embedding_provider: Arc::new(NullEmbeddingProvider),
            g2p_provider: Arc::new(RuleBasedG2PProvider),
            language_provider: Arc::new(language_detector),
            lexicon_provider: resources.clone(),
            lemmatizer_provider: None,
            symbol_provider: resources.clone(),
            abbreviation_provider: Some(resources.clone()),
            transliteration_provider: Some(Arc::new(RuleBasedTransliterationProvider)),
            entity_provider: Some(Arc::new(
                crate::entities::RuleBasedEntityProvider::default().with_lexicon(resources.clone()),
            )),
            reranker_provider: None,
            spam_predictor: Arc::new(HeuristicSpamPredictor),
            similarity_scorer: None,
            similarity_profile: None,
            preset_fallbacks: Vec::new(),
            store: RwLock::new(Box::new(MemoryStore::default())),
            patterns: RwLock::new(BTreeMap::new()),
            rebus_cache: None,
        };
        engine.install_caches();
        engine
    }

    /// Wrap the embedding, G2P, and language providers in bounded
    /// revision-aware caches and (re)create the rebus cache when
    /// `config.cache` enables them. Providers that already report cache
    /// diagnostics are left alone, so user-wrapped providers are never
    /// double-wrapped. Called after construction and after every provider
    /// swap so `with_*` replacements stay cached consistently.
    fn install_caches(&mut self) {
        let limits = self.config.cache.clone();
        if limits.embeddings > 0 && self.embedding_provider.cache_diagnostics().is_none() {
            self.embedding_provider = Arc::new(CachedEmbeddingProvider::new(
                self.embedding_provider.clone(),
                limits.embeddings,
            ));
        }
        if limits.g2p > 0 && self.g2p_provider.cache_diagnostics().is_none() {
            self.g2p_provider = Arc::new(CachedG2PProvider::new(
                self.g2p_provider.clone(),
                limits.g2p,
            ));
        }
        if limits.language > 0 && self.language_provider.cache_diagnostics().is_none() {
            self.language_provider = Arc::new(CachedLanguageDetectionProvider::new(
                self.language_provider.clone(),
                limits.language,
            ));
        }
        if limits.rebus > 0 {
            let revision = resource_revision(self.resources.manifest());
            match &self.rebus_cache {
                Some(cache) => {
                    if let Ok(mut guard) = cache.lock() {
                        guard.set_revision(&revision);
                    }
                }
                None => {
                    self.rebus_cache = Some(Mutex::new(RevisionCache::new(revision, limits.rebus)));
                }
            }
        } else {
            self.rebus_cache = None;
        }
    }

    /// Drop all cached rebus decodings (used after provider swaps that change
    /// decoding behavior without changing the resource revision).
    fn invalidate_rebus_cache(&mut self) {
        if let Some(cache) = &self.rebus_cache {
            if let Ok(mut guard) = cache.lock() {
                guard.invalidate();
            }
        }
    }

    /// Ergonomic construction: `TextIntelligence::builder().build()?`.
    pub fn builder() -> EngineBuilder {
        EngineBuilder::new()
    }

    /// Local production preset: resource packs, trained models from
    /// `./models` when present, espeak-ng G2P with a rule-based fallback,
    /// and a local embedding baseline. Unavailable pieces degrade gracefully
    /// and are reported by [`Self::diagnostics`]; nothing touches the network.
    pub fn production_local() -> Result<Self, TextIntelError> {
        Self::builder().production_local().build()
    }

    pub(crate) fn assemble(builder: EngineBuilder) -> Result<Self, TextIntelError> {
        builder
            .config
            .validate()
            .map_err(TextIntelError::InvalidConfiguration)?;
        let resources = Arc::new(match builder.resources {
            Some(loader) => loader,
            None => ResourceLoader::common()
                .map_err(|error| TextIntelError::Serialization(error.to_string()))?,
        });
        let mut engine = Self::from_parts(builder.config, resources);
        if let Some(provider) = builder.embedding {
            engine.embedding_provider = provider;
        }
        if let Some(provider) = builder.g2p {
            engine.g2p_provider = provider;
        }
        if let Some(provider) = builder.language {
            engine.language_provider = provider;
        }
        if let Some(provider) = builder.lexicon {
            engine.lexicon_provider = provider;
        }
        if let Some(provider) = builder.lemmatizer {
            engine.lemmatizer_provider = Some(provider);
        }
        if let Some(provider) = builder.symbols {
            engine.symbol_provider = provider;
        }
        if let Some(provider) = builder.abbreviations {
            engine.abbreviation_provider = Some(provider);
        }
        if let Some(provider) = builder.transliteration {
            engine.transliteration_provider = Some(provider);
        }
        if builder.entity_disabled {
            engine.entity_provider = None;
        } else if let Some(provider) = builder.entity {
            engine.entity_provider = Some(provider);
        }
        if let Some(provider) = builder.reranker {
            engine.reranker_provider = Some(provider);
        }
        if let Some(predictor) = builder.spam {
            engine.spam_predictor = predictor;
        }
        if let Some(scorer) = builder.similarity_scorer {
            engine.similarity_scorer = Some(scorer);
        }
        if let Some(profile) = builder.similarity_profile {
            engine.similarity_profile = Some(profile);
        }
        engine.preset_fallbacks = builder.preset_fallbacks;
        if let Some(path) = builder.similarity_model_path {
            if let Some(scorer) = crate::engine::production::load_similarity_scorer(
                &path,
                builder.similarity_model_required,
            )? {
                engine.similarity_scorer = Some(Arc::new(scorer));
            }
        }
        if let Some(path) = builder.spam_model_path {
            if let Some(predictor) =
                crate::engine::production::load_spam_predictor(&path, builder.spam_model_required)?
            {
                engine.spam_predictor = Arc::new(predictor);
            }
        }
        if let Some(path) = builder.reranker_model_path {
            if let Some(reranker) = crate::engine::production::load_reranker(
                &path,
                builder.reranker_model_required,
                builder.reranker_max_candidates,
            )? {
                engine.reranker_provider = Some(Arc::new(reranker));
            }
        }
        if let Some(path) = builder.json_store_path {
            #[cfg(feature = "ann-hnsw")]
            let mut store = match builder.json_store_ann {
                Some((dimensions, max_elements)) => {
                    JsonFileStore::open_with_ann(path, dimensions, max_elements)
                }
                None => JsonFileStore::open(path),
            }
            .map_err(TextIntelError::Storage)?;
            #[cfg(not(feature = "ann-hnsw"))]
            let mut store = {
                if builder.json_store_ann.is_some() {
                    return Err(TextIntelError::InvalidConfiguration(
                        "json_store_with_ann requires the ann-hnsw feature".to_string(),
                    ));
                }
                JsonFileStore::open(path).map_err(TextIntelError::Storage)?
            };
            store.set_retrieval_limits(
                engine.config.max_per_channel_candidates,
                engine.config.max_ann_candidates,
            );
            engine.store = RwLock::new(Box::new(store));
        }
        // Builder-supplied providers replaced the from_parts defaults above;
        // wrap them when the configured cache limits enable caching.
        engine.install_caches();
        Ok(engine)
    }

    pub fn with_embedding_provider<P: EmbeddingProvider + 'static>(mut self, provider: P) -> Self {
        self.embedding_provider = Arc::new(provider);
        self.install_caches();
        self.invalidate_rebus_cache();
        self
    }

    pub fn with_g2p_provider<P: G2PProvider + 'static>(mut self, provider: P) -> Self {
        self.g2p_provider = Arc::new(provider);
        self.install_caches();
        self.invalidate_rebus_cache();
        self
    }

    pub fn with_language_provider<P: LanguageDetectionProvider + 'static>(
        mut self,
        provider: P,
    ) -> Self {
        self.language_provider = Arc::new(provider);
        self.install_caches();
        self
    }

    pub fn with_lexicon_provider<P: LexiconProvider + 'static>(mut self, provider: P) -> Self {
        self.lexicon_provider = Arc::new(provider);
        self.invalidate_rebus_cache();
        self
    }

    /// Replace the language, lexicon, symbol, and abbreviation indexes with one
    /// coherent resource set. This is the normal entry point for application
    /// packs.
    pub fn with_resources(mut self, resources: ResourceLoader) -> Self {
        let resources = Arc::new(resources);
        self.language_provider = Arc::new(NgramLanguageDetector::from_resources(&resources));
        self.lexicon_provider = resources.clone();
        self.symbol_provider = resources.clone();
        self.abbreviation_provider = Some(resources.clone());
        // The rule-based entity extractor consults the lexicon to tell names
        // from capitalized common words; re-attach it to the new resources
        // (custom entity providers are left untouched).
        if self
            .entity_provider
            .as_ref()
            .is_some_and(|provider| provider.capabilities().provider == "rule_based_entities")
        {
            self.entity_provider = Some(Arc::new(
                crate::entities::RuleBasedEntityProvider::default().with_lexicon(resources.clone()),
            ));
        }
        self.resources = resources;
        // A new resource set changes the rebus revision (invalidating when it
        // differs) and re-caches the fresh language detector.
        self.install_caches();
        self
    }

    pub fn with_abbreviation_provider<P: AbbreviationProvider + 'static>(
        mut self,
        provider: P,
    ) -> Self {
        self.abbreviation_provider = Some(Arc::new(provider));
        self.invalidate_rebus_cache();
        self
    }

    pub fn with_lemmatizer_provider<P: LemmatizerProvider + 'static>(
        mut self,
        provider: P,
    ) -> Self {
        self.lemmatizer_provider = Some(Arc::new(provider));
        self
    }

    pub fn with_symbol_knowledge_provider<P: SymbolKnowledgeProvider + 'static>(
        mut self,
        provider: P,
    ) -> Self {
        self.symbol_provider = Arc::new(provider);
        self.invalidate_rebus_cache();
        self
    }

    pub fn with_transliteration_provider<P: TransliterationProvider + 'static>(
        mut self,
        provider: P,
    ) -> Self {
        self.transliteration_provider = Some(Arc::new(provider));
        self
    }

    /// Disable transliteration views (fingerprint keeps all other channels).
    pub fn without_transliteration(mut self) -> Self {
        self.transliteration_provider = None;
        self
    }

    pub fn with_entity_provider<P: EntityProvider + 'static>(mut self, provider: P) -> Self {
        self.entity_provider = Some(Arc::new(provider));
        self
    }

    /// Disable entity extraction (fingerprints carry no entity evidence;
    /// entity features read 0.0, never a penalty).
    pub fn without_entities(mut self) -> Self {
        self.entity_provider = None;
        self
    }

    pub fn with_reranker_provider<P: RerankerProvider + 'static>(mut self, provider: P) -> Self {
        self.reranker_provider = Some(Arc::new(provider));
        self
    }

    pub fn with_spam_predictor<P: SpamPredictor + 'static>(mut self, predictor: P) -> Self {
        self.spam_predictor = Arc::new(predictor);
        self
    }

    pub fn with_similarity_scorer<P: SimilarityScorer + 'static>(mut self, scorer: P) -> Self {
        self.similarity_scorer = Some(Arc::new(scorer));
        self
    }

    pub fn with_similarity_profile(mut self, profile: SimilarityProfile) -> Self {
        self.similarity_profile = Some(profile);
        self
    }

    pub fn with_store<S: VectorStore + 'static>(mut self, store: S) -> Self {
        self.store = RwLock::new(Box::new(store));
        self
    }

    pub fn with_json_store(
        self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<Self, TextIntelError> {
        let store = JsonFileStore::open(path).map_err(TextIntelError::Storage)?;
        Ok(self.with_store(store))
    }
}
