//! Typed decision requests, answers, and responses.
//!
//! A decision request pairs shared context (`state`) with one bounded
//! [`DecisionQuestion`]. Providers return a [`DecisionAnswer`] carrying a
//! full probability distribution — never prose — plus a selective
//! [`Decision`] (`Accept` or `Escalate`). Pre-analyzed evidence travels on
//! the request (`fingerprint`, `candidate_fingerprints`) so providers stay
//! pure scorers; [`crate::engine::TextIntelligence::decide`] fills those
//! fields from `state` before dispatching.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::core::types::MessageFingerprint;

/// Version of the decision request/response schema. Artifacts pin this value
/// and are rejected when it does not match the build.
pub const DECISION_SCHEMA_VERSION: u32 = 1;

/// Upper bound on choice candidates and score levels per request. Decision
/// latency grows with the candidate count (see the v1 cross-encoder), so
/// oversized requests are rejected instead of silently truncated.
pub const MAX_DECISION_CANDIDATES: usize = 64;

/// Upper bound on `state` length in characters. Longer inputs are rejected;
/// callers chunk or summarize first.
pub const MAX_DECISION_STATE_CHARS: usize = 8_192;

/// Upper bound on [`crate::decision::DecisionProvider::decide_batch`] length.
pub const MAX_DECISION_BATCH: usize = 256;

/// Tolerance for probability-sum checks (`sum ≈ 1.0`).
pub const PROBABILITY_SUM_TOLERANCE: f64 = 1e-6;

/// Conventional criterion id meaning "none of the other options applies".
/// Providers treat it as an ordinary candidate; evaluation and callers use
/// its probability as an explicit out-of-distribution signal instead of
/// trusting raw max-softmax confidence alone.
pub const NONE_OF_THE_ABOVE: &str = "NONE_OF_THE_ABOVE";

/// One bounded question over shared state. This is the whole output surface:
/// `choice`, `binary`, or `score`. There is no free-text variant by design.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DecisionQuestion {
    /// Pick one of `criteria` (`id -> description`). Descriptions are
    /// provider-scored reference texts; ids are the stable answer labels.
    Choice {
        instructions: String,
        criteria: BTreeMap<String, String>,
    },
    /// Truth decision over `statement`. Returns `P(true)` / `P(false)`.
    Binary { statement: String },
    /// Ordered levels (`levels[i]` is the label of score `i`). Returns the
    /// full distribution plus the expected score `Σ p_i * i`.
    Score {
        instructions: String,
        levels: Vec<String>,
    },
}

impl DecisionQuestion {
    /// Short question-type label (`choice`, `binary`, `score`).
    pub fn question_type(&self) -> &'static str {
        match self {
            Self::Choice { .. } => "choice",
            Self::Binary { .. } => "binary",
            Self::Score { .. } => "score",
        }
    }

    /// Candidate count (criteria, 2, or levels).
    pub fn candidate_count(&self) -> usize {
        match self {
            Self::Choice { criteria, .. } => criteria.len(),
            Self::Binary { .. } => 2,
            Self::Score { levels, .. } => levels.len(),
        }
    }

    /// Stable candidate labels in scoring order (criteria ids sorted,
    /// `["false", "true"]`, or level indexes as strings).
    pub fn candidate_labels(&self) -> Vec<String> {
        match self {
            Self::Choice { criteria, .. } => criteria.keys().cloned().collect(),
            Self::Binary { .. } => vec!["false".to_string(), "true".to_string()],
            Self::Score { levels, .. } => {
                (0..levels.len()).map(|index| index.to_string()).collect()
            }
        }
    }

    /// Structural validation: non-empty content, candidate bounds, no blank
    /// ids or descriptions.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Choice {
                instructions,
                criteria,
            } => {
                if instructions.trim().is_empty() {
                    return Err("choice instructions must not be empty".to_string());
                }
                if criteria.len() < 2 {
                    return Err(format!(
                        "choice needs at least 2 criteria, got {}",
                        criteria.len()
                    ));
                }
                if criteria.len() > MAX_DECISION_CANDIDATES {
                    return Err(format!(
                        "choice has {} criteria, maximum is {MAX_DECISION_CANDIDATES}",
                        criteria.len()
                    ));
                }
                for (id, description) in criteria {
                    if id.trim().is_empty() {
                        return Err("choice criterion id must not be blank".to_string());
                    }
                    if description.trim().is_empty() {
                        return Err(format!(
                            "choice criterion {id:?} description must not be empty"
                        ));
                    }
                }
                Ok(())
            }
            Self::Binary { statement } => {
                if statement.trim().is_empty() {
                    return Err("binary statement must not be empty".to_string());
                }
                Ok(())
            }
            Self::Score {
                instructions,
                levels,
            } => {
                if instructions.trim().is_empty() {
                    return Err("score instructions must not be empty".to_string());
                }
                if levels.len() < 2 {
                    return Err(format!(
                        "score needs at least 2 levels, got {}",
                        levels.len()
                    ));
                }
                if levels.len() > MAX_DECISION_CANDIDATES {
                    return Err(format!(
                        "score has {} levels, maximum is {MAX_DECISION_CANDIDATES}",
                        levels.len()
                    ));
                }
                for (index, level) in levels.iter().enumerate() {
                    if level.trim().is_empty() {
                        return Err(format!("score level {index} label must not be empty"));
                    }
                }
                Ok(())
            }
        }
    }
}

