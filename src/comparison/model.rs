use std::collections::BTreeMap;

use crate::core::capabilities::{CapabilityLevel, ModelMetadata, ProviderCapabilities};
use crate::core::config::SimilarityWeights;
use crate::core::providers::SimilarityScorer;
use crate::core::types::{ComparisonResult, MessageFingerprint};

use super::scorer::score_fingerprints;

mod features;
mod optim;

pub use features::{
    TRAINING_FEATURES, fuzzy_decode_mismatch, language_agreement, mean_channel_confidence,
    training_features,
};
pub use optim::{balanced_sample_weights, logistic_step, logistic_step_weighted, sigmoid};

/// A named deterministic calibration profile. The channel calculation stays
/// explainable; calibration only maps its raw score to an operating-point
/// probability.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SimilarityProfile {
    pub name: String,
    pub weights: SimilarityWeights,
    #[serde(default)]
    pub bias: f64,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
}

fn default_temperature() -> f64 {
    1.0
}

impl SimilarityProfile {
    pub fn new(name: impl Into<String>, weights: SimilarityWeights) -> Self {
        Self {
            name: name.into(),
            weights,
            bias: 0.0,
            temperature: 1.0,
        }
    }

    pub fn general_similarity() -> Self {
        Self::new("general_similarity", SimilarityWeights::default())
    }

    pub fn duplicate() -> Self {
        Self::new(
            "duplicate",
            SimilarityWeights {
                semantic: 0.10,
                lexical: 0.20,
                character: 0.20,
                visual: 0.20,
                phonetic: 0.10,
                symbolic: 0.05,
                decoded: 0.10,
                obfuscation: 0.05,
            },
        )
    }

    pub fn spam() -> Self {
        Self::new(
            "spam",
            SimilarityWeights {
                semantic: 0.05,
                lexical: 0.10,
                character: 0.10,
                visual: 0.20,
                phonetic: 0.05,
                symbolic: 0.05,
                decoded: 0.20,
                obfuscation: 0.25,
            },
        )
    }

    pub fn obfuscation() -> Self {
        Self::new(
            "obfuscation",
            SimilarityWeights {
                semantic: 0.05,
                lexical: 0.05,
                character: 0.15,
                visual: 0.30,
                phonetic: 0.05,
                symbolic: 0.10,
                decoded: 0.20,
                obfuscation: 0.10,
            },
        )
    }

    pub fn rebus() -> Self {
        Self::new(
            "rebus",
            SimilarityWeights {
                semantic: 0.20,
                lexical: 0.20,
                character: 0.05,
                visual: 0.05,
                phonetic: 0.20,
                symbolic: 0.20,
                decoded: 0.10,
                obfuscation: 0.0,
            },
        )
    }

    pub fn with_calibration(mut self, bias: f64, temperature: f64) -> Self {
        self.bias = bias;
        self.temperature = temperature.max(f64::EPSILON);
        self
    }

    pub fn apply(&self, raw_score: f64) -> f64 {
        let raw_score = raw_score.clamp(f64::EPSILON, 1.0 - f64::EPSILON);
        let logit = ((raw_score / (1.0 - raw_score)).ln() + self.bias).clamp(-60.0, 60.0);
        (1.0 / (1.0 + (-logit / self.temperature).exp())).clamp(0.0, 1.0)
    }
}

pub fn score_fingerprints_with_profile(
    left: &MessageFingerprint,
    right: &MessageFingerprint,
    profile: &SimilarityProfile,
) -> ComparisonResult {
    let mut result = score_fingerprints(left, right, &profile.weights);
    result.score = profile.apply(result.score);
    result
}

/// Interpretable linear/logistic scorer for applications that have trained
/// channel weights. Missing channels are omitted and therefore cannot become
/// accidental evidence.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct LogisticSimilarityScorer {
    pub weights: BTreeMap<String, f64>,
    pub bias: f64,
    #[serde(default)]
    pub revision: Option<String>,
}

impl LogisticSimilarityScorer {
    pub fn new(weights: BTreeMap<String, f64>, bias: f64) -> Self {
        Self {
            weights,
            bias,
            revision: None,
        }
    }

    pub fn with_revision(mut self, revision: impl Into<String>) -> Self {
        self.revision = Some(revision.into());
        self
    }
}

