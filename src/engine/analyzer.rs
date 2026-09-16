use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use crate::cache::{
    rebus_cache_key, resource_revision, CacheDiagnostics, CachedG2PProvider,
    CachedLanguageDetectionProvider, RevisionCache,
};
use crate::comparison::model::{score_fingerprints_with_profile, SimilarityProfile};
use crate::comparison::scorer::score_fingerprints as weighted_score_fingerprints;
use crate::core::capabilities::ProviderCapabilities;
use crate::core::config::EngineConfig;
use crate::core::error::{ProviderError, TextIntelError};
use crate::core::providers::{
    AbbreviationProvider, EmbeddingProvider, G2PProvider, LanguageDetectionProvider,
    LemmatizerProvider, LexiconProvider, RerankerProvider, SimilarityScorer, SpamPredictor,
    SymbolKnowledgeProvider, TransliterationProvider, VectorStore,
};
use crate::core::types::{
    ChannelAvailability, ComparisonResult, DecodedCandidate, DuplicateMode, DuplicateResult,
    LexicalFeatures, MessageFingerprint, Pattern, PatternMatch, PhoneticCandidate, SearchResult,
    SpokenCandidate, StageTimings, Transformation,
};
use crate::detection::duplicates::{duplicate_result, duplicate_result_with_mode};
use crate::detection::patterns::match_pattern_fingerprint;
use crate::detection::spam::HeuristicSpamPredictor;
use crate::engine::production::{DegradedCapability, EngineBuilder, EngineDiagnostics};
use crate::language::segmentation::segment_message_with_provider;
use crate::language::NgramLanguageDetector;
use crate::lexical::character::char_features;
use crate::lexical::minhash::{minhash_signature, simhash};
use crate::lexical::ngrams::word_ngrams;
use crate::lexical::tokenizer::{simple_lemmas_with_provider, stop_words_with_provider, tokenize};
use crate::normalization::leetspeak::apply_leet;
use crate::normalization::repetition::collapse_repetition;
use crate::normalization::unicode::{casefold_text, nfkc};
use crate::normalization::whitespace::normalize_whitespace;
use crate::obfuscation::features::obfuscation_features;
use crate::phonetic::g2p::RuleBasedG2PProvider;
use crate::rebus::decoder::RebusDecoder;
use crate::resources::ResourceLoader;
use crate::semantic::embeddings::{CachedEmbeddingProvider, NullEmbeddingProvider};
use crate::storage::{JsonFileStore, MemoryStore};
use crate::symbols::resolver::resolve_symbols_with_provider;
use crate::transliteration::RuleBasedTransliterationProvider;
use crate::visual::unicode_features::analyze_unicode;

struct RegisteredPattern {
    pattern: Pattern,
    examples: Vec<(String, MessageFingerprint)>,
}

#[derive(Debug, Clone)]
struct EmbeddingInput {
    key: String,
    text: String,
}

/// Fraction of words present in the lexicon (any language), measuring the
/// *intended* reading: when the top rebus candidate differs from the raw
/// text, coverage runs over the candidate's words (`h3llo` counts through
/// `hello`); otherwise it runs over the raw tokens. Segments without a
/// letter (URLs, emoji, pure numbers) are not words and are excluded from
/// both numerator and denominator; texts without words report 0.0. Each
/// word also counts through its alphanumeric fold, so zero-width and
/// punctuation noise (`co\u{200b}de`) does not fake invalidity.
fn lexicon_coverage(
    raw: &str,
    tokens: &[String],
    decoded_top: Option<&str>,
    provider: &dyn LexiconProvider,
) -> f64 {
    let decoded_words: Vec<&str>;
    let words: Vec<&str> = match decoded_top {
        Some(top) if alphanumeric_fold(top) != alphanumeric_fold(raw) => {
            decoded_words = top
                .split_whitespace()
                .filter(|token| token.chars().any(|ch| ch.is_alphabetic()))
                .collect();
            decoded_words
        }
        _ => tokens
            .iter()
            .map(String::as_str)
            .filter(|token| token.chars().any(|ch| ch.is_alphabetic()))
            .collect(),
    };
    if words.is_empty() {
        return 0.0;
    }
    let known = words
        .iter()
        .filter(|token| {
            provider.contains(token, None) || provider.contains(&alphanumeric_fold(token), None)
        })
        .count();
    (known as f64 / words.len() as f64).clamp(0.0, 1.0)
}

