//! Production preset, ergonomic builder, and engine diagnostics.
//!
//! [`TextIntelligence::default`](crate::engine::TextIntelligence::default)
//! stays lightweight and deterministic. Everything production-grade lives
//! here: [`EngineBuilder`] wires explicit providers and trained artifacts,
//! [`EngineBuilder::production_local`] assembles the local production stack
//! (resource packs, trained models when present, espeak-ng G2P with a
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
    AbbreviationProvider, EmbeddingProvider, G2PProvider, LanguageDetectionProvider,
    LemmatizerProvider, LexiconProvider, RerankerProvider, SimilarityScorer, SpamPredictor,
    SymbolKnowledgeProvider, TransliterationProvider,
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
    pub g2p: ProviderCapabilities,
    pub language: ProviderCapabilities,
    pub lexicon: ProviderCapabilities,
    pub symbols: ProviderCapabilities,
    pub abbreviations: Option<ProviderCapabilities>,
    #[serde(default)]
    pub transliteration: Option<ProviderCapabilities>,
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
}

/// Production similarity artifact within `dir`: `similarity-v2` (trained on
/// dataset 0.5.0 with semantic and phonetic evidence) wins when present;
/// otherwise fall back to `similarity-v1` so older checkouts keep working.
/// Diagnostics always report the loaded revision, so the active artifact is
/// explicit.
pub fn preferred_similarity_artifact_in(dir: &std::path::Path) -> PathBuf {
    let v2 = dir.join("similarity-v2.json");
    if v2.exists() {
        v2
    } else {
        dir.join("similarity-v1.json")
    }
}