/// One typed decision request: shared `state` plus a single question.
///
/// Text evidence is attached out-of-band: `fingerprint` (analyzed `state`)
/// and, for choice questions, one analyzed fingerprint per criterion id.
/// These fields never serialize — `request.json` carries `state` and
/// `question` only — and the engine fills them before dispatch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionRequest {
    /// Shared context the question is asked about (customer message, …).
    pub state: String,
    /// The single bounded question to answer.
    pub question: DecisionQuestion,
    /// Task name for task-specific calibration/threshold lookup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    /// Analyzed `state`. `None` until the engine prepares the request.
    #[serde(skip)]
    pub fingerprint: Option<MessageFingerprint>,
    /// Analyzed criterion descriptions, keyed by criterion id. The engine
    /// prepares these for choice questions; providers must not re-analyze.
    #[serde(skip)]
    pub candidate_fingerprints: BTreeMap<String, MessageFingerprint>,
}

impl DecisionRequest {
    pub fn new(state: impl Into<String>, question: DecisionQuestion) -> Self {
        Self {
            state: state.into(),
            question,
            task: None,
            fingerprint: None,
            candidate_fingerprints: BTreeMap::new(),
        }
    }

    pub fn with_task(mut self, task: impl Into<String>) -> Self {
        self.task = Some(task.into());
        self
    }

    pub fn with_fingerprint(mut self, fingerprint: MessageFingerprint) -> Self {
        self.fingerprint = Some(fingerprint);
        self
    }

    /// Structural validation of the serializable fields (state + question).
    /// Evidence attachment is checked by providers, not here.
    pub fn validate(&self) -> Result<(), String> {
        let chars = self.state.chars().count();
        if self.state.trim().is_empty() {
            return Err("decision state must not be empty".to_string());
        }
        if chars > MAX_DECISION_STATE_CHARS {
            return Err(format!(
                "decision state has {chars} characters, maximum is {MAX_DECISION_STATE_CHARS}"
            ));
        }
        if let Some(task) = &self.task
            && task.trim().is_empty()
        {
            return Err("decision task must not be blank when set".to_string());
        }
        self.question.validate()
    }
}

/// A probabilistic answer: always a distribution, never prose.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DecisionAnswer {
    /// Winning criterion id plus the full distribution.
    Choice {
        choice: String,
        confidence: f64,
        probabilities: BTreeMap<String, f64>,
    },
    /// Truth probabilities. `confidence` is `max(p_true, p_false)`.
    Binary {
        probability_true: f64,
        probability_false: f64,
        confidence: f64,
    },
    /// Expected score `Σ p_i * i` plus the level distribution.
    Score {
        expected_score: f64,
        confidence: f64,
        probabilities: Vec<f64>,
    },
}