impl SimilarityScorer for LogisticSimilarityScorer {
    fn score(&self, left: &MessageFingerprint, right: &MessageFingerprint) -> ComparisonResult {
        let base = score_fingerprints(left, right, &SimilarityWeights::default());
        // Every trained feature is applied: the eight channels plus mean
        // channel confidence, top-language agreement, the twelve
        // confusable-separation features, the fuzzy-decode mismatch
        // interaction, and the seven contextual/entity features (see
        // [`training_features`]).
        let values = [
            ("semantic", base.semantic.unwrap_or(0.0)),
            ("lexical", base.lexical.unwrap_or(0.0)),
            ("character", base.character.unwrap_or(0.0)),
            ("visual", base.visual.unwrap_or(0.0)),
            ("phonetic", base.phonetic.unwrap_or(0.0)),
            ("symbolic", base.symbolic.unwrap_or(0.0)),
            ("decoded", base.decoded_similarity.unwrap_or(0.0)),
            ("obfuscation", base.obfuscation_similarity.unwrap_or(0.0)),
            ("channel_confidence", mean_channel_confidence(&base)),
            ("language_agreement", language_agreement(left, right)),
            ("lexicon_validity", base.lexicon_validity.clamp(0.0, 1.0)),
            ("single_word_pair", base.single_word_pair.clamp(0.0, 1.0)),
            (
                "swapped_word_similarity",
                base.swapped_word_similarity.clamp(0.0, 1.0),
            ),
            ("swapped_phonetic", base.swapped_phonetic.clamp(0.0, 1.0)),
            ("confusable_swap", base.confusable_swap.clamp(0.0, 1.0)),
            (
                "valid_swap_similarity",
                base.valid_swap_similarity.clamp(0.0, 1.0),
            ),
            (
                "normalized_identity",
                base.normalized_identity.clamp(0.0, 1.0),
            ),
            ("cross_script_pair", base.cross_script_pair.clamp(0.0, 1.0)),
            (
                "cross_script_agreement",
                base.cross_script_agreement.clamp(0.0, 1.0),
            ),
            (
                "substring_containment",
                base.substring_containment.clamp(0.0, 1.0),
            ),
            ("exact_decode", base.exact_decode.clamp(0.0, 1.0)),
            ("single_word_exact", base.single_word_exact.clamp(0.0, 1.0)),
            ("fuzzy_decode_mismatch", fuzzy_decode_mismatch(&base)),
            (
                "contextual_semantic",
                base.contextual_semantic.clamp(0.0, 1.0),
            ),
            (
                "cross_language_semantic",
                base.cross_language_semantic.clamp(0.0, 1.0),
            ),
            ("entity_agreement", base.entity_agreement.clamp(0.0, 1.0)),
            ("entity_conflict", base.entity_conflict.clamp(0.0, 1.0)),
            (
                "semantic_without_lexical_overlap",
                base.semantic_without_lexical_overlap.clamp(0.0, 1.0),
            ),
            (
                "transliteration_semantic_agreement",
                base.transliteration_semantic_agreement.clamp(0.0, 1.0),
            ),
            (
                "transliteration_semantic_conflict",
                base.transliteration_semantic_conflict.clamp(0.0, 1.0),
            ),
        ];
        let logit = values.iter().fold(self.bias, |total, (name, value)| {
            total + value * self.weights.get(*name).copied().unwrap_or(0.0)
        });
        let probability = (1.0 / (1.0 + (-logit).exp())).clamp(0.0, 1.0);
        let mut result = base;
        result.score = probability;
        result
    }

    fn capabilities(&self) -> ProviderCapabilities {
        let mut capabilities = ProviderCapabilities::new("logistic_similarity_scorer")
            .with_version(
                self.revision
                    .clone()
                    .unwrap_or_else(|| "unversioned".to_string()),
            )
            .with_quality(CapabilityLevel::Production);
        if let Some(revision) = &self.revision {
            capabilities = capabilities.with_model_revision(revision.clone());
        }
        capabilities
    }
}

/// Schema version of the interpretable training feature vector. Bump when
/// [`training_features`] gains, drops, or reorders features.
pub const TRAINING_FEATURE_SCHEMA_VERSION: u32 = 9;

/// Versioned trained-similarity artifact written by `tools/train_similarity.rs`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SimilarityModelArtifact {
    pub artifact_version: u32,
    pub kind: String,
    pub feature_schema_version: u32,
    pub dataset_version: String,
    pub weights: BTreeMap<String, f64>,
    pub bias: f64,
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub metrics: BTreeMap<String, f64>,
    /// Training configuration (`iterations`, `learning_rate`, `l2`,
    /// `sample_weighting`, …). Recorded by the training tool; older
    /// artifacts read empty.
    #[serde(default)]
    pub training_config: BTreeMap<String, String>,
    /// Calibration configuration (`bias_iterations`, `temperature`,
    /// `method`, …). Recorded by the training tool; older artifacts read
    /// empty.
    #[serde(default)]
    pub calibration_config: BTreeMap<String, String>,
    /// Embedding provider metadata behind the training featurization
    /// (model id, revision, dimensions). Older artifacts read `None`.
    #[serde(default)]
    pub embedding_model: Option<ModelMetadata>,
}

