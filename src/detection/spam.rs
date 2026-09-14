use crate::core::capabilities::ProviderCapabilities;
use crate::core::error::ProviderError;
use crate::core::providers::SpamPredictor;
use crate::core::types::{MessageFingerprint, PatternMatch, SpamFeatures, SpamResult};

/// Extract stable, serializable abuse features. A predictor can be trained
/// over this structure without coupling model code to the fingerprint layout.
pub fn spam_features(fingerprint: &MessageFingerprint, patterns: &[PatternMatch]) -> SpamFeatures {
    let url_count = fingerprint
        .segments
        .iter()
        .filter(|segment| segment.segment_type == "url")
        .count();
    let email_count = fingerprint
        .segments
        .iter()
        .filter(|segment| segment.segment_type == "email")
        .count();
    let emoji_count = fingerprint
        .segments
        .iter()
        .filter(|segment| segment.segment_type == "emoji")
        .count();
    let length = fingerprint.char_features.length.max(1) as f64;
    let letters = fingerprint.char_features.letters.max(1) as f64;
    let pattern_score = patterns
        .iter()
        .map(|pattern| pattern.score)
        .fold(0.0, f64::max);
    let decoded_similarity = fingerprint
        .rebus_candidates
        .iter()
        .map(|candidate| candidate.score)
        .fold(0.0, f64::max);
    SpamFeatures {
        url_count,
        email_count,
        emoji_count,
        number_ratio: fingerprint.char_features.digits as f64 / length,
        emoji_ratio: emoji_count as f64 / length,
        uppercase_ratio: fingerprint
            .raw
            .chars()
            .filter(|character| character.is_uppercase())
            .count() as f64
            / letters,
        punctuation_ratio: fingerprint.char_features.punctuation as f64 / length,
        entropy: character_entropy(&fingerprint.raw),
        repetition_score: fingerprint.obfuscation_features.repetition_score,
        obfuscation_score: fingerprint.obfuscation_features.score,
        pattern_score,
        decoded_similarity,
        semantic_pattern_similarity: pattern_score,
        mixed_scripts: fingerprint.unicode_features.mixed_scripts,
        confusable_count: fingerprint.unicode_features.confusable_characters.len(),
    }
}

/// Explicitly named fallback predictor. It is deterministic and explainable,
/// but reports `calibrated=false` until an application supplies evaluation
/// data and a learned predictor.
#[derive(Debug, Default, Clone, Copy)]
pub struct HeuristicSpamPredictor;

impl SpamPredictor for HeuristicSpamPredictor {
    fn predict(
        &self,
        fingerprint: &MessageFingerprint,
        patterns: &[PatternMatch],
    ) -> Result<SpamResult, ProviderError> {
        Ok(predict_spam(fingerprint, patterns))
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("heuristic_spam_v1")
    }
}

pub fn predict_spam(fingerprint: &MessageFingerprint, patterns: &[PatternMatch]) -> SpamResult {
    let features = spam_features(fingerprint, patterns);
    let mut probability = 0.05;
    probability += (features.url_count as f64 * 0.22).min(0.44);
    probability += (features.email_count as f64 * 0.10).min(0.20);
    probability += (features.emoji_ratio * 0.6).min(0.18);
    probability += (features.number_ratio * 0.5).min(0.15);
    probability += features.obfuscation_score * 0.30;
    probability += features.repetition_score * 0.20;
    probability += features.pattern_score * 0.25;
    probability += (features.uppercase_ratio * 0.08).min(0.08);
    probability += (features.punctuation_ratio * 0.12).min(0.12);
    probability = probability.clamp(0.0, 1.0);

    let mut labels = Vec::new();
    let mut reasons = Vec::new();
    if features.url_count > 0 {
        labels.push("promotion".to_string());
        reasons.push("Contains one or more URLs".to_string());
    }
    if fingerprint.obfuscation_features.leetspeak || fingerprint.obfuscation_features.confusables {
        labels.push("obfuscated".to_string());
        reasons.push("Obfuscation signals were detected".to_string());
    }
    if features.repetition_score > 0.08 {
        labels.push("flooding".to_string());
        reasons.push("Repeated characters suggest message flooding".to_string());
    }
    if features.pattern_score >= 0.75 {
        labels.push("known_pattern".to_string());
        reasons.push("Message matched a registered pattern".to_string());
    }
    if features.emoji_count >= 2 || features.number_ratio > 0.20 {
        labels.push("attention_grabbing".to_string());
        reasons.push("High emoji or number ratio".to_string());
    }
    if features.entropy > 4.5 && fingerprint.char_features.length >= 12 {
        labels.push("high_entropy".to_string());
        reasons.push("Message has an unusually diverse character distribution".to_string());
    }
    if labels.is_empty() && probability >= 0.5 {
        labels.push("suspicious".to_string());
        reasons.push("Combined spam signals exceeded the warning threshold".to_string());
    }
    SpamResult {
        probability,
        labels,
        reasons,
        features,
        calibrated: false,
        model: "heuristic-v1".to_string(),
    }
}

/// Schema version of the spam training feature vector.
pub const SPAM_FEATURE_SCHEMA_VERSION: u32 = 1;