impl DecisionAnswer {
    /// Reported confidence (max probability of the distribution).
    pub fn confidence(&self) -> f64 {
        match self {
            Self::Choice { confidence, .. }
            | Self::Binary { confidence, .. }
            | Self::Score { confidence, .. } => *confidence,
        }
    }

    /// Predicted label in [`DecisionQuestion::candidate_labels`] space:
    /// criterion id, `"true"`/`"false"`, or level index as a string.
    pub fn predicted_label(&self) -> String {
        match self {
            Self::Choice { choice, .. } => choice.clone(),
            Self::Binary {
                probability_true,
                probability_false,
                ..
            } => {
                if probability_true >= probability_false {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            }
            Self::Score { probabilities, .. } => probabilities
                .iter()
                .enumerate()
                .max_by(|left, right| left.1.total_cmp(right.1))
                .map(|(index, _)| index.to_string())
                .unwrap_or_default(),
        }
    }

    /// Distribution values aligned with [`DecisionQuestion::candidate_labels`].
    pub fn probabilities_in_order(&self) -> Vec<f64> {
        match self {
            Self::Choice { probabilities, .. } => probabilities.values().copied().collect(),
            Self::Binary {
                probability_true,
                probability_false,
                ..
            } => vec![*probability_false, *probability_true],
            Self::Score { probabilities, .. } => probabilities.clone(),
        }
    }

    /// Cross-check an answer against its question: matching ids/levels,
    /// finite probabilities in `[0, 1]` summing to ≈ 1, a known winner, and
    /// a consistent confidence value.
    pub fn validate_against(&self, question: &DecisionQuestion) -> Result<(), String> {
        let check_distribution = |values: &[f64]| -> Result<(), String> {
            if values.iter().any(|value| !value.is_finite()) {
                return Err("decision probabilities must be finite".to_string());
            }
            if values.iter().any(|value| !(0.0..=1.0).contains(value)) {
                return Err("decision probabilities must be within [0.0, 1.0]".to_string());
            }
            let sum: f64 = values.iter().sum();
            if (sum - 1.0).abs() > PROBABILITY_SUM_TOLERANCE {
                return Err(format!(
                    "decision probabilities sum to {sum}, expected ≈ 1.0"
                ));
            }
            Ok(())
        };
        let check_confidence = |confidence: f64, expected: f64| -> Result<(), String> {
            if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
                return Err("decision confidence must be finite and within [0.0, 1.0]".to_string());
            }
            if (confidence - expected).abs() > PROBABILITY_SUM_TOLERANCE {
                return Err(format!(
                    "decision confidence {confidence} does not match max probability {expected}"
                ));
            }
            Ok(())
        };
        match (self, question) {
            (
                Self::Choice {
                    choice,
                    confidence,
                    probabilities,
                },
                DecisionQuestion::Choice { criteria, .. },
            ) => {
                let mut expected: Vec<&String> = criteria.keys().collect();
                let mut observed: Vec<&String> = probabilities.keys().collect();
                expected.sort_unstable();
                observed.sort_unstable();
                if expected != observed {
                    return Err(
                        "choice probabilities must cover exactly the criterion ids".to_string()
                    );
                }
                if !criteria.contains_key(choice) {
                    return Err(format!("unknown choice answer {choice:?}"));
                }
                let values: Vec<f64> = probabilities.values().copied().collect();
                check_distribution(&values)?;
                let max = values.iter().copied().fold(0.0, f64::max);
                check_confidence(*confidence, max)?;
                let winner = probabilities[choice];
                if (winner - max).abs() > PROBABILITY_SUM_TOLERANCE {
                    return Err("choice winner is not the max-probability candidate".to_string());
                }
                Ok(())
            }
            (
                Self::Binary {
                    probability_true,
                    probability_false,
                    confidence,
                },
                DecisionQuestion::Binary { .. },
            ) => {
                check_distribution(&[*probability_true, *probability_false])?;
                check_confidence(*confidence, probability_true.max(*probability_false))?;
                Ok(())
            }
            (
                Self::Score {
                    expected_score,
                    confidence,
                    probabilities,
                },
                DecisionQuestion::Score { levels, .. },
            ) => {
                if probabilities.len() != levels.len() {
                    return Err(format!(
                        "score has {} probabilities for {} levels",
                        probabilities.len(),
                        levels.len()
                    ));
                }
                check_distribution(probabilities)?;
                let expected: f64 = probabilities
                    .iter()
                    .enumerate()
                    .map(|(index, value)| index as f64 * value)
                    .sum();
                if (expected_score - expected).abs() > PROBABILITY_SUM_TOLERANCE {
                    return Err("score expected_score does not match Σ p_i * i".to_string());
                }
                let max = probabilities.iter().copied().fold(0.0, f64::max);
                check_confidence(*confidence, max)?;
                Ok(())
            }
            _ => Err("decision answer type does not match the question type".to_string()),
        }
    }
}

