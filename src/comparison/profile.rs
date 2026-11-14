use crate::core::capabilities::{CapabilityLevel, ProviderCapabilities};
use crate::core::config::SimilarityWeights;
use crate::core::providers::SimilarityScorer;
use crate::core::types::{ComparisonResult, MessageFingerprint};

use super::scorer::score_fingerprints;

/// A named deterministic scoring profile. The channel calculation stays
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

/// Deterministic profile scorer: the weighted channel blend plus the
/// profile's logit calibration, exposed as a [`SimilarityScorer`] so
/// applications and the CLI can score
/// through the trait. [`SimilarityProfile::general_similarity`] is the
/// default profile.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ProfileSimilarityScorer {
    pub profile: SimilarityProfile,
}

impl ProfileSimilarityScorer {
    pub fn new(profile: SimilarityProfile) -> Self {
        Self { profile }
    }
}

impl Default for ProfileSimilarityScorer {
    fn default() -> Self {
        Self::new(SimilarityProfile::general_similarity())
    }
}

impl SimilarityScorer for ProfileSimilarityScorer {
    fn score(&self, left: &MessageFingerprint, right: &MessageFingerprint) -> ComparisonResult {
        score_fingerprints_with_profile(left, right, &self.profile)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new(format!("profile_similarity:{}", self.profile.name))
            .with_quality(CapabilityLevel::Basic)
    }
}
