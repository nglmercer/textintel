use std::collections::BTreeMap;

use crate::core::capabilities::{CapabilityLevel, ProviderCapabilities};
use crate::core::config::SimilarityWeights;
use crate::core::providers::SimilarityScorer;
use crate::core::types::{ComparisonResult, MessageFingerprint};

use super::scorer::score_fingerprints;

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
        let values = [
            ("semantic", base.semantic),
            ("lexical", base.lexical),
            ("character", base.character),
            ("visual", base.visual),
            ("phonetic", base.phonetic),
            ("symbolic", base.symbolic),
            ("decoded", base.decoded_similarity),
            ("obfuscation", base.obfuscation_similarity),
        ];
        let logit = values.iter().fold(self.bias, |total, (name, value)| {
            total + value.unwrap_or(0.0) * self.weights.get(*name).copied().unwrap_or(0.0)
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
pub const TRAINING_FEATURE_SCHEMA_VERSION: u32 = 1;

/// Interpretable features in fixed order: the eight evidence channels
/// (missing channels read as 0.0, exactly as the scorer treats them),
/// mean channel confidence, and top-language agreement.
pub const TRAINING_FEATURES: &[&str] = &[
    "semantic",
    "lexical",
    "character",
    "visual",
    "phonetic",
    "symbolic",
    "decoded",
    "obfuscation",
    "channel_confidence",
    "language_agreement",
];

/// Extract the training feature vector for one comparison. `language_agreement`
/// is 1.0 when both fingerprints agree on the top language, else 0.0.
pub fn training_features(
    result: &ComparisonResult,
    language_agreement: f64,
) -> BTreeMap<String, f64> {
    let channels = [
        ("semantic", result.semantic),
        ("lexical", result.lexical),
        ("character", result.character),
        ("visual", result.visual),
        ("phonetic", result.phonetic),
        ("symbolic", result.symbolic),
        ("decoded", result.decoded_similarity),
        ("obfuscation", result.obfuscation_similarity),
    ];
    let mut features = BTreeMap::new();
    for (name, value) in channels {
        features.insert(name.to_string(), value.unwrap_or(0.0));
    }
    let confidence = if result.channel_confidence.is_empty() {
        0.0
    } else {
        result.channel_confidence.values().sum::<f64>() / result.channel_confidence.len() as f64
    };
    features.insert("channel_confidence".to_string(), confidence.clamp(0.0, 1.0));
    features.insert(
        "language_agreement".to_string(),
        language_agreement.clamp(0.0, 1.0),
    );
    features
}

/// Top-language agreement between two fingerprints: 1.0 when the first
/// language candidates match (or both are empty), else 0.0.
pub fn language_agreement(left: &MessageFingerprint, right: &MessageFingerprint) -> f64 {
    let top = |fingerprint: &MessageFingerprint| {
        fingerprint
            .language_candidates
            .first()
            .map(|candidate| candidate.language.clone())
            .unwrap_or_else(|| "unknown".to_string())
    };
    if top(left) == top(right) {
        1.0
    } else {
        0.0
    }
}

pub fn sigmoid(logit: f64) -> f64 {
    (1.0 / (1.0 + (-logit.clamp(-60.0, 60.0)).exp())).clamp(0.0, 1.0)
}

/// One full-batch gradient step for L2-regularized logistic loss. Returns
/// the mean loss. Deterministic: fixed order, no sampling.
pub fn logistic_step(
    features: &[Vec<f64>],
    labels: &[bool],
    weights: &mut [f64],
    bias: &mut f64,
    learning_rate: f64,
    l2: f64,
) -> f64 {
    let uniform = vec![1.0; features.len()];
    logistic_step_weighted(features, labels, weights, bias, learning_rate, l2, &uniform)
}

/// Weighted variant of [`logistic_step`]: each sample contributes
/// proportionally to `sample_weights` (normalized by their sum), so class
/// balancing is exact instead of approximate. Non-finite or negative sample
/// weights are treated as zero; an all-zero weight vector leaves the
/// parameters untouched and reports zero loss.
pub fn logistic_step_weighted(
    features: &[Vec<f64>],
    labels: &[bool],
    weights: &mut [f64],
    bias: &mut f64,
    learning_rate: f64,
    l2: f64,
    sample_weights: &[f64],
) -> f64 {
    let clean: Vec<f64> = sample_weights
        .iter()
        .map(|value| {
            if value.is_finite() && *value > 0.0 {
                *value
            } else {
                0.0
            }
        })
        .collect();
    let total: f64 = clean.iter().sum();
    if total <= 0.0 {
        return 0.0;
    }
    let mut loss = 0.0;
    let mut gradient = vec![0.0; weights.len()];
    let mut bias_gradient = 0.0;
    for ((row, label), weight) in features.iter().zip(labels.iter()).zip(clean.iter()) {
        let logit = row
            .iter()
            .zip(weights.iter())
            .map(|(value, weight)| value * weight)
            .sum::<f64>()
            + *bias;
        let predicted = sigmoid(logit).clamp(1e-12, 1.0 - 1e-12);
        let target = if *label { 1.0 } else { 0.0 };
        loss += weight * -(target * predicted.ln() + (1.0 - target) * (1.0 - predicted).ln());
        let error = weight * (predicted - target);
        for (index, value) in row.iter().enumerate() {
            gradient[index] += error * value;
        }
        bias_gradient += error;
    }
    for (index, weight) in weights.iter_mut().enumerate() {
        *weight -= learning_rate * (gradient[index] / total + l2 * *weight);
    }
    *bias -= learning_rate * bias_gradient / total;
    loss / total
}

/// Balanced sample weights: positives and negatives each carry half the total
/// mass, so a skewed training split cannot drag the operating point. Either
/// class may be empty (all mass goes to the other side).
pub fn balanced_sample_weights(labels: &[bool]) -> Vec<f64> {
    let positive_count = labels.iter().filter(|label| **label).count();
    let negative_count = labels.len().saturating_sub(positive_count);
    let positives = positive_count.max(1) as f64;
    let negatives = negative_count.max(1) as f64;
    labels
        .iter()
        .map(|label| {
            if *label {
                0.5 / positives
            } else {
                0.5 / negatives
            }
        })
        .collect()
}

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
    fn gradient_steps_reduce_separable_loss() {
        // Two clusters on one feature: repeated steps must drive loss down
        // and orient the weight positively.
        let features = vec![
            vec![0.9, 0.1],
            vec![0.8, 0.2],
            vec![0.2, 0.8],
            vec![0.1, 0.9],
        ];
        let labels = vec![true, true, false, false];
        let mut weights = vec![0.0, 0.0];
        let mut bias = 0.0;
        let mut previous = f64::INFINITY;
        for _ in 0..200 {
            let loss = logistic_step(&features, &labels, &mut weights, &mut bias, 0.5, 1e-4);
            assert!(loss <= previous + 1e-12, "loss must not increase");
            previous = loss;
        }
        assert!(previous < 0.5);
        assert!(weights[0] > 0.0);
        assert!(weights[1] < 0.0);
    }

    #[test]
    fn weighted_step_matches_uniform_and_balances_classes() {
        let features = vec![vec![1.0], vec![1.0], vec![0.0], vec![0.0]];
        let labels = vec![true, true, false, false];
        let mut plain_weights = vec![0.0];
        let mut plain_bias = 0.0;
        let plain_loss = logistic_step(
            &features,
            &labels,
            &mut plain_weights,
            &mut plain_bias,
            0.5,
            0.0,
        );
        let mut weighted_weights = vec![0.0];
        let mut weighted_bias = 0.0;
        let weighted_loss = logistic_step_weighted(
            &features,
            &labels,
            &mut weighted_weights,
            &mut weighted_bias,
            0.5,
            0.0,
            &[1.0, 1.0, 1.0, 1.0],
        );
        assert_eq!(plain_loss, weighted_loss);
        assert_eq!(plain_weights, weighted_weights);
        assert_eq!(plain_bias, weighted_bias);

        // Nine negatives against one positive: balanced weights give the lone
        // positive half the gradient mass instead of one tenth.
        let skewed = vec![
            true, false, false, false, false, false, false, false, false, false,
        ];
        let balanced = balanced_sample_weights(&skewed);
        assert_eq!(balanced[0], 0.5);
        assert!((balanced[1..].iter().sum::<f64>() - 0.5).abs() < 1e-12);

        // Hostile weights degrade to zero instead of NaN.
        let mut untouched_weights = vec![0.25];
        let mut untouched_bias = 0.5;
        let loss = logistic_step_weighted(
            &features,
            &labels,
            &mut untouched_weights,
            &mut untouched_bias,
            0.5,
            0.0,
            &[f64::NAN, f64::INFINITY, -1.0, 0.0],
        );
        assert_eq!(loss, 0.0);
        assert_eq!(untouched_weights, vec![0.25]);
        assert_eq!(untouched_bias, 0.5);
    }

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

    #[test]
    fn training_features_cover_schema_in_order() {
        assert_eq!(TRAINING_FEATURES.len(), 10);
        assert_eq!(TRAINING_FEATURE_SCHEMA_VERSION, 1);
        let mut sorted = TRAINING_FEATURES.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), TRAINING_FEATURES.len());
    }
}
