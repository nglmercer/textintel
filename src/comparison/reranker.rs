//! Optional retrieval reranker over full channel comparisons.
//!
//! The search pipeline retrieves candidates cheaply, scores each with the
//! full multilingual comparison, and — only when a reranker is configured —
//! rescores the bounded top set with this linear channel model before the
//! final top-N cut. The default engine never depends on it.

use crate::core::capabilities::{CapabilityLevel, ProviderCapabilities};
use crate::core::error::ProviderError;
use crate::core::providers::RerankerProvider;
use crate::core::types::{ComparisonResult, MessageFingerprint};

/// Linear weights over channel-agreement features. Every weight must be
/// finite; missing channels contribute their default (0.0) rather than a
/// penalty, so partial evidence never demotes a candidate.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(default)]
pub struct ChannelRerankWeights {
    pub base: f64,
    pub decoded: f64,
    pub semantic: f64,
    pub lexical: f64,
    pub phonetic: f64,
    pub visual: f64,
    pub symbolic: f64,
    pub character: f64,
    pub obfuscation: f64,
    pub language_agreement: f64,
    pub bias: f64,
}

impl Default for ChannelRerankWeights {
    fn default() -> Self {
        Self {
            base: 1.5,
            decoded: 1.2,
            semantic: 1.0,
            lexical: 0.8,
            phonetic: 0.6,
            visual: 0.4,
            symbolic: 0.5,
            character: 0.4,
            obfuscation: 0.2,
            language_agreement: 0.3,
            bias: -1.0,
        }
    }
}

impl ChannelRerankWeights {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("base", self.base),
            ("decoded", self.decoded),
            ("semantic", self.semantic),
            ("lexical", self.lexical),
            ("phonetic", self.phonetic),
            ("visual", self.visual),
            ("symbolic", self.symbolic),
            ("character", self.character),
            ("obfuscation", self.obfuscation),
            ("language_agreement", self.language_agreement),
            ("bias", self.bias),
        ] {
            if !value.is_finite() {
                return Err(format!("reranker weight {name} must be finite"));
            }
        }
        Ok(())
    }
}

/// Logistic reranker over channel agreement. Only the top `max_candidates`
/// inputs (by incoming retrieval score) are rescored and returned sorted;
/// the tail follows in original order with original scores, so the reranker
/// can reorder but never drop evidence.
#[derive(Debug, Clone)]
pub struct ChannelScoreReranker {
    weights: ChannelRerankWeights,
    max_candidates: usize,
    revision: Option<String>,
}

impl ChannelScoreReranker {
    pub fn new(weights: ChannelRerankWeights, max_candidates: usize) -> Result<Self, String> {
        weights.validate()?;
        if max_candidates == 0 {
            return Err("max_candidates must be positive".to_string());
        }
        Ok(Self {
            weights,
            max_candidates,
            revision: None,
        })
    }

    pub fn with_revision(mut self, revision: impl Into<String>) -> Self {
        self.revision = Some(revision.into());
        self
    }

    /// Build from a versioned artifact: the deterministic default weights and
    /// trained weights share this path, so either can serve production. The
    /// candidate bound stays a deployment choice, not a trained parameter.
    pub fn from_artifact(
        artifact: &RerankerModelArtifact,
        max_candidates: usize,
    ) -> Result<Self, String> {
        Self::new(artifact.weights.clone(), max_candidates).map(|reranker| {
            match &artifact.revision {
                Some(revision) => reranker.with_revision(revision.clone()),
                None => reranker,
            }
        })
    }

    pub fn max_candidates(&self) -> usize {
        self.max_candidates
    }
}

/// Versioned channel-reranker artifact. The deterministic default weights can
/// be exported through this format, and trained weights load through the same
/// [`ChannelScoreReranker::from_artifact`] path — the engine never
/// distinguishes them after load.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct RerankerModelArtifact {
    pub artifact_version: u32,
    pub kind: String,
    pub dataset_version: String,
    pub weights: ChannelRerankWeights,
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub metrics: std::collections::BTreeMap<String, f64>,
}

impl RerankerModelArtifact {
    pub fn new(dataset_version: impl Into<String>, weights: ChannelRerankWeights) -> Self {
        Self {
            artifact_version: 1,
            kind: "channel_reranker".to_string(),
            dataset_version: dataset_version.into(),
            weights,
            revision: None,
            metrics: std::collections::BTreeMap::new(),
        }
    }

