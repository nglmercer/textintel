//! Adapters exposing the existing cheap specialist models through the
//! typed decision API. These validate the request/response plumbing — and
//! serve as permanent baselines and fallbacks — without any new neural
//! model.
//!
//! * [`SpamDecisionProvider`] answers binary truth questions and
//!   two-option spam/ham choices from a [`SpamPredictor`].
//! * [`SimilarityDecisionProvider`] answers choice questions by scoring the
//!   analyzed state against one analyzed reference text per criterion and
//!   applying softmax over the scores.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::core::capabilities::{CapabilityLevel, ProviderCapabilities};
use crate::core::error::ProviderError;
use crate::core::providers::{SimilarityScorer, SpamPredictor};

use super::provider::{DecisionProvider, validate_request};
use super::scoring::softmax_with_temperature;
use super::types::{
    DecisionAnswer, DecisionModelInfo, DecisionQuestion, DecisionRequest, DecisionResponse,
    selective_decision,
};

/// Default accept threshold for adapters: the argmax always accepts, so
/// callers get a usable verdict while task-specific calibration (see
/// [`crate::decision::TaskCalibration`]) stays the production path.
pub const ADAPTER_ACCEPT_THRESHOLD: f64 = 0.5;

/// Typed decisions from a spam predictor. Binary questions are answered as
/// `P(spam)`; choice questions must carry exactly the configured
/// spam/ham labels (default `"spam"`/`"ham"`). Score questions are
/// rejected: a spam probability is not an ordered severity scale.
pub struct SpamDecisionProvider {
    predictor: Arc<dyn SpamPredictor>,
    positive_id: String,
    negative_id: String,
    accept_threshold: f64,
}

impl SpamDecisionProvider {
    pub fn new(predictor: Arc<dyn SpamPredictor>) -> Self {
        Self {
            predictor,
            positive_id: "spam".to_string(),
            negative_id: "ham".to_string(),
            accept_threshold: ADAPTER_ACCEPT_THRESHOLD,
        }
    }

    /// Rename the choice labels (`positive` first). Both must be non-blank
    /// and distinct.
    pub fn with_labels(
        mut self,
        positive: impl Into<String>,
        negative: impl Into<String>,
    ) -> Result<Self, String> {
        let positive = positive.into();
        let negative = negative.into();
        if positive.trim().is_empty() || negative.trim().is_empty() {
            return Err("spam decision labels must not be blank".to_string());
        }
        if positive == negative {
            return Err("spam decision labels must be distinct".to_string());
        }
        self.positive_id = positive;
        self.negative_id = negative;
        Ok(self)
    }

    pub fn with_accept_threshold(mut self, threshold: f64) -> Result<Self, String> {
        if !threshold.is_finite() || !(0.0..=1.0).contains(&threshold) {
            return Err(format!(
                "accept threshold {threshold} must be finite and within [0.0, 1.0]"
            ));
        }
        self.accept_threshold = threshold;
        Ok(self)
    }

    fn probability(&self, request: &DecisionRequest) -> Result<f64, ProviderError> {
        let fingerprint = request.fingerprint.as_ref().ok_or_else(|| {
            ProviderError::new(
                self.capabilities().provider,
                "spam decision needs the analyzed state fingerprint; prepare the request with the engine first",
            )
        })?;
        // Adapters see no registered patterns; the engine-side spam path
        // keeps its own pattern-aware scoring.
        let result = self.predictor.predict(fingerprint, &[])?;
        if !result.probability.is_finite() {
            return Err(ProviderError::new(
                self.capabilities().provider,
                "spam predictor returned a non-finite probability",
            ));
        }
        Ok(result.probability.clamp(0.0, 1.0))
    }

    fn respond(&self, request: &DecisionRequest, answer: DecisionAnswer) -> DecisionResponse {
        let decision = selective_decision(answer.confidence(), self.accept_threshold);
        let mut response = DecisionResponse::new(answer, decision, self.capabilities().provider);
        if let Some(task) = &request.task {
            response = response.with_task(task.clone());
        }
        response
    }
}

impl DecisionProvider for SpamDecisionProvider {
    fn decide(&self, request: &DecisionRequest) -> Result<DecisionResponse, ProviderError> {
        let provider = self.capabilities().provider;
        validate_request(&provider, request)?;
        let probability = self.probability(request)?;
        let answer = match &request.question {
            DecisionQuestion::Binary { .. } => {
                let confidence = probability.max(1.0 - probability);
                DecisionAnswer::Binary {
                    probability_true: probability,
                    probability_false: 1.0 - probability,
                    confidence,
                }
            }
            DecisionQuestion::Choice { criteria, .. } => {
                let mut ids: Vec<&String> = criteria.keys().collect();
                ids.sort_unstable();
                let mut expected = vec![&self.positive_id, &self.negative_id];
                expected.sort_unstable();
                if ids != expected {
                    return Err(ProviderError::new(
                        provider,
                        format!(
                            "spam decisions support only the {:?} choice labels",
                            expected
                        ),
                    ));
                }
                let mut probabilities = BTreeMap::new();
                probabilities.insert(self.positive_id.clone(), probability);
                probabilities.insert(self.negative_id.clone(), 1.0 - probability);
                let choice = if probability >= 1.0 - probability {
                    self.positive_id.clone()
                } else {
                    self.negative_id.clone()
                };
                DecisionAnswer::Choice {
                    choice,
                    confidence: probability.max(1.0 - probability),
                    probabilities,
                }
            }
            DecisionQuestion::Score { .. } => {
                return Err(ProviderError::new(
                    provider,
                    "spam decisions do not support ordered scores",
                ));
            }
        };
        Ok(self.respond(request, answer))
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("spam_decision_adapter").with_quality(CapabilityLevel::Basic)
    }