/// [`preferred_similarity_artifact_in`] for the conventional `./models`
/// directory.
pub fn preferred_similarity_artifact() -> PathBuf {
    preferred_similarity_artifact_in(std::path::Path::new("models"))
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
    pub(crate) reranker: Option<Arc<dyn RerankerProvider>>,
    /// Explicit local transformer directory for [`Self::production_local`].
    pub(crate) transformer_model_path: Option<PathBuf>,
    /// Fallback notes recorded while assembling the preset (surfaced via
    /// [`EngineDiagnostics::degraded`]).
    pub(crate) preset_fallbacks: Vec<DegradedCapability>,
    pub(crate) spam: Option<Arc<dyn SpamPredictor>>,
    pub(crate) similarity_scorer: Option<Arc<dyn SimilarityScorer>>,
    pub(crate) similarity_profile: Option<crate::comparison::SimilarityProfile>,
    pub(crate) similarity_model_path: Option<PathBuf>,
    pub(crate) similarity_model_required: bool,
    pub(crate) spam_model_path: Option<PathBuf>,
    pub(crate) spam_model_required: bool,
    pub(crate) reranker_model_path: Option<PathBuf>,
    pub(crate) reranker_model_required: bool,
    pub(crate) reranker_max_candidates: Option<usize>,
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

    /// Pin the local transformer directory used by [`Self::production_local`].
    /// The directory must already exist (no download); when it cannot be
    /// opened the preset falls back to the feature-hash baseline and records
    /// the failure in diagnostics.
    pub fn transformer_model(mut self, path: impl Into<PathBuf>) -> Self {
        self.transformer_model_path = Some(path.into());
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

    /// Load a trained similarity artifact at build time. Missing files fail
    /// the build; present-but-invalid files always fail (never silently
    /// ignored).
    pub fn trained_similarity_model(mut self, path: impl Into<PathBuf>) -> Self {
        self.similarity_model_path = Some(path.into());
        self.similarity_model_required = true;
        self
    }

    /// Load a trained spam artifact at build time (same failure semantics as
    /// [`Self::trained_similarity_model`]).
    pub fn trained_spam_model(mut self, path: impl Into<PathBuf>) -> Self {
        self.spam_model_path = Some(path.into());
        self.spam_model_required = true;
        self
    }

    /// Load a versioned reranker artifact at build time (same failure
    /// semantics as [`Self::trained_similarity_model`]). The deterministic
    /// default weights and trained weights share this path; the candidate
    /// bound stays a deployment choice (see [`Self::reranker_max_candidates`],
    /// default 64).
    pub fn trained_reranker_model(mut self, path: impl Into<PathBuf>) -> Self {
        self.reranker_model_path = Some(path.into());
        self.reranker_model_required = true;
        self
    }

    /// Bound the reranked head when loading [`Self::trained_reranker_model`].
    /// Reranking only ever runs on this bounded set; the tail keeps its
    /// retrieval order and full-comparison scores.
    pub fn reranker_max_candidates(mut self, max_candidates: usize) -> Self {
        self.reranker_max_candidates = Some(max_candidates.max(1));
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

    /// Attempt the local production stack: embedded resource packs, trained
    /// models from `./models` when present, espeak-ng G2P with a rule-based
    /// fallback, transformer embeddings when a local model is configured
    /// (explicit [`Self::transformer_model`], `TEXTINTEL_TRANSFORMER_MODEL`,
    /// or `./models/transformer`), falling back to the feature-hash baseline,
    /// and bounded revision-aware caches for embeddings, G2P, language
    /// detection, and rebus decoding (see [`EngineDiagnostics::caches`]).
    /// Anything unavailable is reported through [`EngineDiagnostics::degraded`];
    /// nothing panics and nothing touches the network.
    pub fn production_local(mut self) -> Self {
        self.config.semantic = true;
        self.config.phonetic = true;
        // Bounded revision-aware caches for embeddings, G2P, language
        // detection, and rebus decoding. Explicitly configured limits win;
        // only disabled (0) entries take the production defaults.
        self.config.cache = std::mem::take(&mut self.config.cache).with_production_defaults();
        #[cfg(feature = "phonetic-espeak")]
        {
            if self.g2p.is_none() {
                if let Ok(provider) = crate::phonetic::EspeakNgG2PProvider::auto_detect() {
                    self.g2p = Some(Arc::new(provider));
                }
            }
        }
        if self.embedding.is_none() {
            // Only the transformer branch below can set this; without the
            // feature the fallback always runs.
            #[cfg_attr(not(feature = "semantic-transformer"), allow(unused_mut))]
            let mut used_transformer = false;
            let configured = self.transformer_model_path.clone().or_else(|| {
                std::env::var("TEXTINTEL_TRANSFORMER_MODEL")
                    .ok()
                    .map(PathBuf::from)
            });
            // `models/transformer` is a conventional best-effort location; an
            // explicit path or env var counts as "configured" and its failure
            // is reported, while a missing conventional directory is not.
            let (candidate, explicit) = match configured {
                Some(path) => (Some(path), true),
                None => {
                    let conventional = PathBuf::from("models/transformer");
                    if conventional.join("config.json").is_file() {
                        (Some(conventional), false)
                    } else {
                        (None, false)
                    }
                }
            };
            #[cfg(feature = "semantic-transformer")]
            {
                if let Some(directory) = candidate {
                    match crate::semantic::TransformerEmbeddingProvider::open(&directory) {
                        Ok(provider) => {
                            self.embedding = Some(Arc::new(provider));
                            used_transformer = true;
                        }
                        Err(error) if explicit => {
                            self.preset_fallbacks.push(DegradedCapability {
                                capability: "semantic".to_string(),
                                configured: "feature_hash_embedding".to_string(),
                                wanted: "transformer_embedding".to_string(),
                                detail: format!(
                                    "configured transformer model at {} failed to open ({error}); serving the feature-hash fallback",
                                    directory.display()
                                ),
                            });
                        }
                        Err(_) => {}
                    }
                }
            }
            #[cfg(not(feature = "semantic-transformer"))]
            {
                if let Some(directory) = candidate.filter(|_| explicit) {
                    self.preset_fallbacks.push(DegradedCapability {
                        capability: "semantic".to_string(),
                        configured: "feature_hash_embedding".to_string(),
                        wanted: "transformer_embedding".to_string(),
                        detail: format!(
                            "transformer model configured at {} but this build lacks the semantic-transformer feature; serving the feature-hash fallback",
                            directory.display()
                        ),
                    });
                }
            }
            if !used_transformer {
                if let Ok(provider) = crate::semantic::FeatureHashEmbeddingProvider::new(256) {
                    self.embedding = Some(Arc::new(provider));
                }
            }
        }
        if self.similarity_model_path.is_none() {
            self.similarity_model_path = Some(preferred_similarity_artifact());
        }
        if self.spam_model_path.is_none() {
            self.spam_model_path = Some(PathBuf::from("models/spam-v1.json"));
        }
        if self.reranker_model_path.is_none() && self.reranker.is_none() {
            self.reranker_model_path = Some(PathBuf::from("models/reranker-v1.json"));
        }
        self
    }

    pub fn build(self) -> Result<crate::engine::TextIntelligence, TextIntelError> {
        crate::engine::TextIntelligence::assemble(self)
    }
}

/// Load a trained similarity artifact. A missing file is skipped only for
/// preset best-effort paths (`required == false`); a present-but-invalid file
/// always fails so corrupt models are never silently ignored.
pub(crate) fn load_similarity_scorer(
    path: &std::path::Path,
    required: bool,
) -> Result<Option<crate::comparison::LogisticSimilarityScorer>, TextIntelError> {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(_) if !required => return Ok(None),
        Err(error) => {
            return Err(TextIntelError::InvalidConfiguration(format!(
                "cannot read similarity model {}: {error}",
                path.display()
            )));
        }
    };
    crate::comparison::SimilarityModelArtifact::from_json(&source)
        .map(|artifact| artifact.to_scorer())
        .map(Some)
        .map_err(TextIntelError::InvalidConfiguration)
}

/// Load a trained spam artifact (same missing/invalid semantics as
/// [`load_similarity_scorer`]).
pub(crate) fn load_spam_predictor(
    path: &std::path::Path,
    required: bool,
) -> Result<Option<crate::detection::TrainedSpamPredictor>, TextIntelError> {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(_) if !required => return Ok(None),
        Err(error) => {
            return Err(TextIntelError::InvalidConfiguration(format!(
                "cannot read spam model {}: {error}",
                path.display()
            )));
        }
    };
    crate::detection::SpamModelArtifact::from_json(&source)
        .map(|artifact| artifact.to_predictor())
        .map(Some)
        .map_err(TextIntelError::InvalidConfiguration)
}

/// Load a versioned reranker artifact (same missing/invalid semantics as
/// [`load_similarity_scorer`]). Absence keeps the engine on plain retrieval
/// order; the candidate bound defaults to 64.
pub(crate) fn load_reranker(
    path: &std::path::Path,
    required: bool,
    max_candidates: Option<usize>,
) -> Result<Option<crate::comparison::ChannelScoreReranker>, TextIntelError> {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(_) if !required => return Ok(None),
        Err(error) => {
            return Err(TextIntelError::InvalidConfiguration(format!(
                "cannot read reranker model {}: {error}",
                path.display()
            )));
        }
    };
    crate::comparison::RerankerModelArtifact::from_json(&source)
        .map_err(TextIntelError::InvalidConfiguration)
        .and_then(|artifact| {
            artifact
                .to_reranker(max_candidates.unwrap_or(64))
                .map_err(TextIntelError::InvalidConfiguration)
        })
        .map(Some)
}
