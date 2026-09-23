//! Engine diagnostics: configuration access, provider capabilities,
//! health checks, resource manifests, and deployment reports.

use std::collections::BTreeMap;

use crate::cache::CacheDiagnostics;
use crate::core::capabilities::ProviderCapabilities;
use crate::core::config::EngineConfig;
use crate::core::error::TextIntelError;
use crate::engine::TextIntelligence;
use crate::engine::production::{DegradedCapability, EngineDiagnostics};

impl TextIntelligence {
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
        if let Some(entities) = &self.entity_provider {
            capabilities.insert("entities".to_string(), entities.capabilities());
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
        if let Some(generative) = &self.generative_provider {
            capabilities.insert("generative".to_string(), generative.capabilities());
        }
        if let Some(scorer) = &self.similarity_scorer {
            capabilities.insert("similarity".to_string(), scorer.capabilities());
        }
        if let Some(decision) = &self.decision_provider {
            capabilities.insert("decision".to_string(), decision.capabilities());
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
            embedding_model: self.embedding_provider.model_metadata(),
            g2p: get("g2p"),
            language: get("language"),
            lexicon: get("lexicon"),
            symbols: get("symbols"),
            abbreviations: capabilities.get("abbreviations").cloned(),
            transliteration: capabilities.get("transliteration").cloned(),
            entities: capabilities.get("entities").cloned(),
            spam: get("spam"),
            similarity: capabilities.get("similarity").cloned(),
            reranker: capabilities.get("reranker").cloned(),
            generative: capabilities.get("generative").cloned(),
            decision: capabilities.get("decision").cloned(),
            store: get("store"),
            ann_enabled: store_capabilities.ann_enabled,
            degraded: self.degraded_capabilities(&capabilities),
            resource_manifest: self.resource_manifest(),
            caches: self.cache_diagnostics(),
            store_capabilities,
            candidate_budgets: crate::engine::production::CandidateBudgets {
                max_search_candidates: self.config.max_search_candidates,
                max_per_channel_candidates: self.config.max_per_channel_candidates,
                max_ann_candidates: self.config.max_ann_candidates,
            },
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
            ("decision".to_string(), self.decision_cache_diagnostics()),
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
            None => note(
                "phonetic",
                "none",
                "espeak_ng_g2p",
                "no G2P backend configured",
            ),
        }
        if !capabilities.contains_key("similarity") {
            note(
                "similarity",
                "weighted deterministic scorer",
                "trained similarity model",
                "load models/similarity-v5.json through trained_similarity_model() for calibrated scoring",
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
                "load models/spam-v2.json through trained_spam_model() for the trained predictor",
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
}