    fn model_info(&self) -> DecisionModelInfo {
        DecisionModelInfo::new("spam_decision_adapter", "specialist_adapter")
            .with_questions(["choice", "binary"])
    }
}

/// Typed choice decisions from a similarity scorer: each criterion
/// description is an analyzed reference text, and the state is scored
/// against every reference. Softmax over the scores (with temperature)
/// yields the distribution. Binary and score questions are rejected.
pub struct SimilarityDecisionProvider {
    scorer: Arc<dyn SimilarityScorer>,
    temperature: f64,
    accept_threshold: f64,
}

impl SimilarityDecisionProvider {
    pub fn new(scorer: Arc<dyn SimilarityScorer>) -> Self {
        Self {
            scorer,
            temperature: 0.2,
            accept_threshold: ADAPTER_ACCEPT_THRESHOLD,
        }
    }

    pub fn with_temperature(mut self, temperature: f64) -> Result<Self, String> {
        if !temperature.is_finite() || temperature <= 0.0 {
            return Err(format!(
                "temperature {temperature} must be finite and positive"
            ));
        }
        self.temperature = temperature;
        Ok(self)
    }

    pub fn with_accept_threshold(mut self, threshold: f64) -> Result<Self, String> {
        if !threshold.is_finite() || !(0.0..=1.0).contains(&threshold) {
            return Err(format!(
                "accept threshold {threshold} must be finite and within [0.0, 1.0]"
            ));
        }
        self.accept_threshold = threshold;
        Ok(self)
    }

    fn respond(&self, request: &DecisionRequest, answer: DecisionAnswer) -> DecisionResponse {
        let decision = selective_decision(answer.confidence(), self.accept_threshold);
        let mut response = DecisionResponse::new(answer, decision, self.capabilities().provider);
        if let Some(task) = &request.task {
            response = response.with_task(task.clone());
        }
        response
    }
}

impl DecisionProvider for SimilarityDecisionProvider {
    fn decide(&self, request: &DecisionRequest) -> Result<DecisionResponse, ProviderError> {
        let provider = self.capabilities().provider;
        validate_request(&provider, request)?;
        let DecisionQuestion::Choice { criteria, .. } = &request.question else {
            return Err(ProviderError::new(
                provider,
                "similarity decisions support only choice questions",
            ));
        };
        let fingerprint = request.fingerprint.as_ref().ok_or_else(|| {
            ProviderError::new(
                provider.clone(),
                "similarity decisions need the analyzed state fingerprint; prepare the request with the engine first",
            )
        })?;
        let mut ids: Vec<&String> = criteria.keys().collect();
        ids.sort_unstable();
        let mut logits = Vec::with_capacity(ids.len());
        for id in &ids {
            let candidate = request.candidate_fingerprints.get(*id).ok_or_else(|| {
                ProviderError::new(
                    provider.clone(),
                    format!(
                        "similarity decisions need the analyzed fingerprint for criterion {id:?}; prepare the request with the engine first"
                    ),
                )
            })?;
            let score = self.scorer.score(fingerprint, candidate).score;
            if !score.is_finite() {
                return Err(ProviderError::new(
                    provider.clone(),
                    "similarity scorer returned a non-finite score",
                ));
            }
            logits.push(score);
        }
        let probabilities = softmax_with_temperature(&logits, self.temperature)
            .map_err(|message| ProviderError::new(provider.clone(), message))?;
        let (winner, confidence) = ids
            .iter()
            .zip(probabilities.iter())
            .max_by(|left, right| left.1.total_cmp(right.1))
            .map(|(id, value)| ((*id).clone(), *value))
            .ok_or_else(|| ProviderError::new(provider.clone(), "choice has no criteria"))?;
        let distribution: BTreeMap<String, f64> =
            ids.into_iter().cloned().zip(probabilities).collect();
        let answer = DecisionAnswer::Choice {
            choice: winner,
            confidence,
            probabilities: distribution,
        };
        Ok(self.respond(request, answer))
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("similarity_decision_adapter")
            .with_quality(CapabilityLevel::Basic)
    }

    fn model_info(&self) -> DecisionModelInfo {
        DecisionModelInfo::new("similarity_decision_adapter", "specialist_adapter")
            .with_questions(["choice"])
    }
}