/// Selective-classification verdict: act on the answer or escalate it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    #[default]
    Escalate,
    Accept,
}

/// Threshold rule for [`Decision`]. Non-finite confidence escalates
/// (fail-closed); otherwise `confidence >= threshold` accepts.
pub fn selective_decision(confidence: f64, threshold: f64) -> Decision {
    if !confidence.is_finite() || !threshold.is_finite() {
        return Decision::Escalate;
    }
    if confidence >= threshold {
        Decision::Accept
    } else {
        Decision::Escalate
    }
}

/// Complete provider output: answer, selective verdict, and provenance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionResponse {
    pub answer: DecisionAnswer,
    pub decision: Decision,
    /// Provider name (see capabilities).
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    /// Model/artifact revision behind the answer, when any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_revision: Option<String>,
}

impl DecisionResponse {
    pub fn new(answer: DecisionAnswer, decision: Decision, provider: impl Into<String>) -> Self {
        Self {
            answer,
            decision,
            provider: provider.into(),
            task: None,
            model_revision: None,
        }
    }

    pub fn with_task(mut self, task: impl Into<String>) -> Self {
        self.task = Some(task.into());
        self
    }

    pub fn with_model_revision(mut self, revision: impl Into<String>) -> Self {
        self.model_revision = Some(revision.into());
        self
    }

    /// Confidence of the carried answer.
    pub fn confidence(&self) -> f64 {
        self.answer.confidence()
    }

    /// Validate the carried answer against the originating request.
    pub fn validate_against(&self, request: &DecisionRequest) -> Result<(), String> {
        self.answer.validate_against(&request.question)
    }
}

/// Static model facts for `decision-model-info` and diagnostics: identity,
/// architecture, schemas, calibration, capabilities, and bounds. Counts and
/// names only — never user text or weights.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecisionModelInfo {
    pub provider: String,
    pub architecture: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precision: Option<String>,
    pub decision_schema: u32,
    pub feature_schema: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calibration_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calibration_revision: Option<String>,
    /// Supported question types (`choice`, `binary`, `score` subset).
    pub supported_questions: Vec<String>,
    pub local: bool,
    pub max_candidates: usize,
    pub max_state_chars: usize,
}

impl DecisionModelInfo {
    pub fn new(provider: impl Into<String>, architecture: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            architecture: architecture.into(),
            model_id: None,
            model_revision: None,
            precision: None,
            decision_schema: DECISION_SCHEMA_VERSION,
            feature_schema: crate::decision::fusion::FUSION_FEATURE_SCHEMA_VERSION,
            calibration_method: None,
            calibration_revision: None,
            supported_questions: Vec::new(),
            local: true,
            max_candidates: MAX_DECISION_CANDIDATES,
            max_state_chars: MAX_DECISION_STATE_CHARS,
        }
    }

    pub fn with_model(mut self, id: impl Into<String>, revision: Option<String>) -> Self {
        self.model_id = Some(id.into());
        self.model_revision = revision;
        self
    }

    pub fn with_questions<I, S>(mut self, questions: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.supported_questions = questions.into_iter().map(Into::into).collect();
        self
    }

    pub fn remote(mut self) -> Self {
        self.local = false;
        self
    }
}