fn alphanumeric_fold(text: &str) -> String {
    crate::normalization::unicode::casefold_text(text)
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .collect()
}

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
    reranker_provider: Option<Arc<dyn RerankerProvider>>,
    spam_predictor: Arc<dyn SpamPredictor>,
    similarity_scorer: Option<Arc<dyn SimilarityScorer>>,
    similarity_profile: Option<SimilarityProfile>,
    preset_fallbacks: Vec<DegradedCapability>,
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
            abbreviation_provider: Some(resources),
            transliteration_provider: Some(Arc::new(RuleBasedTransliterationProvider)),
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

    /// Semantic-rescoring marker for rebus cache keys: `sem:off` when
    /// rescoring is disabled, otherwise the embedding model identity so model
    /// swaps key separately.
    fn rebus_semantic_marker(&self) -> String {
        if !self.config.semantic {
            return "sem:off".to_string();
        }
        if let Some(metadata) = self.embedding_provider.model_metadata() {
            return format!(
                "sem:{}@{}#{}",
                metadata.model_id,
                metadata.revision.as_deref().unwrap_or("-"),
                metadata.dimensions
            );
        }
        let capabilities = self.embedding_provider.capabilities();
        format!(
            "sem:{}@{}#{}",
            capabilities.provider,
            capabilities.model_revision.as_deref().unwrap_or("-"),
            capabilities.dimensions.unwrap_or(0)
        )
    }

    fn rebus_cache_lookup(
        &self,
        text: &str,
        languages: Option<&[String]>,
        max_candidates: Option<usize>,
        semantic: &str,
    ) -> Option<Vec<DecodedCandidate>> {
        let cache = self.rebus_cache.as_ref()?;
        let key = rebus_cache_key(
            text,
            languages,
            max_candidates,
            &self.config.rebus_weights,
            semantic,
        );
        cache.lock().ok()?.get(&key)
    }

    fn rebus_cache_store(
        &self,
        text: &str,
        languages: Option<&[String]>,
        max_candidates: Option<usize>,
        semantic: &str,
        candidates: Vec<DecodedCandidate>,
    ) {
        let Some(cache) = self.rebus_cache.as_ref() else {
            return;
        };
        let key = rebus_cache_key(
            text,
            languages,
            max_candidates,
            &self.config.rebus_weights,
            semantic,
        );
        if let Ok(mut guard) = cache.lock() {
            guard.put(key, candidates);
        }
    }

    /// Observable rebus-cache state (counts only, never cached texts).
    fn rebus_cache_diagnostics(&self) -> CacheDiagnostics {
        self.rebus_cache
            .as_ref()
            .and_then(|cache| cache.lock().ok().map(|guard| guard.diagnostics()))
            .unwrap_or_else(CacheDiagnostics::disabled)
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
            let store = match builder.json_store_ann {
                Some((dimensions, max_elements)) => {
                    JsonFileStore::open_with_ann(path, dimensions, max_elements)
                }
                None => JsonFileStore::open(path),
            }
            .map_err(TextIntelError::Storage)?;
            #[cfg(not(feature = "ann-hnsw"))]
            let store = {
                if builder.json_store_ann.is_some() {
                    return Err(TextIntelError::InvalidConfiguration(
                        "json_store_with_ann requires the ann-hnsw feature".to_string(),
                    ));
                }
                JsonFileStore::open(path).map_err(TextIntelError::Storage)?
            };
            engine.store = RwLock::new(Box::new(store));
        }
        // Builder-supplied providers replaced the from_parts defaults above;
        // wrap them when the configured cache limits enable caching.
        engine.install_caches();
        Ok(engine)
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    /// Stable diagnostics for deployments. Capability inspection is local and
    /// never performs network or model loading work.
    pub fn provider_capabilities(&self) -> BTreeMap<String, ProviderCapabilities> {
        let mut capabilities = BTreeMap::new();
        capabilities.insert(
            "embedding".to_string(),
            self.embedding_provider.capabilities(),
        );
        capabilities.insert("g2p".to_string(), self.g2p_provider.capabilities());
        capabilities.insert(
            "language".to_string(),
            self.language_provider.capabilities(),
        );
        capabilities.insert("lexicon".to_string(), self.lexicon_provider.capabilities());
        capabilities.insert("symbols".to_string(), self.symbol_provider.capabilities());
        if let Some(abbreviations) = &self.abbreviation_provider {
            capabilities.insert("abbreviations".to_string(), abbreviations.capabilities());
        }
        if let Some(transliteration) = &self.transliteration_provider {
            capabilities.insert(
                "transliteration".to_string(),
                transliteration.capabilities(),
            );
        }
        capabilities.insert("spam".to_string(), self.spam_predictor.capabilities());
        capabilities.insert(
            "store".to_string(),
            self.store
                .read()
                .map(|store| store.capabilities())
                .unwrap_or_else(|_| {
                    ProviderCapabilities::new("store:unavailable")
                        .with_quality(crate::core::capabilities::CapabilityLevel::Unavailable)
                }),
        );
        if let Some(reranker) = &self.reranker_provider {
            capabilities.insert("reranker".to_string(), reranker.capabilities());
        }
        if let Some(scorer) = &self.similarity_scorer {
            capabilities.insert("similarity".to_string(), scorer.capabilities());
        }
        capabilities
    }

    pub fn health_check(&self) -> Result<(), TextIntelError> {
        self.embedding_provider
            .health_check()
            .map_err(TextIntelError::from)
    }

    /// Provenance for every loaded linguistic resource pack (source, license,
    /// revision, hash when declared).
    pub fn resource_manifest(&self) -> Vec<crate::resources::ResourcePackInfo> {
        self.resources.manifest().to_vec()
    }

    /// Deployment and reproducibility report: provider lineup, resource
    /// revisions, and every capability serving below production quality.
    /// Inspection is local and never performs network or model work.
    pub fn diagnostics(&self) -> EngineDiagnostics {
        let capabilities = self.provider_capabilities();
        let get = |name: &str| {
            capabilities
                .get(name)
                .cloned()
                .unwrap_or_else(|| ProviderCapabilities::new(format!("{name}:unknown")))
        };
        let symbol_languages = self.resources.symbol_pack_languages();
        // ANN availability comes from explicit store capabilities, never from
        // the provider name: a `MemoryStore` may serve HNSW internally while
        // reporting a plain `memory_store` provider.
        let store_capabilities = self
            .store
            .read()
            .map(|store| store.store_capabilities())
            .unwrap_or_default();
        EngineDiagnostics {
            api_version: env!("CARGO_PKG_VERSION").to_string(),
            fingerprint_schema: crate::core::types::FINGERPRINT_SCHEMA_VERSION,
            resource_languages: self.resources.languages(),
            abbreviation_languages: self.resources.abbreviation_languages(),
            symbol_languages,
            symbol_tokens: self.resources.symbol_count(),
            embedding: get("embedding"),
            g2p: get("g2p"),
            language: get("language"),
            lexicon: get("lexicon"),
            symbols: get("symbols"),
            abbreviations: capabilities.get("abbreviations").cloned(),
            transliteration: capabilities.get("transliteration").cloned(),
            spam: get("spam"),
            similarity: capabilities.get("similarity").cloned(),
            reranker: capabilities.get("reranker").cloned(),
            store: get("store"),
            ann_enabled: store_capabilities.ann_enabled,
            degraded: self.degraded_capabilities(&capabilities),
            resource_manifest: self.resource_manifest(),
            caches: self.cache_diagnostics(),
            store_capabilities,
        }
    }

    /// Bounded revision-aware cache state by subsystem. Counts only — never
    /// cached texts. Uncached subsystems report `enabled: false`.
    fn cache_diagnostics(&self) -> BTreeMap<String, CacheDiagnostics> {
        BTreeMap::from([
            (
                "embeddings".to_string(),
                self.embedding_provider
                    .cache_diagnostics()
                    .unwrap_or_else(CacheDiagnostics::disabled),
            ),
            (
                "g2p".to_string(),
                self.g2p_provider
                    .cache_diagnostics()
                    .unwrap_or_else(CacheDiagnostics::disabled),
            ),
            (
                "language".to_string(),
                self.language_provider
                    .cache_diagnostics()
                    .unwrap_or_else(CacheDiagnostics::disabled),
            ),
            ("rebus".to_string(), self.rebus_cache_diagnostics()),
        ])
    }

    fn degraded_capabilities(
        &self,
        capabilities: &BTreeMap<String, ProviderCapabilities>,
    ) -> Vec<DegradedCapability> {
        use crate::core::capabilities::CapabilityLevel;
        let store = self
            .store
            .read()
            .map(|store| store.store_capabilities())
            .unwrap_or_default();
        let mut degraded = Vec::new();
        let mut note = |capability: &str, configured: &str, wanted: &str, detail: &str| {
            degraded.push(DegradedCapability {
                capability: capability.to_string(),
                configured: configured.to_string(),
                wanted: wanted.to_string(),
                detail: detail.to_string(),
            });
        };
        match capabilities.get("embedding").map(|info| info.quality) {
            Some(CapabilityLevel::Production) => {}
            Some(CapabilityLevel::Basic) => note(
                "semantic",
                &capabilities["embedding"].provider,
                "multilingual transformer embeddings",
                "serving a Basic fallback; configure a transformer provider for production quality",
            ),
            _ => note(
                "semantic",
                "none",
                "multilingual transformer embeddings",
                "no embedding backend configured; semantic channel is skipped and weights renormalize",
            ),
        }
        match capabilities.get("g2p").map(|info| info.provider.as_str()) {
            Some("espeak_ng_g2p") => {}
            Some(name) => note(
                "phonetic",
                name,
                "espeak_ng_g2p",
                "install espeak-ng and use EspeakNgG2PProvider::auto_detect for production phonetics",
            ),
            None => note("phonetic", "none", "espeak_ng_g2p", "no G2P backend configured"),
        }
        if !capabilities.contains_key("similarity") {
            note(
                "similarity",
                "weighted deterministic scorer",
                "trained similarity model",
                "load models/similarity-v4.json through trained_similarity_model() for calibrated scoring",
            );
        }
        if capabilities
            .get("spam")
            .map(|info| info.provider.starts_with("heuristic"))
            .unwrap_or(true)
        {
            note(
                "spam",
                "heuristic",
                "trained spam model",
                "load models/spam-v1.json through trained_spam_model() for the trained predictor",
            );
        }
        if !capabilities.contains_key("reranker") {
            note(
                "reranker",
                "disabled",
                "channel-score reranker",
                "full-comparison order is final; configure a RerankerProvider to rescore top candidates",
            );
        }
        if !store.persistent {
            note(
                "persistence",
                "in-memory",
                "local JSON or redb store",
                "fingerprints do not survive restarts; use json_store() or a persistent VectorStore",
            );
        }
        if !store.ann_enabled {
            note(
                "retrieval",
                "index scan",
                "HNSW ANN (ann-hnsw feature)",
                "large stores compare exhaustively; enable the ANN index for sublinear retrieval",
            );
        }
        degraded.extend(self.preset_fallbacks.clone());
        degraded
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

    fn score_pair(
        &self,
        left: &MessageFingerprint,
        right: &MessageFingerprint,
    ) -> ComparisonResult {
        if let Some(scorer) = &self.similarity_scorer {
            scorer.score(left, right)
        } else if let Some(profile) = &self.similarity_profile {
            score_fingerprints_with_profile(left, right, profile)
        } else {
            weighted_score_fingerprints(left, right, &self.config.similarity_weights)
        }
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

    fn check_length(&self, text: &str) -> Result<(), TextIntelError> {
        let length = text.chars().count();
        if length > self.config.max_input_length {
            return Err(TextIntelError::InputTooLong {
                length,
                maximum: self.config.max_input_length,
            });
        }
        Ok(())
    }

    fn detect(
        &self,
        text: &str,
    ) -> Result<Vec<crate::core::types::LanguageCandidate>, TextIntelError> {
        self.language_provider
            .detect(text)
            .map_err(TextIntelError::from)
    }

    fn build_phonetic_candidates(
        &self,
        raw: &str,
        languages: &[crate::core::types::LanguageCandidate],
        rebus: &[DecodedCandidate],
    ) -> Result<Vec<PhoneticCandidate>, TextIntelError> {
        if !self.config.phonetic {
            return Ok(Vec::new());
        }
        let mut values = Vec::new();
        let mut seen = BTreeSet::new();
        // Configured hints win over detection for G2P voice selection, then
        // detected languages fill the remaining slots.
        let mut language_list: Vec<String> = self
            .config
            .language_hints
            .iter()
            .filter(|hint| hint.as_str() != "unknown")
            .cloned()
            .collect();
        language_list.extend(
            languages
                .iter()
                .filter(|candidate| candidate.language != "unknown")
                .map(|candidate| candidate.language.clone()),
        );
        language_list.dedup();
        language_list.truncate(3);
        let language_list = if language_list.is_empty() {
            vec!["und".to_string()]
        } else {
            language_list
        };
        for language in &language_list {
            let mut texts = vec![raw.to_string()];
            texts.extend(rebus.iter().take(3).map(|decoded| decoded.text.clone()));
            let candidates = self
                .g2p_provider
                .phonemize_batch(&texts, language)
                .map_err(TextIntelError::from)?;
            for candidate in candidates {
                if seen.insert((candidate.source.clone(), candidate.language.clone())) {
                    values.push(candidate);
                }
            }
        }
        Ok(values)
    }

    fn analyze_base(
        &self,
        text: &str,
    ) -> Result<(MessageFingerprint, Vec<EmbeddingInput>), TextIntelError> {
        let (fingerprint, inputs, _) = self.analyze_stages(text)?;
        Ok((fingerprint, inputs))
    }

    /// [`analyze_base`](Self::analyze) plus per-stage timings. Durations only;
    /// no text or vectors ever enter [`StageTimings`].
    fn analyze_stages(
        &self,
        text: &str,
    ) -> Result<(MessageFingerprint, Vec<EmbeddingInput>, StageTimings), TextIntelError> {
        self.check_length(text)?;
        let total_started = Instant::now();
        let mut timings = StageTimings::default();
        let elapsed = |started: Instant| started.elapsed().as_secs_f64() * 1_000_000.0;

        let started = Instant::now();
        let unicode = analyze_unicode(text);
        timings.normalization_micros += elapsed(started);

        let started = Instant::now();
        let languages = self.detect(text)?;
        let language_names: Vec<String> = languages
            .iter()
            .map(|candidate| candidate.language.clone())
            .collect();
        let segments = segment_message_with_provider(
            text,
            self.config.max_segments,
            self.language_provider.as_ref(),
        )
        .map_err(TextIntelError::from)?;
        let tokens = tokenize(text);
        let lemmas = match &self.lemmatizer_provider {
            Some(provider) => provider
                .lemmatize(&tokens, None)
                .map_err(TextIntelError::from)?,
            None => simple_lemmas_with_provider(
                &tokens,
                Some(&language_names),
                self.lexicon_provider.as_ref(),
            ),
        };
        let lexical = LexicalFeatures {
            tokens: tokens.clone(),
            lemmas: lemmas.clone(),
            stop_words: stop_words_with_provider(
                &tokens,
                Some(&language_names),
                self.lexicon_provider.as_ref(),
            ),
            word_ngrams: word_ngrams(&tokens, 2),
            token_ngrams: word_ngrams(&tokens, 3),
            jaccard_ready: lemmas.iter().cloned().collect(),
            simhash: simhash(&lemmas, 64),
            minhash: minhash_signature(&lemmas, 32),
        };
        timings.language_micros += elapsed(started);

        let started = Instant::now();
        let symbols = resolve_symbols_with_provider(
            text,
            self.config.max_symbol_readings,
            self.symbol_provider.as_ref(),
        );
        timings.symbols_micros += elapsed(started);

        let started = Instant::now();
        let obfuscation = obfuscation_features(text, &unicode);
        timings.normalization_micros += elapsed(started);

        // Configured hints override detected languages for decoding; hints
        // never change detection itself. Otherwise scope follows detection,
        // ranked best-first: the long tail is noise that crowds the true
        // language's readings out of the beam (`I ❤ NY` decoded to Hindi
        // before English because eight tail languages outranked it), so
        // scope stops at the three strongest substantive hypotheses, which
        // covers monolingual and code-switched text. Two honest fallbacks:
        // when nothing substantive was detected the remainder is empty and
        // un-scopes decoding (the tokenizer treats empty as "all allowed"),
        // and a single segment topped by `unknown` also decodes globally —
        // with no confident detection and no surrounding context, scoping by
        // the tail is pure gamble (short slang like `luv` detects as
        // [unknown, fr, es, ...], cutting the true language's readings),
        // while globally the true reading wins on its own probability.
        // NOTE: multi-segment unknown-topped texts (obfuscated `ch34p`,
        // digit-heavy `2nite`) keep tail scoping: neutral decoding lets
        // high-prior foreign number readings (`2` → `dos`) crowd out the
        // intended reading, which scores worse than the noisy scope. The
        // principled fix (validity-gated scoping plus calibrated
        // cross-language priors) is future work; see the gap analysis.
        let decode_languages: Vec<String> = if self.config.language_hints.is_empty() {
            let top_unknown = language_names
                .first()
                .is_some_and(|top| top.eq_ignore_ascii_case("unknown"));
            if top_unknown && segments.len() == 1 {
                Vec::new()
            } else {
                language_names
                    .iter()
                    .filter(|name| !name.eq_ignore_ascii_case("unknown"))
                    .take(3)
                    .cloned()
                    .collect()
            }
        } else {
            self.config.language_hints.clone()
        };
        let started = Instant::now();
        // Fingerprint rebus decoding never uses semantic rescoring (the
        // `sem:off` key marker); `decode_with_languages` may, and keys
        // separately.
        let rebus = match self.rebus_cache_lookup(
            text,
            Some(&decode_languages),
            Some(self.config.max_candidates),
            "sem:off",
        ) {
            Some(hit) => hit,
            None => {
                let decoder = RebusDecoder::new(self.config.clone());
                let abbreviations = self.abbreviation_provider.as_deref();
                let decoded = decoder.decode_with_abbreviations(
                    text,
                    Some(&decode_languages),
                    Some(self.config.max_candidates),
                    self.symbol_provider.as_ref(),
                    self.lexicon_provider.as_ref(),
                    self.g2p_provider.as_ref(),
                    None,
                    abbreviations,
                );
                self.rebus_cache_store(
                    text,
                    Some(&decode_languages),
                    Some(self.config.max_candidates),
                    "sem:off",
                    decoded.clone(),
                );
                decoded
            }
        };
        let spoken_candidates = rebus
            .iter()
            .map(|candidate| SpokenCandidate {
                text: candidate.text.clone(),
                language: candidate.language.clone(),
                probability: candidate.score,
                source_transformations: candidate.transformations.clone(),
                confidence: candidate.score,
                source: "rebus".to_string(),
            })
            .collect::<Vec<_>>();
        timings.rebus_micros += elapsed(started);

        let semantic_embeddings = BTreeMap::new();
        let started = Instant::now();
        let phonetic_candidates = self.build_phonetic_candidates(text, &languages, &rebus)?;
        timings.phonetic_micros += elapsed(started);

        let started = Instant::now();
        let normalized = normalize_whitespace(&collapse_repetition(
            &apply_leet(&casefold_text(&nfkc(text))),
            self.config.repetition_keep,
        ));
        let mut normalization_views = BTreeMap::new();
        normalization_views.insert("nfc".to_string(), unicode.nfc.clone());
        normalization_views.insert("nfkc".to_string(), unicode.nfkc.clone());
        normalization_views.insert("casefold".to_string(), unicode.casefolded.clone());
        normalization_views.insert("leet".to_string(), apply_leet(&unicode.casefolded));
        normalization_views.insert(
            "repetition_collapsed".to_string(),
            collapse_repetition(&unicode.casefolded, self.config.repetition_keep),
        );
        normalization_views.insert("normalized".to_string(), normalized.clone());
        // Transliteration views are additive: `raw` is never replaced. Each
        // view records its provider confidence so scoring can weight the
        // conversion instead of trusting it unconditionally.
        let mut transliteration_confidence = BTreeMap::new();
        if let Some(provider) = &self.transliteration_provider {
            for view in provider.transliterate(text) {
                let name = format!("transliteration:{}", view.target_script.to_lowercase());
                transliteration_confidence.insert(name.clone(), view.confidence);
                normalization_views.insert(name, view.text);
            }
        }
        let transformations = normalization_views
            .iter()
            .filter(|(name, value)| value.as_str() != text && name.as_str() != "normalized")
            .map(|(name, value)| {
                let mut step =
                    Transformation::new(text, value.clone(), format!("normalization:{name}"))
                        .with_span(0, text.len(), text);
                if name.starts_with("transliteration:") {
                    step = step.with_provider("transliteration");
                    if let Some(confidence) = transliteration_confidence.get(name) {
                        step = step.with_confidence(*confidence);
                    }
                } else {
                    step = step.with_provider("normalization");
                }
                step
            })
            .collect();
        timings.normalization_micros += elapsed(started);
        let mut channel_availability = BTreeMap::new();
        channel_availability.insert(
            "lexical".to_string(),
            ChannelAvailability::available("resource_index", 0.85),
        );
        channel_availability.insert(
            "character".to_string(),
            ChannelAvailability::available("deterministic", 1.0),
        );
        channel_availability.insert(
            "visual".to_string(),
            ChannelAvailability::available("unicode_analysis", 0.9),
        );
        channel_availability.insert(
            "symbolic".to_string(),
            if symbols.is_empty() {
                ChannelAvailability::unavailable("symbol_index")
            } else {
                ChannelAvailability::available("symbol_index", 0.75)
            },
        );
        channel_availability.insert(
            "decoded".to_string(),
            ChannelAvailability::available("bounded_beam_search", 0.7),
        );
        channel_availability.insert(
            "obfuscation".to_string(),
            ChannelAvailability::available("deterministic", 0.9),
        );
        channel_availability.insert(
            "semantic".to_string(),
            if semantic_embeddings.is_empty() {
                ChannelAvailability::unavailable("embedding_provider")
            } else {
                ChannelAvailability::available("embedding_provider", 0.7)
            },
        );
        channel_availability.insert(
            "phonetic".to_string(),
            if phonetic_candidates.is_empty() {
                ChannelAvailability::unavailable("g2p_provider")
            } else {
                ChannelAvailability::available("g2p_provider", 0.65)
            },
        );
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "fingerprint_schema_version".to_string(),
            crate::core::types::FINGERPRINT_SCHEMA_VERSION.to_string(),
        );
        metadata.insert(
            "semantic_enabled".to_string(),
            self.config.semantic.to_string(),
        );
        metadata.insert(
            "phonetic_enabled".to_string(),
            self.config.phonetic.to_string(),
        );
        metadata.insert("input_length".to_string(), text.chars().count().to_string());
        // Coverage is computed after decoding so validity reflects the
        // intended reading (`h3llo` counts through `hello`), not the raw
        // obfuscation. See `lexicon_coverage`.
        let lexicon_coverage = lexicon_coverage(
            text,
            &tokens,
            rebus.first().map(|candidate| candidate.text.as_str()),
            self.lexicon_provider.as_ref(),
        );
        let embedding_inputs = if self.config.semantic {
            let mut values = vec![EmbeddingInput {
                key: "default".to_string(),
                text: text.to_string(),
            }];
            values.extend(rebus.iter().take(3).enumerate().map(|(index, candidate)| {
                EmbeddingInput {
                    key: format!("decoded:{}", index + 1),
                    text: candidate.text.clone(),
                }
            }));
            values.extend(
                segments
                    .iter()
                    .filter(|segment| segment.segment_type == "text")
                    .take(16)
                    .enumerate()
                    .map(|(index, segment)| EmbeddingInput {
                        key: format!("segment:{index}"),
                        text: segment.text.clone(),
                    }),
            );
            values
        } else {
            Vec::new()
        };
        let fingerprint = MessageFingerprint {
            schema_version: crate::core::types::FINGERPRINT_SCHEMA_VERSION,
            raw: text.to_string(),
            normalized: Some(normalized),
            normalization_views,
            transliteration_confidence,
            lexicon_coverage,
            transformations,
            language_candidates: languages,
            segments,
            tokens,
            lemmas,
            char_features: char_features(text),
            unicode_features: unicode,
            symbols,
            lexical_features: lexical,
            semantic_embeddings,
            spoken_candidates,
            phonetic_candidates,
            rebus_candidates: rebus,
            obfuscation_features: obfuscation,
            channel_availability,
            metadata,
        };
        timings.total_micros = total_started.elapsed().as_secs_f64() * 1_000_000.0;
        Ok((fingerprint, embedding_inputs, timings))
    }

    fn attach_embeddings(
        &self,
        fingerprint: &mut MessageFingerprint,
        embedding_inputs: &[EmbeddingInput],
        values: Vec<Vec<f32>>,
    ) -> Result<(), TextIntelError> {
        if values.len() != embedding_inputs.len() {
            return Err(TextIntelError::Provider(ProviderError::new(
                "embedding",
                format!(
                    "returned {} vectors for {} inputs",
                    values.len(),
                    embedding_inputs.len()
                ),
            )));
        }
        for (input, vector) in embedding_inputs.iter().zip(values) {
            if vector.iter().any(|value| !value.is_finite()) {
                return Err(TextIntelError::Provider(ProviderError::new(
                    "embedding",
                    "returned a non-finite vector",
                )));
            }
            if let Some(metadata) = self.embedding_provider.model_metadata() {
                metadata.validate_vector(&vector).map_err(|message| {
                    TextIntelError::Provider(ProviderError::new("embedding", message))
                })?;
            }
            if !vector.is_empty() {
                fingerprint
                    .semantic_embeddings
                    .insert(input.key.clone(), vector);
            }
        }
        let availability = fingerprint
            .channel_availability
            .entry("semantic".to_string())
            .or_insert_with(|| ChannelAvailability::unavailable("embedding_provider"));
        if !fingerprint.semantic_embeddings.is_empty() {
            *availability = ChannelAvailability::available("embedding_provider", 0.7);
        }
        fingerprint.metadata.insert(
            "semantic_vectors".to_string(),
            fingerprint.semantic_embeddings.len().to_string(),
        );
        if let Some(metadata) = self.embedding_provider.model_metadata() {
            fingerprint
                .metadata
                .insert("semantic_model".to_string(), metadata.model_id.clone());
            fingerprint.metadata.insert(
                "semantic_dimensions".to_string(),
                metadata.dimensions.to_string(),
            );
            if let Some(revision) = metadata.revision {
                fingerprint
                    .metadata
                    .insert("semantic_revision".to_string(), revision);
            }
        }
        Ok(())
    }

    pub fn analyze(&self, text: &str) -> Result<MessageFingerprint, TextIntelError> {
        Ok(self.analyze_with_timing(text)?.0)
    }

    /// [`analyze`](Self::analyze) plus per-stage timings. The timings carry
    /// durations only — no input text, embeddings, or user data — so they are
    /// safe to log and export by default.
    pub fn analyze_with_timing(
        &self,
        text: &str,
    ) -> Result<(MessageFingerprint, StageTimings), TextIntelError> {
        let total_started = Instant::now();
        let (mut fingerprint, embedding_inputs, mut timings) = self.analyze_stages(text)?;
        if self.config.semantic {
            let started = Instant::now();
            let values = self
                .embedding_provider
                .embed_batch(
                    &embedding_inputs
                        .iter()
                        .map(|input| input.text.clone())
                        .collect::<Vec<_>>(),
                )
                .map_err(TextIntelError::from)?;
            self.attach_embeddings(&mut fingerprint, &embedding_inputs, values)?;
            timings.semantic_micros = started.elapsed().as_secs_f64() * 1_000_000.0;
        }
        timings.total_micros = total_started.elapsed().as_secs_f64() * 1_000_000.0;
        Ok((fingerprint, timings))
    }

    pub fn analyze_batch(
        &self,
        texts: &[String],
    ) -> Result<Vec<MessageFingerprint>, TextIntelError> {
        if texts.len() > self.config.max_batch_size {
            return Err(TextIntelError::InvalidConfiguration(format!(
                "batch size {} exceeds max_batch_size={}",
                texts.len(),
                self.config.max_batch_size
            )));
        }
        let mut bases = Vec::with_capacity(texts.len());
        let mut all_embedding_inputs = Vec::new();
        for text in texts {
            let (fingerprint, embedding_texts) = self.analyze_base(text)?;
            all_embedding_inputs.extend(embedding_texts);
            bases.push((fingerprint, all_embedding_inputs.len()));
        }
        if !self.config.semantic {
            return Ok(bases
                .into_iter()
                .map(|(fingerprint, _)| fingerprint)
                .collect());
        }
        let all_embedding_texts = all_embedding_inputs
            .iter()
            .map(|input| input.text.clone())
            .collect::<Vec<_>>();
        let values = self
            .embedding_provider
            .embed_batch(&all_embedding_texts)
            .map_err(TextIntelError::from)?;
        if values.len() != all_embedding_texts.len() {
            return Err(TextIntelError::Provider(ProviderError::new(
                "embedding",
                format!(
                    "returned {} vectors for {} batched inputs",
                    values.len(),
                    all_embedding_texts.len()
                ),
            )));
        }
        let mut output = Vec::with_capacity(bases.len());
        let mut offset = 0usize;
        for (mut fingerprint, end) in bases {
            let count = end.saturating_sub(offset);
            let inputs = &all_embedding_inputs[offset..end];
            let vectors = values[offset..end].to_vec();
            self.attach_embeddings(&mut fingerprint, inputs, vectors)?;
            output.push(fingerprint);
            offset += count;
        }
        Ok(output)
    }

    pub fn compare_batch(
        &self,
        pairs: &[(String, String)],
    ) -> Result<Vec<ComparisonResult>, TextIntelError> {
        if pairs.len() > self.config.max_batch_size {
            return Err(TextIntelError::InvalidConfiguration(format!(
                "batch size {} exceeds max_batch_size={}",
                pairs.len(),
                self.config.max_batch_size
            )));
        }
        let texts = pairs
            .iter()
            .flat_map(|(left, right)| [left.clone(), right.clone()])
            .collect::<Vec<_>>();
        let fingerprints = self.analyze_batch(&texts)?;
        pairs
            .iter()
            .enumerate()
            .map(|(index, _)| {
                Ok(self.score_pair(&fingerprints[index * 2], &fingerprints[index * 2 + 1]))
            })
            .collect()
    }

    pub fn decode(&self, text: &str) -> Result<Vec<DecodedCandidate>, TextIntelError> {
        self.decode_with_languages(text, None, None)
    }

    pub fn decode_with_languages(
        &self,
        text: &str,
        languages: Option<&[String]>,
        max_candidates: Option<usize>,
    ) -> Result<Vec<DecodedCandidate>, TextIntelError> {
        self.check_length(text)?;
        let decoder = RebusDecoder::new(self.config.clone());
        // Semantic rescoring only when an embedding backend is configured;
        // the default null backend yields no vectors and stays free.
        let semantic;
        let semantic_ref: Option<&crate::rebus::SemanticEvidence> = if self.config.semantic {
            let provider = self.embedding_provider.clone();
            semantic = move |surface: &str, source: &str| -> Option<f64> {
                let vectors = provider
                    .embed(&[surface.to_string(), source.to_string()])
                    .ok()?;
                let (left, right) = (vectors.first()?, vectors.get(1)?);
                if left.is_empty() || right.is_empty() {
                    return None;
                }
                Some(crate::semantic::similarity::cosine(left, right))
            };
            Some(&semantic)
        } else {
            None
        };
        // Explicit languages win; configured hints fill in when the caller
        // passes none; otherwise the decoder runs language-neutral.
        let effective = languages.or(if self.config.language_hints.is_empty() {
            None
        } else {
            Some(self.config.language_hints.as_slice())
        });
        let marker = self.rebus_semantic_marker();
        if let Some(hit) = self.rebus_cache_lookup(text, effective, max_candidates, &marker) {
            return Ok(hit);
        }
        let decoded = decoder.decode_with_abbreviations(
            text,
            effective,
            max_candidates,
            self.symbol_provider.as_ref(),
            self.lexicon_provider.as_ref(),
            self.g2p_provider.as_ref(),
            semantic_ref,
            self.abbreviation_provider.as_deref(),
        );
        self.rebus_cache_store(text, effective, max_candidates, &marker, decoded.clone());
        Ok(decoded)
    }

    pub fn compare(&self, left: &str, right: &str) -> Result<ComparisonResult, TextIntelError> {
        Ok(self.compare_with_timing(left, right)?.0)
    }

    /// [`compare`](Self::compare) plus per-stage timings. The two `analyze`
    /// stages are summed per stage; `comparison` holds the scoring step and
    /// `total` the whole call. Timings never contain user text.
    pub fn compare_with_timing(
        &self,
        left: &str,
        right: &str,
    ) -> Result<(ComparisonResult, StageTimings), TextIntelError> {
        let total_started = Instant::now();
        let (left_fp, left_timings) = self.analyze_with_timing(left)?;
        let (right_fp, right_timings) = self.analyze_with_timing(right)?;
        let started = Instant::now();
        let result = self.score_pair(&left_fp, &right_fp);
        let mut timings = StageTimings {
            normalization_micros: left_timings.normalization_micros
                + right_timings.normalization_micros,
            language_micros: left_timings.language_micros + right_timings.language_micros,
            symbols_micros: left_timings.symbols_micros + right_timings.symbols_micros,
            rebus_micros: left_timings.rebus_micros + right_timings.rebus_micros,
            semantic_micros: left_timings.semantic_micros + right_timings.semantic_micros,
            phonetic_micros: left_timings.phonetic_micros + right_timings.phonetic_micros,
            comparison_micros: started.elapsed().as_secs_f64() * 1_000_000.0,
            total_micros: 0.0,
        };
        timings.total_micros = total_started.elapsed().as_secs_f64() * 1_000_000.0;
        Ok((result, timings))
    }

    pub fn compare_fingerprints(
        &self,
        left: &MessageFingerprint,
        right: &MessageFingerprint,
    ) -> ComparisonResult {
        self.score_pair(left, right)
    }

    pub fn add_pattern(
        &self,
        id: impl Into<String>,
        examples: Vec<String>,
    ) -> Result<(), TextIntelError> {
        let id = id.into();
        self.add_pattern_with_options(Pattern {
            id,
            examples,
            negative_examples: Vec::new(),
            threshold: 0.75,
            languages: Vec::new(),
            tags: Vec::new(),
            enabled_channels: Vec::new(),
        })
    }

    pub fn add_pattern_with_options(&self, pattern: Pattern) -> Result<(), TextIntelError> {
        if pattern.id.trim().is_empty() || pattern.examples.is_empty() {
            return Err(TextIntelError::InvalidConfiguration(
                "pattern id and examples are required".to_string(),
            ));
        }
        if !pattern.threshold.is_finite() || !(0.0..=1.0).contains(&pattern.threshold) {
            return Err(TextIntelError::InvalidConfiguration(
                "pattern threshold must be between 0 and 1".to_string(),
            ));
        }
        let id = pattern.id.clone();
        let examples = pattern.examples.clone();
        let mut analyzed = Vec::with_capacity(examples.len());
        for example in &examples {
            analyzed.push((example.clone(), self.analyze(example)?));
        }
        let pattern = RegisteredPattern {
            pattern,
            examples: analyzed,
        };
        self.patterns
            .write()
            .map_err(|_| TextIntelError::Storage("pattern lock poisoned".to_string()))?
            .insert(id, pattern);
        Ok(())
    }

    pub fn remove_pattern(&self, id: &str) -> Result<bool, TextIntelError> {
        Ok(self
            .patterns
            .write()
            .map_err(|_| TextIntelError::Storage("pattern lock poisoned".to_string()))?
            .remove(id)
            .is_some())
    }

    /// Snapshot the registered pattern definitions (without analyzed
    /// fingerprints) for persistence or inspection.
    pub fn pattern_definitions(&self) -> Result<Vec<Pattern>, TextIntelError> {
        let patterns = self
            .patterns
            .read()
            .map_err(|_| TextIntelError::Storage("pattern lock poisoned".to_string()))?;
        Ok(patterns
            .values()
            .map(|registered| registered.pattern.clone())
            .collect())
    }

    /// Persist registered pattern definitions to `path` in a versioned
    /// envelope. Example fingerprints are re-analyzed on load, so the file
    /// stays valid across fingerprint schema upgrades.
    pub fn save_patterns_to(&self, path: impl AsRef<Path>) -> Result<(), TextIntelError> {
        let patterns = self.pattern_definitions()?;
        crate::storage::patterns::save_patterns_to(path.as_ref(), &patterns)
            .map_err(TextIntelError::Storage)
    }

    /// Load pattern definitions from `path`, validating and re-analyzing
    /// every record. Returns the number of patterns registered.
    pub fn load_patterns_from(&self, path: impl AsRef<Path>) -> Result<usize, TextIntelError> {
        let patterns = crate::storage::patterns::load_patterns_from(path.as_ref())
            .map_err(TextIntelError::Storage)?;
        let count = patterns.len();
        for pattern in patterns {
            self.add_pattern_with_options(pattern)?;
        }
        Ok(count)
    }

    pub fn match_patterns(&self, text: &str) -> Result<Vec<PatternMatch>, TextIntelError> {
        let query = self.analyze(text)?;
        let patterns = self
            .patterns
            .read()
            .map_err(|_| TextIntelError::Storage("pattern lock poisoned".to_string()))?;
        let mut matches = patterns
            .values()
            .filter_map(|registered| {
                match_pattern_fingerprint(
                    &query,
                    &registered.pattern,
                    &registered.examples,
                    &self.config.similarity_weights,
                )
            })
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| right.score.total_cmp(&left.score));
        Ok(matches)
    }

    pub fn detect_spam(
        &self,
        text: &str,
    ) -> Result<crate::core::types::SpamResult, TextIntelError> {
        let fingerprint = self.analyze(text)?;
        let patterns = self.match_patterns(text)?;
        self.spam_predictor
            .predict(&fingerprint, &patterns)
            .map_err(TextIntelError::from)
    }

    pub fn duplicate(
        &self,
        left: &str,
        right: &str,
        threshold: f64,
    ) -> Result<DuplicateResult, TextIntelError> {
        let left = self.analyze(left)?;
        let right = self.analyze(right)?;
        Ok(duplicate_result(
            &left,
            &right,
            threshold.clamp(0.0, 1.0),
            &self.config.similarity_weights,
        ))
    }

    pub fn duplicate_with_mode(
        &self,
        left: &str,
        right: &str,
        threshold: f64,
        mode: DuplicateMode,
    ) -> Result<DuplicateResult, TextIntelError> {
        let left = self.analyze(left)?;
        let right = self.analyze(right)?;
        Ok(duplicate_result_with_mode(
            &left,
            &right,
            threshold.clamp(0.0, 1.0),
            &self.config.similarity_weights,
            mode,
        ))
    }

    pub fn add_document(&self, id: impl Into<String>, text: &str) -> Result<(), TextIntelError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(TextIntelError::InvalidConfiguration(
                "document id cannot be empty".to_string(),
            ));
        }
        let fingerprint = self.analyze(text)?;
        let mut store = self
            .store
            .write()
            .map_err(|_| TextIntelError::Storage("store lock poisoned".to_string()))?;
        if store.len() >= self.config.max_documents
            && store.records().iter().all(|(current, _)| current != &id)
        {
            return Err(TextIntelError::Storage(format!(
                "max_documents={} reached",
                self.config.max_documents
            )));
        }
        store
            .upsert(id, fingerprint)
            .map_err(TextIntelError::Storage)
    }

    pub fn remove_document(&self, id: &str) -> Result<bool, TextIntelError> {
        self.store
            .write()
            .map_err(|_| TextIntelError::Storage("store lock poisoned".to_string()))?
            .remove(id)
            .map_err(TextIntelError::Storage)
    }

    pub fn document_count(&self) -> Result<usize, TextIntelError> {
        Ok(self
            .store
            .read()
            .map_err(|_| TextIntelError::Storage("store lock poisoned".to_string()))?
            .len())
    }

    pub fn find_similar(
        &self,
        text: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, TextIntelError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let query = self.analyze(text)?;
        let retrieved = self
            .store
            .read()
            .map_err(|_| TextIntelError::Storage("store lock poisoned".to_string()))?
            .search_candidates_with_metadata(&query, self.config.max_search_candidates.max(limit))
            .map_err(TextIntelError::Storage)?;
        let retrieval_channels = retrieved.channels;
        let records = retrieved.records;
        let mut candidates = records
            .into_iter()
            .map(|(id, fingerprint)| {
                let comparison = self.score_pair(&query, &fingerprint);
                (id, fingerprint, comparison)
            })
            .collect::<Vec<_>>();
        let candidate_count = candidates.len();
        candidates.sort_by(|left, right| right.2.score.total_cmp(&left.2.score));
        let results = if let Some(reranker) = &self.reranker_provider {
            let input = candidates
                .iter()
                .map(|(id, fingerprint, comparison)| {
                    (id.clone(), fingerprint.clone(), comparison.score)
                })
                .collect();
            let reranked = reranker
                .rerank(&query, input)
                .map_err(TextIntelError::from)?;
            let mut by_id = candidates
                .into_iter()
                .map(|(id, _, comparison)| (id, comparison))
                .collect::<BTreeMap<_, _>>();
            let mut reranked_results: Vec<(String, ComparisonResult)> = reranked
                .into_iter()
                .filter_map(|(id, _, score)| {
                    by_id.remove(&id).map(|mut comparison| {
                        comparison.score = score.clamp(0.0, 1.0);
                        (id, comparison)
                    })
                })
                .collect();
            reranked_results.sort_by(|left, right| right.1.score.total_cmp(&left.1.score));
            reranked_results
        } else {
            candidates
                .into_iter()
                .map(|(id, _, comparison)| (id, comparison))
                .collect()
        };
        Ok(results
            .into_iter()
            .take(limit)
            .map(|(id, comparison)| SearchResult {
                id,
                score: comparison.score,
                comparison,
                candidate_count,
                retrieval_channels: retrieval_channels.clone(),
            })
            .collect())
    }

    pub fn find_duplicates(
        &self,
        text: &str,
        threshold: f64,
    ) -> Result<Vec<(String, DuplicateResult)>, TextIntelError> {
        self.find_duplicates_with_mode(text, threshold, DuplicateMode::Combined)
    }

    pub fn find_duplicates_with_mode(
        &self,
        text: &str,
        threshold: f64,
        mode: DuplicateMode,
    ) -> Result<Vec<(String, DuplicateResult)>, TextIntelError> {
        let query = self.analyze(text)?;
        let records = self
            .store
            .read()
            .map_err(|_| TextIntelError::Storage("store lock poisoned".to_string()))?
            .search_candidates(&query, self.config.max_search_candidates)
            .map_err(TextIntelError::Storage)?;
        Ok(records
            .into_iter()
            .map(|(id, fingerprint)| {
                (
                    id,
                    duplicate_result_with_mode(
                        &query,
                        &fingerprint,
                        threshold.clamp(0.0, 1.0),
                        &self.config.similarity_weights,
                        mode,
                    ),
                )
            })
            .filter(|(_, result)| result.duplicate)
            .collect())
    }
}