/// Interpretable spam features in fixed order. Counts are raw; ratios and
/// scores already lie in `[0.0, 1.0]`; `mixed_scripts` encodes as 0.0/1.0.
pub const SPAM_FEATURES: &[&str] = &[
    "url_count",
    "email_count",
    "emoji_count",
    "number_ratio",
    "emoji_ratio",
    "uppercase_ratio",
    "punctuation_ratio",
    "entropy",
    "repetition_score",
    "obfuscation_score",
    "pattern_score",
    "decoded_similarity",
    "semantic_pattern_similarity",
    "mixed_scripts",
    "confusable_count",
];

/// Ordered feature vector matching [`SPAM_FEATURES`].
pub fn spam_feature_vector(features: &SpamFeatures) -> Vec<f64> {
    vec![
        features.url_count as f64,
        features.email_count as f64,
        features.emoji_count as f64,
        features.number_ratio,
        features.emoji_ratio,
        features.uppercase_ratio,
        features.punctuation_ratio,
        features.entropy,
        features.repetition_score,
        features.obfuscation_score,
        features.pattern_score,
        features.decoded_similarity,
        features.semantic_pattern_similarity,
        if features.mixed_scripts { 1.0 } else { 0.0 },
        features.confusable_count as f64,
    ]
}

/// Versioned trained-spam artifact. `calibrated` is true only for artifacts
/// produced by the training tool, which fits the bias on held-out data.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SpamModelArtifact {
    pub artifact_version: u32,
    pub kind: String,
    pub feature_schema_version: u32,
    pub dataset_version: String,
    pub weights: std::collections::BTreeMap<String, f64>,
    pub bias: f64,
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub calibrated: bool,
    #[serde(default)]
    pub metrics: std::collections::BTreeMap<String, f64>,
}

impl SpamModelArtifact {
    pub fn new(
        dataset_version: impl Into<String>,
        weights: std::collections::BTreeMap<String, f64>,
        bias: f64,
    ) -> Self {
        Self {
            artifact_version: 1,
            kind: "logistic_spam".to_string(),
            feature_schema_version: SPAM_FEATURE_SCHEMA_VERSION,
            dataset_version: dataset_version.into(),
            weights,
            bias,
            revision: None,
            calibrated: false,
            metrics: std::collections::BTreeMap::new(),
        }
    }

    pub fn with_revision(mut self, revision: impl Into<String>) -> Self {
        self.revision = Some(revision.into());
        self
    }

    pub fn with_calibrated(mut self, calibrated: bool) -> Self {
        self.calibrated = calibrated;
        self
    }

    pub fn with_metrics(mut self, metrics: std::collections::BTreeMap<String, f64>) -> Self {
        self.metrics = metrics;
        self
    }

