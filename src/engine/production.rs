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
    SymbolKnowledgeProvider,
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
    pub spam: ProviderCapabilities,
    pub similarity: Option<ProviderCapabilities>,
    pub reranker: Option<ProviderCapabilities>,
    pub store: ProviderCapabilities,
    pub ann_enabled: bool,
    pub degraded: Vec<DegradedCapability>,
    pub resource_manifest: Vec<ResourcePackInfo>,
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
    pub(crate) reranker: Option<Arc<dyn RerankerProvider>>,
    pub(crate) spam: Option<Arc<dyn SpamPredictor>>,
    pub(crate) similarity_scorer: Option<Arc<dyn SimilarityScorer>>,
    pub(crate) similarity_profile: Option<crate::comparison::SimilarityProfile>,
    pub(crate) similarity_model_path: Option<PathBuf>,
    pub(crate) similarity_model_required: bool,
    pub(crate) spam_model_path: Option<PathBuf>,
    pub(crate) spam_model_required: bool,
    pub(crate) json_store_path: Option<PathBuf>,
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

    /// Persist fingerprints to a local JSON file store.
    pub fn json_store(mut self, path: impl Into<PathBuf>) -> Self {
        self.json_store_path = Some(path.into());
        self
    }

    /// Attempt the local production stack: embedded resource packs, trained
    /// models from `./models` when present, espeak-ng G2P with a rule-based
    /// fallback, and a local feature-hash embedding baseline. Anything
    /// unavailable is reported through [`EngineDiagnostics::degraded`];
    /// nothing panics and nothing touches the network.
    pub fn production_local(mut self) -> Self {
        self.config.semantic = true;
        self.config.phonetic = true;
        #[cfg(feature = "phonetic-espeak")]
        {
            if self.g2p.is_none() {
                if let Ok(provider) = crate::phonetic::EspeakNgG2PProvider::auto_detect() {
                    self.g2p = Some(Arc::new(provider));
                }
            }
        }
        if self.embedding.is_none() {
            if let Ok(provider) = crate::semantic::FeatureHashEmbeddingProvider::new(256) {
                self.embedding = Some(Arc::new(provider));
            }
        }
        if self.similarity_model_path.is_none() {
            self.similarity_model_path = Some(PathBuf::from("models/similarity-v1.json"));
        }
        if self.spam_model_path.is_none() {
            self.spam_model_path = Some(PathBuf::from("models/spam-v1.json"));
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