    /// The deterministic default weights as an artifact (baseline revision).
    pub fn deterministic(dataset_version: impl Into<String>) -> Self {
        Self::new(dataset_version, ChannelRerankWeights::default())
            .with_revision("channel-reranker-v1")
    }

    pub fn with_revision(mut self, revision: impl Into<String>) -> Self {
        self.revision = Some(revision.into());
        self
    }

    pub fn with_metrics(mut self, metrics: std::collections::BTreeMap<String, f64>) -> Self {
        self.metrics = metrics;
        self
    }

    pub fn from_json(source: &str) -> Result<Self, String> {
        let artifact: Self = serde_json::from_str(source)
            .map_err(|error| format!("invalid reranker JSON: {error}"))?;
        if artifact.artifact_version != 1 {
            return Err(format!(
                "unsupported reranker artifact_version {}",
                artifact.artifact_version
            ));
        }
        if artifact.kind != "channel_reranker" {
            return Err(format!("unexpected reranker kind {}", artifact.kind));
        }
        artifact
            .weights
            .validate()
            .map_err(|error| format!("invalid reranker weights: {error}"))?;
        Ok(artifact)
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|error| error.to_string())
    }

    pub fn to_reranker(&self, max_candidates: usize) -> Result<ChannelScoreReranker, String> {
        ChannelScoreReranker::from_artifact(self, max_candidates)
    }
}

impl RerankerProvider for ChannelScoreReranker {
    fn rerank(
        &self,
        query: &MessageFingerprint,
        candidates: Vec<(String, MessageFingerprint, f64)>,
    ) -> Result<Vec<(String, MessageFingerprint, f64)>, ProviderError> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        // Head selection by incoming retrieval score, then a bounded full
        // channel recomputation per head item. The head is returned sorted by
        // corrected score; the tail follows in original relative order with
        // original scores, so reranking reorders but never drops evidence.
        let mut ordered = candidates;
        ordered.sort_by(|left, right| right.2.total_cmp(&left.2));
        let head = ordered.len().min(self.max_candidates);
        let mut rescored = Vec::with_capacity(head);
        let mut tail = Vec::with_capacity(ordered.len().saturating_sub(head));
        for (position, (id, fingerprint, score)) in ordered.into_iter().enumerate() {
            if position < head {
                let comparison = crate::comparison::score_fingerprints(
                    query,
                    &fingerprint,
                    &crate::core::config::SimilarityWeights::default(),
                );
                let language_agreement = match (query.top_language(), fingerprint.top_language()) {
                    (Some(left), Some(right)) if left == right => 1.0,
                    _ => 0.0,
                };
                let mut corrected = comparison;
                corrected.score = score.clamp(0.0, 1.0);
                let next = rerank_score(&self.weights, &corrected, language_agreement);
                rescored.push((id, fingerprint, next));
            } else {
                tail.push((id, fingerprint, score));
            }
        }
        rescored.sort_by(|left, right| right.2.total_cmp(&left.2));
        rescored.extend(tail);
        Ok(rescored)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        let mut capabilities = ProviderCapabilities::new("channel_score_reranker")
            .with_quality(CapabilityLevel::Basic);
        if let Some(revision) = &self.revision {
            capabilities = capabilities.with_model_revision(revision.clone());
        }
        capabilities
    }
}

/// Score one reranked candidate from its comparison channels. Kept separate
/// so the feature mapping stays testable without engine fixtures.
pub fn rerank_score(
    weights: &ChannelRerankWeights,
    comparison: &ComparisonResult,
    language_agreement: f64,
) -> f64 {
    let logit = weights.bias
        + weights.base * comparison.score
        + weights.decoded * comparison.decoded_similarity.unwrap_or(0.0)
        + weights.semantic * comparison.semantic.unwrap_or(0.0)
        + weights.lexical * comparison.lexical.unwrap_or(0.0)
        + weights.phonetic * comparison.phonetic.unwrap_or(0.0)
        + weights.visual * comparison.visual.unwrap_or(0.0)
        + weights.symbolic * comparison.symbolic.unwrap_or(0.0)
        + weights.character * comparison.character.unwrap_or(0.0)
        + weights.obfuscation
            * comparison
                .obfuscation_similarity
                .or(comparison.obfuscation)
                .unwrap_or(0.0)
        + weights.language_agreement * language_agreement;
    crate::comparison::sigmoid(logit).clamp(0.0, 1.0)
}