    /// Deserialize and validate: kind, feature schema, and weight names must
    /// match this build, otherwise the artifact is rejected, never guessed.
    pub fn from_json(source: &str) -> Result<Self, String> {
        let artifact: Self =
            serde_json::from_str(source).map_err(|error| format!("invalid artifact: {error}"))?;
        if artifact.kind != "logistic_spam" {
            return Err(format!("unsupported artifact kind {:?}", artifact.kind));
        }
        if artifact.feature_schema_version != SPAM_FEATURE_SCHEMA_VERSION {
            return Err(format!(
                "feature schema {} is not supported (build expects {})",
                artifact.feature_schema_version, SPAM_FEATURE_SCHEMA_VERSION
            ));
        }
        let mut names: Vec<&str> = artifact.weights.keys().map(String::as_str).collect();
        names.sort_unstable();
        let mut sorted = SPAM_FEATURES.to_vec();
        sorted.sort_unstable();
        if names != sorted {
            return Err(format!(
                "artifact weights {names:?} do not match spam features {sorted:?}"
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

    pub fn to_predictor(&self) -> TrainedSpamPredictor {
        TrainedSpamPredictor {
            artifact: self.clone(),
        }
    }
}

/// Learned spam predictor. Keeps the heuristic predictor's output shape
/// (`probability`, `labels`, `reasons`) and adds weight-based reasons: the
/// top contributing features for each decision.
#[derive(Debug, Clone)]
pub struct TrainedSpamPredictor {
    artifact: SpamModelArtifact,
}

impl TrainedSpamPredictor {
    pub fn new(artifact: SpamModelArtifact) -> Result<Self, String> {
        SpamModelArtifact::from_json(&artifact.to_json().map_err(|error| error.to_string())?)?;
        Ok(Self { artifact })
    }

    pub fn artifact(&self) -> &SpamModelArtifact {
        &self.artifact
    }

    fn probability(&self, features: &SpamFeatures) -> f64 {
        let vector = spam_feature_vector(features);
        let mut logit = self.artifact.bias;
        for (index, name) in SPAM_FEATURES.iter().enumerate() {
            logit += vector[index] * self.artifact.weights.get(*name).copied().unwrap_or(0.0);
        }
        crate::comparison::sigmoid(logit)
    }
}

impl SpamPredictor for TrainedSpamPredictor {
    fn predict(
        &self,
        fingerprint: &MessageFingerprint,
        patterns: &[PatternMatch],
    ) -> Result<SpamResult, ProviderError> {
        let features = spam_features(fingerprint, patterns);
        let vector = spam_feature_vector(&features);
        let probability = self.probability(&features);
        let mut contributions: Vec<(&str, f64)> = SPAM_FEATURES
            .iter()
            .zip(vector.iter())
            .map(|(name, value)| {
                (
                    *name,
                    value * self.artifact.weights.get(*name).copied().unwrap_or(0.0),
                )
            })
            .collect();
        contributions.sort_by(|left, right| {
            right
                .1
                .abs()
                .total_cmp(&left.1.abs())
                .then_with(|| left.0.cmp(right.0))
        });
        let mut labels = Vec::new();
        if probability >= 0.5 {
            labels.push("spam".to_string());
        }
        let reasons = contributions
            .iter()
            .take(3)
            .map(|(name, contribution)| format!("{name} ({contribution:+.2})"))
            .collect();
        Ok(SpamResult {
            probability,
            labels,
            reasons,
            features,
            calibrated: self.artifact.calibrated,
            model: format!(
                "trained-spam:{}",
                self.artifact.revision.as_deref().unwrap_or("v1")
            ),
        })
    }

    fn capabilities(&self) -> ProviderCapabilities {
        let mut capabilities = ProviderCapabilities::new("trained_spam");
        if let Some(revision) = &self.artifact.revision {
            capabilities = capabilities.with_version(revision.clone());
        }
        capabilities
    }
}

fn character_entropy(text: &str) -> f64 {
    if text.is_empty() {
        return 0.0;
    }
    let mut counts = std::collections::BTreeMap::<char, usize>::new();
    for character in text.chars() {
        *counts.entry(character).or_default() += 1;
    }
    let length = text.chars().count() as f64;
    counts
        .values()
        .map(|count| {
            let probability = *count as f64 / length;
            -probability * probability.log2()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_vector_covers_schema_in_order() {
        assert_eq!(SPAM_FEATURES.len(), 15);
        assert_eq!(SPAM_FEATURE_SCHEMA_VERSION, 1);
        let features = SpamFeatures::default();
        assert_eq!(spam_feature_vector(&features).len(), SPAM_FEATURES.len());
        let mixed = SpamFeatures {
            mixed_scripts: true,
            url_count: 2,
            ..SpamFeatures::default()
        };
        let vector = spam_feature_vector(&mixed);
        assert_eq!(vector[0], 2.0);
        assert_eq!(vector[SPAM_FEATURES.len() - 2], 1.0);
    }

    #[test]
    fn artifact_round_trip_and_rejection() {
        let weights: std::collections::BTreeMap<String, f64> = SPAM_FEATURES
            .iter()
            .map(|name| ((*name).to_string(), 0.1))
            .collect();
        let artifact = SpamModelArtifact::new("synthetic-spam-v1", weights, -1.0)
            .with_revision("r1")
            .with_calibrated(true);
        let loaded = SpamModelArtifact::from_json(&artifact.to_json().unwrap()).unwrap();
        assert_eq!(loaded, artifact);
        assert!(loaded.calibrated);

        let mut source = serde_json::to_value(&artifact).unwrap();
        source["kind"] = serde_json::Value::String("other".to_string());
        assert!(SpamModelArtifact::from_json(&source.to_string()).is_err());

        let mut source = serde_json::to_value(&artifact).unwrap();
        source["weights"]
            .as_object_mut()
            .unwrap()
            .remove("url_count");
        assert!(SpamModelArtifact::from_json(&source.to_string()).is_err());
    }

    #[test]
    fn predictor_reports_labels_reasons_and_calibration() {
        let weights: std::collections::BTreeMap<String, f64> = SPAM_FEATURES
            .iter()
            .map(|name| ((*name).to_string(), 0.0))
            .collect();
        let mut biased = weights;
        biased.insert("url_count".to_string(), 5.0);
        let predictor = SpamModelArtifact::new("test", biased, -2.0)
            .with_calibrated(true)
            .to_predictor();
        let engine = crate::TextIntelligence::default();
        let fingerprint = engine
            .analyze("claim now at http://example.com/win")
            .unwrap();
        let result = predictor.predict(&fingerprint, &[]).unwrap();
        assert!(result.probability > 0.5);
        assert_eq!(result.labels, vec!["spam".to_string()]);
        assert!(!result.reasons.is_empty());
        assert!(result.calibrated);
        assert!(result.model.starts_with("trained-spam:"));

        let clean = engine.analyze("see you at noon").unwrap();
        let ham = predictor.predict(&clean, &[]).unwrap();
        assert!(ham.probability < 0.5);
        assert!(ham.labels.is_empty());

        // Uncalibrated artifacts propagate calibrated=false.
        let plain = SpamModelArtifact::new(
            "test",
            SPAM_FEATURES
                .iter()
                .map(|name| ((*name).to_string(), 0.0))
                .collect(),
            0.0,
        )
        .to_predictor();
        assert!(!plain.predict(&clean, &[]).unwrap().calibrated);
    }
}