impl SimilarityModelArtifact {
    pub fn new(
        dataset_version: impl Into<String>,
        weights: BTreeMap<String, f64>,
        bias: f64,
    ) -> Self {
        Self {
            artifact_version: 1,
            kind: "logistic_similarity".to_string(),
            feature_schema_version: TRAINING_FEATURE_SCHEMA_VERSION,
            dataset_version: dataset_version.into(),
            weights,
            bias,
            revision: None,
            metrics: BTreeMap::new(),
            training_config: BTreeMap::new(),
            calibration_config: BTreeMap::new(),
            embedding_model: None,
        }
    }

    pub fn with_revision(mut self, revision: impl Into<String>) -> Self {
        self.revision = Some(revision.into());
        self
    }

    pub fn with_metrics(mut self, metrics: BTreeMap<String, f64>) -> Self {
        self.metrics = metrics;
        self
    }

    pub fn with_training_config(mut self, config: BTreeMap<String, String>) -> Self {
        self.training_config = config;
        self
    }

    pub fn with_calibration_config(mut self, config: BTreeMap<String, String>) -> Self {
        self.calibration_config = config;
        self
    }

    pub fn with_embedding_model(mut self, model: ModelMetadata) -> Self {
        self.embedding_model = Some(model);
        self
    }

    /// Deserialize and validate: kind, feature schema, and weight names must
    /// match this build, otherwise the artifact is rejected, never guessed.
    pub fn from_json(source: &str) -> Result<Self, String> {
        let artifact: Self =
            serde_json::from_str(source).map_err(|error| format!("invalid artifact: {error}"))?;
        if artifact.kind != "logistic_similarity" {
            return Err(format!("unsupported artifact kind {:?}", artifact.kind));
        }
        if artifact.feature_schema_version != TRAINING_FEATURE_SCHEMA_VERSION {
            return Err(format!(
                "feature schema {} is not supported (build expects {})",
                artifact.feature_schema_version, TRAINING_FEATURE_SCHEMA_VERSION
            ));
        }
        let expected: Vec<&str> = TRAINING_FEATURES.to_vec();
        let mut names: Vec<&str> = artifact.weights.keys().map(String::as_str).collect();
        names.sort_unstable();
        let mut sorted = expected.clone();
        sorted.sort_unstable();
        if names != sorted {
            return Err(format!(
                "artifact weights {names:?} do not match training features {sorted:?}"
            ));
        }
        if !artifact.bias.is_finite() || artifact.weights.values().any(|weight| !weight.is_finite())
        {
            return Err("artifact contains non-finite parameters".to_string());
        }
        Ok(artifact)
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|error| error.to_string())
    }

    /// Ordered weight vector matching [`TRAINING_FEATURES`].
    pub fn ordered_weights(&self) -> Vec<f64> {
        TRAINING_FEATURES
            .iter()
            .map(|name| self.weights.get(*name).copied().unwrap_or(0.0))
            .collect()
    }

    pub fn to_scorer(&self) -> LogisticSimilarityScorer {
        let mut scorer = LogisticSimilarityScorer::new(self.weights.clone(), self.bias);
        if let Some(revision) = &self.revision {
            scorer = scorer.with_revision(revision.clone());
        }
        scorer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_round_trip_preserves_parameters() {
        let weights: BTreeMap<String, f64> = TRAINING_FEATURES
            .iter()
            .map(|name| ((*name).to_string(), 0.1))
            .collect();
        let artifact = SimilarityModelArtifact::new("test-0.0", weights, -0.5)
            .with_revision("r1")
            .with_metrics(BTreeMap::from([("test_accuracy".to_string(), 0.9)]));
        let loaded = SimilarityModelArtifact::from_json(&artifact.to_json().unwrap()).unwrap();
        assert_eq!(loaded, artifact);
        assert_eq!(loaded.ordered_weights(), vec![0.1; TRAINING_FEATURES.len()]);
        let scorer = loaded.to_scorer();
        assert_eq!(scorer.bias, -0.5);
    }

    #[test]
    fn artifact_rejects_mismatched_schema_and_weights() {
        let weights: BTreeMap<String, f64> = TRAINING_FEATURES
            .iter()
            .map(|name| ((*name).to_string(), 0.1))
            .collect();
        let valid = SimilarityModelArtifact::new("test-0.0", weights, 0.0);
        let mut source = serde_json::to_value(&valid).unwrap();

        source["kind"] = serde_json::Value::String("other".to_string());
        assert!(SimilarityModelArtifact::from_json(&source.to_string()).is_err());

        let mut source = serde_json::to_value(&valid).unwrap();
        source["feature_schema_version"] = serde_json::json!(999);
        assert!(SimilarityModelArtifact::from_json(&source.to_string()).is_err());

        let mut source = serde_json::to_value(&valid).unwrap();
        source["weights"].as_object_mut().unwrap().remove("lexical");
        assert!(SimilarityModelArtifact::from_json(&source.to_string()).is_err());

        let mut source = serde_json::to_value(&valid).unwrap();
        source["bias"] = serde_json::Value::Null;
        assert!(SimilarityModelArtifact::from_json(&source.to_string()).is_err());
    }
}
