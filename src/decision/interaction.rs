//! Model v2 — shared-state candidate interaction scorer.
//!
//! The state is encoded once; each candidate description is encoded once
//! (and cached when criteria are static); a small MLP scores the
//! interaction vector `[state, candidate, state*candidate,
//! |state - candidate|]` plus the versioned fusion features, producing one
//! logit per candidate. Softmax over candidates yields the distribution.
//!
//! The backbone stays frozen (any [`EmbeddingProvider`]); only the tiny
//! head trains. With `multilingual-e5-small` (384 dims, ~118M params) the
//! whole model stays well under 300M.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::core::capabilities::{CapabilityLevel, ProviderCapabilities};
use crate::core::error::ProviderError;
use crate::core::providers::EmbeddingProvider;

use super::fusion::{FUSION_FEATURE_SCHEMA_VERSION, fusion_feature_vector};
use super::provider::{DecisionProvider, validate_request};
use super::scoring::softmax_with_temperature;
use super::types::{
    DECISION_SCHEMA_VERSION, DecisionAnswer, DecisionModelInfo, DecisionQuestion, DecisionRequest,
    DecisionResponse, selective_decision,
};

/// Envelope version of [`InteractionArtifact`].
pub const INTERACTION_ARTIFACT_VERSION: u32 = 1;

/// Artifact kind tag for interaction heads.
pub const INTERACTION_ARTIFACT_KIND: &str = "textintel_interaction_head";

/// Architecture label for the v2 scorer.
pub const ARCHITECTURE_STATE_CANDIDATE_INTERACTION: &str = "state_candidate_interaction";

/// Default softmax temperature for interaction logits.
pub const INTERACTION_TEMPERATURE: f64 = 1.0;

/// `tanh`-approximation GELU, shared by training and inference.
pub fn gelu(x: f32) -> f32 {
    let c = (2.0f32 / std::f32::consts::PI).sqrt();
    0.5 * x * (1.0 + (c * (x + 0.044715 * x * x * x)).tanh())
}

/// Derivative of [`gelu`] for head training.
pub fn gelu_prime(x: f32) -> f32 {
    let c = (2.0f32 / std::f32::consts::PI).sqrt();
    let inner = c * (x + 0.044715 * x * x * x);
    let tanh_inner = inner.tanh();
    let sech2 = 1.0 - tanh_inner * tanh_inner;
    0.5 * (1.0 + tanh_inner) + 0.5 * x * sech2 * c * (1.0 + 3.0 * 0.044715 * x * x)
}

/// Interaction features for one (state, candidate) pair: `[state,
/// candidate, state*candidate, |state - candidate|]` with the fusion
/// vector appended when provided. Dims must agree.
pub fn interaction_features(
    state: &[f32],
    candidate: &[f32],
    fusion: Option<&[f64]>,
) -> Result<Vec<f32>, String> {
    if state.is_empty() {
        return Err("interaction needs a non-empty state embedding".to_string());
    }
    if state.len() != candidate.len() {
        return Err(format!(
            "state dim {} != candidate dim {}",
            state.len(),
            candidate.len()
        ));
    }
    if state.iter().any(|value| !value.is_finite())
        || candidate.iter().any(|value| !value.is_finite())
    {
        return Err("interaction embeddings must be finite".to_string());
    }
    let mut features =
        Vec::with_capacity(state.len() * 4 + fusion.map_or(0, |values| values.len()));
    features.extend_from_slice(state);
    features.extend_from_slice(candidate);
    features.extend(
        state
            .iter()
            .zip(candidate.iter())
            .map(|(left, right)| left * right),
    );
    features.extend(
        state
            .iter()
            .zip(candidate.iter())
            .map(|(left, right)| (left - right).abs()),
    );
    if let Some(extra) = fusion {
        if extra.iter().any(|value| !value.is_finite()) {
            return Err("interaction fusion features must be finite".to_string());
        }
        features.extend(extra.iter().map(|value| *value as f32));
    }
    Ok(features)
}

/// Tiny scoring head: `Linear(input→hidden) → GELU → Linear(hidden→1)`.
/// `w1` is row-major `hidden × input`.
#[derive(Debug, Clone, PartialEq)]
pub struct InteractionHead {
    pub input_dim: usize,
    pub hidden_dim: usize,
    pub w1: Vec<f32>,
    pub b1: Vec<f32>,
    pub w2: Vec<f32>,
    pub b2: f32,
}

impl InteractionHead {
    pub fn new(
        input_dim: usize,
        hidden_dim: usize,
        w1: Vec<f32>,
        b1: Vec<f32>,
        w2: Vec<f32>,
        b2: f32,
    ) -> Result<Self, String> {
        if input_dim == 0 || hidden_dim == 0 {
            return Err("interaction head dims must be positive".to_string());
        }
        if w1.len() != hidden_dim * input_dim {
            return Err(format!(
                "w1 has {} values, expected {}",
                w1.len(),
                hidden_dim * input_dim
            ));
        }
        if b1.len() != hidden_dim || w2.len() != hidden_dim {
            return Err("interaction head b1/w2 must match hidden_dim".to_string());
        }
        if !b2.is_finite()
            || w1.iter().any(|value| !value.is_finite())
            || b1.iter().any(|value| !value.is_finite())
            || w2.iter().any(|value| !value.is_finite())
        {
            return Err("interaction head weights must be finite".to_string());
        }
        Ok(Self {
            input_dim,
            hidden_dim,
            w1,
            b1,
            w2,
            b2,
        })
    }

    pub fn zeros(input_dim: usize, hidden_dim: usize) -> Result<Self, String> {
        Self::new(
            input_dim,
            hidden_dim,
            vec![0.0; input_dim * hidden_dim],
            vec![0.0; hidden_dim],
            vec![0.0; hidden_dim],
            0.0,
        )
    }

    pub fn parameter_count(&self) -> usize {
        self.w1.len() + self.b1.len() + self.w2.len() + 1
    }

    /// One scalar logit for a prebuilt interaction vector.
    pub fn forward(&self, features: &[f32]) -> Result<f32, String> {
        if features.len() != self.input_dim {
            return Err(format!(
                "head expects {} features, got {}",
                self.input_dim,
                features.len()
            ));
        }
        if features.iter().any(|value| !value.is_finite()) {
            return Err("head features must be finite".to_string());
        }
        let mut logit = self.b2;
        for unit in 0..self.hidden_dim {
            let row = &self.w1[unit * self.input_dim..(unit + 1) * self.input_dim];
            let mut activation = self.b1[unit];
            for (weight, value) in row.iter().zip(features.iter()) {
                activation += weight * value;
            }
            logit += self.w2[unit] * gelu(activation);
        }
        if !logit.is_finite() {
            return Err("head produced a non-finite logit".to_string());
        }
        Ok(logit)
    }
}

/// Versioned interaction-head envelope (JSON; the head is small enough
/// that safetensors would add machinery without benefit).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InteractionArtifact {
    pub artifact_version: u32,
    pub kind: String,
    pub architecture: String,
    pub decision_schema_version: u32,
    pub feature_schema_version: u32,
    pub embedding_dim: usize,
    pub fusion_features: usize,
    pub hidden_dim: usize,
    pub w1: Vec<f32>,
    pub b1: Vec<f32>,
    pub w2: Vec<f32>,
    pub b2: f32,
    #[serde(default)]
    pub dataset_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default)]
    pub metrics: BTreeMap<String, f64>,
}

impl InteractionArtifact {
    pub fn from_head(
        head: &InteractionHead,
        embedding_dim: usize,
        fusion_features: usize,
        dataset_version: impl Into<String>,
    ) -> Result<Self, String> {
        let expected = embedding_dim * 4 + fusion_features;
        if head.input_dim != expected {
            return Err(format!(
                "head input {} != 4 * {embedding_dim} + {fusion_features}",
                head.input_dim
            ));
        }
        Ok(Self {
            artifact_version: INTERACTION_ARTIFACT_VERSION,
            kind: INTERACTION_ARTIFACT_KIND.to_string(),
            architecture: ARCHITECTURE_STATE_CANDIDATE_INTERACTION.to_string(),
            decision_schema_version: DECISION_SCHEMA_VERSION,
            feature_schema_version: FUSION_FEATURE_SCHEMA_VERSION,
            embedding_dim,
            fusion_features,
            hidden_dim: head.hidden_dim,
            w1: head.w1.clone(),
            b1: head.b1.clone(),
            w2: head.w2.clone(),
            b2: head.b2,
            dataset_version: dataset_version.into(),
            revision: None,
            metrics: BTreeMap::new(),
        })
    }

    pub fn from_json(source: &str) -> Result<Self, String> {
        let artifact: Self =
            serde_json::from_str(source).map_err(|error| format!("invalid artifact: {error}"))?;
        artifact.validate()?;
        Ok(artifact)
    }

    pub fn to_json(&self) -> Result<String, String> {
        self.validate()?;
        serde_json::to_string(self).map_err(|error| error.to_string())
    }

    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let source = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        Self::from_json(&source).map_err(|error| format!("{}: {error}", path.display()))
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.artifact_version != INTERACTION_ARTIFACT_VERSION {
            return Err(format!(
                "artifact version {} is not supported (build expects {INTERACTION_ARTIFACT_VERSION})",
                self.artifact_version
            ));
        }
        if self.kind != INTERACTION_ARTIFACT_KIND {
            return Err(format!("unsupported artifact kind {:?}", self.kind));
        }
        if self.architecture != ARCHITECTURE_STATE_CANDIDATE_INTERACTION {
            return Err(format!(
                "unsupported interaction architecture {:?}",
                self.architecture
            ));
        }
        if self.decision_schema_version != DECISION_SCHEMA_VERSION {
            return Err(format!(
                "decision schema {} is not supported (build expects {DECISION_SCHEMA_VERSION})",
                self.decision_schema_version
            ));
        }
        if self.feature_schema_version != FUSION_FEATURE_SCHEMA_VERSION {
            return Err(format!(
                "feature schema {} is not supported (build expects {FUSION_FEATURE_SCHEMA_VERSION})",
                self.feature_schema_version
            ));
        }
        InteractionHead::new(
            self.embedding_dim * 4 + self.fusion_features,
            self.hidden_dim,
            self.w1.clone(),
            self.b1.clone(),
            self.w2.clone(),
            self.b2,
        )?;
        if self.metrics.values().any(|value| !value.is_finite()) {
            return Err("artifact metrics must be finite".to_string());
        }
        Ok(())
    }

    pub fn to_head(&self) -> Result<InteractionHead, String> {
        self.validate()?;
        InteractionHead::new(
            self.embedding_dim * 4 + self.fusion_features,
            self.hidden_dim,
            self.w1.clone(),
            self.b1.clone(),
            self.w2.clone(),
            self.b2,
        )
    }
}

/// v2 choice provider: one state encoding plus one cached encoding per
/// criterion, scored by the interaction head. Binary/score questions are
/// rejected; the provider embeds through the injected backbone.
pub struct InteractionDecisionProvider {
    embeddings: Arc<dyn EmbeddingProvider>,
    head: InteractionHead,
    embedding_dim: usize,
    use_fusion: bool,
    temperature: f64,
    accept_threshold: f64,
    revision: Option<String>,
}

impl InteractionDecisionProvider {
    pub fn new(
        embeddings: Arc<dyn EmbeddingProvider>,
        artifact: &InteractionArtifact,
    ) -> Result<Self, String> {
        let head = artifact.to_head()?;
        Ok(Self {
            embeddings,
            head,
            embedding_dim: artifact.embedding_dim,
            use_fusion: artifact.fusion_features > 0,
            temperature: INTERACTION_TEMPERATURE,
            accept_threshold: super::adapters::ADAPTER_ACCEPT_THRESHOLD,
            revision: artifact.revision.clone(),
        })
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
}

impl DecisionProvider for InteractionDecisionProvider {
    fn decide(&self, request: &DecisionRequest) -> Result<DecisionResponse, ProviderError> {
        let provider = self.capabilities().provider;
        validate_request(&provider, request)?;
        let DecisionQuestion::Choice { criteria, .. } = &request.question else {
            return Err(ProviderError::new(
                provider,
                "interaction decisions support only choice questions",
            ));
        };
        let fingerprint = request.fingerprint.as_ref().ok_or_else(|| {
            ProviderError::new(
                provider.clone(),
                "interaction decisions need the analyzed state fingerprint; prepare the request with the engine first",
            )
        })?;
        let mut ids: Vec<&String> = criteria.keys().collect();
        ids.sort_unstable();
        let mut texts = Vec::with_capacity(ids.len() + 1);
        texts.push(fingerprint.raw.clone());
        for id in &ids {
            let candidate = request.candidate_fingerprints.get(*id).ok_or_else(|| {
                ProviderError::new(
                    provider.clone(),
                    format!(
                        "interaction decisions need the analyzed fingerprint for criterion {id:?}; prepare the request with the engine first"
                    ),
                )
            })?;
            texts.push(candidate.raw.clone());
        }
        let vectors = self.embeddings.embed(&texts)?;
        if vectors.len() != texts.len() {
            return Err(ProviderError::new(
                provider,
                format!(
                    "embedding provider returned {} vectors for {} texts",
                    vectors.len(),
                    texts.len()
                ),
            ));
        }
        for vector in &vectors {
            if vector.len() != self.embedding_dim {
                return Err(ProviderError::new(
                    provider.clone(),
                    format!(
                        "embedding dim {} != head dim {}",
                        vector.len(),
                        self.embedding_dim
                    ),
                ));
            }
            if vector.iter().any(|value| !value.is_finite()) {
                return Err(ProviderError::new(
                    provider.clone(),
                    "embedding provider returned a non-finite vector",
                ));
            }
        }
        let fusion = self.use_fusion.then(|| fusion_feature_vector(fingerprint));
        let mut logits = Vec::with_capacity(ids.len());
        for (index, _id) in ids.iter().enumerate() {
            let features =
                interaction_features(&vectors[0], &vectors[index + 1], fusion.as_deref())
                    .map_err(|message| ProviderError::new(provider.clone(), message))?;
            let logit = self
                .head
                .forward(&features)
                .map_err(|message| ProviderError::new(provider.clone(), message))?;
            logits.push(f64::from(logit));
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
        let decision = selective_decision(answer.confidence(), self.accept_threshold);
        let mut response = DecisionResponse::new(answer, decision, provider);
        if let Some(task) = &request.task {
            response = response.with_task(task.clone());
        }
        if let Some(revision) = &self.revision {
            response = response.with_model_revision(revision.clone());
        }
        Ok(response)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        let mut capabilities = ProviderCapabilities::new("interaction_decision")
            .with_quality(CapabilityLevel::Production);
        if let Some(revision) = &self.revision {
            capabilities = capabilities.with_model_revision(revision.clone());
        }
        capabilities
    }

    fn model_info(&self) -> DecisionModelInfo {
        let mut info = DecisionModelInfo::new(
            "interaction_decision",
            ARCHITECTURE_STATE_CANDIDATE_INTERACTION,
        )
        .with_questions(["choice"]);
        if let Some(metadata) = self.embeddings.model_metadata() {
            info = info.with_model(metadata.model_id, metadata.revision);
        }
        if let Some(revision) = &self.revision {
            info.calibration_revision = Some(revision.clone());
        }
        info
    }
}

/// Deterministic xorshift64* RNG (stdlib only; training shuffles and
/// initialization must reproduce exactly).
#[derive(Debug, Clone)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^ (value >> 31)
    }

    pub fn next_f32(&mut self) -> f32 {
        // 24 random bits → [0, 1).
        ((self.next_u64() >> 40) as f32) / ((1u32 << 24) as f32)
    }

    pub fn shuffle<T>(&mut self, values: &mut [T]) {
        for index in (1..values.len()).rev() {
            let other = (self.next_u64() % (index as u64 + 1)) as usize;
            values.swap(index, other);
        }
    }
}

/// Xavier-uniform head initialization with a fixed seed.
pub fn init_head_xavier(
    input_dim: usize,
    hidden_dim: usize,
    seed: u64,
) -> Result<InteractionHead, String> {
    let mut rng = SplitMix64::new(seed);
    let mut uniform = |fan_in: usize, fan_out: usize| {
        let limit = (6.0f32 / (fan_in + fan_out) as f32).sqrt();
        rng.next_f32() * 2.0 * limit - limit
    };
    let w1 = (0..input_dim * hidden_dim)
        .map(|_| uniform(input_dim, hidden_dim))
        .collect();
    let w2 = (0..hidden_dim).map(|_| uniform(hidden_dim, 1)).collect();
    InteractionHead::new(input_dim, hidden_dim, w1, vec![0.0; hidden_dim], w2, 0.0)
}

/// One training example: prebuilt interaction vectors per candidate plus
/// the gold candidate index. Features are precomputed (and cached) so
/// head training never touches the backbone.
#[derive(Debug, Clone)]
pub struct HeadTrainExample {
    pub candidates: Vec<Vec<f32>>,
    pub gold: usize,
}

impl HeadTrainExample {
    pub fn new(candidates: Vec<Vec<f32>>, gold: usize) -> Result<Self, String> {
        if candidates.len() < 2 {
            return Err("head training needs at least 2 candidates".to_string());
        }
        if candidates.iter().any(|values| values.is_empty()) {
            return Err("head training features must not be empty".to_string());
        }
        if candidates
            .iter()
            .any(|values| values.iter().any(|value| !value.is_finite()))
        {
            return Err("head training features must be finite".to_string());
        }
        if gold >= candidates.len() {
            return Err("gold candidate index out of range".to_string());
        }
        Ok(Self { candidates, gold })
    }
}

/// Mean gradients plus mean loss: `(w1, b1, w2, b2, loss)`.
type HeadGradients = (Vec<f32>, Vec<f32>, Vec<f32>, f32, f64);

/// Mini-batch Adam trainer for the interaction head: softmax CE over the
/// candidates of each example, mean loss over the batch.
pub struct HeadTrainer {
    pub learning_rate: f32,
    pub beta1: f32,
    pub beta2: f32,
    pub epsilon: f32,
    pub batch_size: usize,
    m_w1: Vec<f32>,
    v_w1: Vec<f32>,
    m_b1: Vec<f32>,
    v_b1: Vec<f32>,
    m_w2: Vec<f32>,
    v_w2: Vec<f32>,
    m_b2: f32,
    v_b2: f32,
    step: u64,
}

impl HeadTrainer {
    pub fn new(
        head: &InteractionHead,
        learning_rate: f32,
        batch_size: usize,
    ) -> Result<Self, String> {
        if !learning_rate.is_finite() || learning_rate <= 0.0 {
            return Err("learning rate must be finite and positive".to_string());
        }
        if batch_size == 0 {
            return Err("batch size must be positive".to_string());
        }
        Ok(Self {
            learning_rate,
            beta1: 0.9,
            beta2: 0.999,
            epsilon: 1e-8,
            batch_size,
            m_w1: vec![0.0; head.w1.len()],
            v_w1: vec![0.0; head.w1.len()],
            m_b1: vec![0.0; head.b1.len()],
            v_b1: vec![0.0; head.b1.len()],
            m_w2: vec![0.0; head.w2.len()],
            v_w2: vec![0.0; head.w2.len()],
            m_b2: 0.0,
            v_b2: 0.0,
            step: 0,
        })
    }

    /// Mean softmax-CE gradients over `batch` (forward + backward).
    fn gradients(
        head: &InteractionHead,
        batch: &[HeadTrainExample],
    ) -> Result<HeadGradients, String> {
        let mut grad_w1 = vec![0.0f32; head.w1.len()];
        let mut grad_b1 = vec![0.0f32; head.b1.len()];
        let mut grad_w2 = vec![0.0f32; head.w2.len()];
        let mut grad_b2 = 0.0f32;
        let mut loss = 0.0f64;
        for example in batch {
            let mut logits = Vec::with_capacity(example.candidates.len());
            // Cache per-candidate activations for the backward pass.
            let mut cached: Vec<Vec<f32>> = Vec::with_capacity(example.candidates.len());
            for features in &example.candidates {
                if features.len() != head.input_dim {
                    return Err("training feature dim does not match the head".to_string());
                }
                let mut pre = vec![0.0f32; head.hidden_dim];
                for (unit, slot) in pre.iter_mut().enumerate() {
                    let row = &head.w1[unit * head.input_dim..(unit + 1) * head.input_dim];
                    *slot = head.b1[unit]
                        + row
                            .iter()
                            .zip(features.iter())
                            .map(|(weight, value)| weight * value)
                            .sum::<f32>();
                }
                let mut logit = head.b2;
                for (unit, value) in pre.iter().enumerate() {
                    logit += head.w2[unit] * gelu(*value);
                }
                logits.push(f64::from(logit));
                cached.push(pre);
            }
            let max = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let sum: f64 = logits.iter().map(|value| (value - max).exp()).sum();
            let probabilities: Vec<f64> = logits
                .iter()
                .map(|value| (value - max).exp() / sum)
                .collect();
            loss += -probabilities[example.gold].max(1e-12).ln();
            for (index, features) in example.candidates.iter().enumerate() {
                let target = if index == example.gold { 1.0 } else { 0.0 };
                let dlogit = (probabilities[index] - target) as f32;
                grad_b2 += dlogit;
                for unit in 0..head.hidden_dim {
                    let activation = gelu(cached[index][unit]);
                    grad_w2[unit] += dlogit * activation;
                    let dpre = dlogit * head.w2[unit] * gelu_prime(cached[index][unit]);
                    grad_b1[unit] += dpre;
                    let row = &mut grad_w1[unit * head.input_dim..(unit + 1) * head.input_dim];
                    for (grad, value) in row.iter_mut().zip(features.iter()) {
                        *grad += dpre * value;
                    }
                }
            }
        }
        let scale = 1.0 / batch.len().max(1) as f32;
        for grad in grad_w1
            .iter_mut()
            .chain(grad_b1.iter_mut())
            .chain(grad_w2.iter_mut())
        {
            *grad *= scale;
        }
        grad_b2 *= scale;
        Ok((
            grad_w1,
            grad_b1,
            grad_w2,
            grad_b2,
            loss / batch.len().max(1) as f64,
        ))
    }

    /// One Adam step over `batch`; returns the mean batch loss.
    pub fn step(
        &mut self,
        head: &mut InteractionHead,
        batch: &[HeadTrainExample],
    ) -> Result<f64, String> {
        if batch.is_empty() {
            return Err("training batch must not be empty".to_string());
        }
        let (grad_w1, grad_b1, grad_w2, grad_b2, loss) = Self::gradients(head, batch)?;
        self.step += 1;
        let step = self.step as f32;
        let beta1 = self.beta1;
        let beta2 = self.beta2;
        let epsilon = self.epsilon;
        let learning_rate = self.learning_rate;
        let fix1 = 1.0 - beta1.powf(step);
        let fix2 = 1.0 - beta2.powf(step);
        let update = |param: &mut f32, grad: f32, mean: &mut f32, var: &mut f32| {
            *mean = beta1 * *mean + (1.0 - beta1) * grad;
            *var = beta2 * *var + (1.0 - beta2) * grad * grad;
            *param -= learning_rate * (*mean / fix1) / ((*var / fix2).sqrt() + epsilon);
        };
        for (((param, grad), mean), var) in head
            .w1
            .iter_mut()
            .zip(grad_w1.iter())
            .zip(self.m_w1.iter_mut())
            .zip(self.v_w1.iter_mut())
        {
            update(param, *grad, mean, var);
        }
        for (((param, grad), mean), var) in head
            .b1
            .iter_mut()
            .zip(grad_b1.iter())
            .zip(self.m_b1.iter_mut())
            .zip(self.v_b1.iter_mut())
        {
            update(param, *grad, mean, var);
        }
        for (((param, grad), mean), var) in head
            .w2
            .iter_mut()
            .zip(grad_w2.iter())
            .zip(self.m_w2.iter_mut())
            .zip(self.v_w2.iter_mut())
        {
            update(param, *grad, mean, var);
        }
        update(&mut head.b2, grad_b2, &mut self.m_b2, &mut self.v_b2);
        if head.w1.iter().any(|value| !value.is_finite())
            || head.b1.iter().any(|value| !value.is_finite())
            || head.w2.iter().any(|value| !value.is_finite())
            || !head.b2.is_finite()
        {
            return Err("head diverged to non-finite weights".to_string());
        }
        Ok(loss)
    }
}

/// Mean softmax-CE loss plus accuracy over examples (no weight updates).
pub fn head_loss_accuracy(
    head: &InteractionHead,
    examples: &[HeadTrainExample],
) -> Result<(f64, f64), String> {
    if examples.is_empty() {
        return Ok((0.0, 0.0));
    }
    let mut loss = 0.0;
    let mut correct = 0usize;
    for example in examples {
        let mut logits = Vec::with_capacity(example.candidates.len());
        for features in &example.candidates {
            logits.push(f64::from(head.forward(features)?));
        }
        let max = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let sum: f64 = logits.iter().map(|value| (value - max).exp()).sum();
        let probabilities: Vec<f64> = logits
            .iter()
            .map(|value| (value - max).exp() / sum)
            .collect();
        loss += -probabilities[example.gold].max(1e-12).ln();
        let predicted = probabilities
            .iter()
            .enumerate()
            .max_by(|left, right| left.1.total_cmp(right.1))
            .map(|(index, _)| index)
            .unwrap_or(usize::MAX);
        if predicted == example.gold {
            correct += 1;
        }
    }
    Ok((
        loss / examples.len() as f64,
        correct as f64 / examples.len() as f64,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_head() -> InteractionHead {
        init_head_xavier(8, 4, 42).expect("head")
    }

    fn tiny_example() -> HeadTrainExample {
        HeadTrainExample::new(
            vec![
                vec![0.1, -0.2, 0.3, 0.0, 0.5, -0.1, 0.2, 0.4],
                vec![-0.3, 0.1, 0.0, 0.2, -0.4, 0.3, 0.1, -0.2],
                vec![0.0, 0.0, 0.1, -0.1, 0.2, 0.2, -0.3, 0.1],
            ],
            1,
        )
        .expect("example")
    }

    #[test]
    fn interaction_layout_concatenates_terms() {
        let state = vec![1.0, 2.0];
        let candidate = vec![3.0, -1.0];
        let features = interaction_features(&state, &candidate, Some(&[0.5])).expect("features");
        assert_eq!(
            features,
            vec![1.0, 2.0, 3.0, -1.0, 3.0, -2.0, 2.0, 3.0, 0.5]
        );
        assert!(interaction_features(&state, &[1.0], None).is_err());
        assert!(interaction_features(&[], &[], None).is_err());
    }

    #[test]
    fn gradients_match_finite_differences() {
        // Backprop is hand-written: verify against numeric gradients.
        let head = tiny_head();
        let batch = vec![tiny_example()];
        let (grad_w1, grad_b1, grad_w2, grad_b2, _) =
            HeadTrainer::gradients(&head, &batch).expect("gradients");
        let loss_of =
            |head: &InteractionHead| head_loss_accuracy(head, &batch).expect("loss").0 as f32;
        let epsilon = 1e-3f32;
        let mut numeric_w1 = vec![0.0f32; head.w1.len()];
        for (index, slot) in numeric_w1.iter_mut().enumerate() {
            let mut plus = head.clone();
            let mut minus = head.clone();
            plus.w1[index] += epsilon;
            minus.w1[index] -= epsilon;
            *slot = (loss_of(&plus) - loss_of(&minus)) / (2.0 * epsilon);
        }
        for (analytic, numeric) in grad_w1.iter().zip(numeric_w1.iter()) {
            assert!(
                (analytic - numeric).abs() < 2e-2,
                "w1 grad {analytic} vs numeric {numeric}"
            );
        }
        let mut numeric_w2 = vec![0.0f32; head.w2.len()];
        for (index, slot) in numeric_w2.iter_mut().enumerate() {
            let mut plus = head.clone();
            let mut minus = head.clone();
            plus.w2[index] += epsilon;
            minus.w2[index] -= epsilon;
            *slot = (loss_of(&plus) - loss_of(&minus)) / (2.0 * epsilon);
        }
        for (analytic, numeric) in grad_w2.iter().zip(numeric_w2.iter()) {
            assert!(
                (analytic - numeric).abs() < 2e-2,
                "w2 grad {analytic} vs numeric {numeric}"
            );
        }
        let mut plus = head.clone();
        let mut minus = head.clone();
        plus.b1[0] += epsilon;
        minus.b1[0] -= epsilon;
        let numeric_b1 = (loss_of(&plus) - loss_of(&minus)) / (2.0 * epsilon);
        assert!((grad_b1[0] - numeric_b1).abs() < 2e-2);
        plus = head.clone();
        minus = head.clone();
        plus.b2 += epsilon;
        minus.b2 -= epsilon;
        let numeric_b2 = (loss_of(&plus) - loss_of(&minus)) / (2.0 * epsilon);
        assert!((grad_b2 - numeric_b2).abs() < 2e-2);
    }

    #[test]
    fn adam_steps_descend_on_a_tiny_problem() {
        let mut head = tiny_head();
        let batch = vec![tiny_example(), tiny_example()];
        let mut trainer = HeadTrainer::new(&head, 0.05, 2).expect("trainer");
        let before = head_loss_accuracy(&head, &batch).expect("loss").0;
        for _ in 0..50 {
            trainer.step(&mut head, &batch).expect("step");
        }
        let after = head_loss_accuracy(&head, &batch).expect("loss").0;
        assert!(after < before, "loss {before} should descend to {after}");
    }

    #[test]
    fn artifacts_reject_mismatches() {
        let head = tiny_head();
        let mut artifact = InteractionArtifact::from_head(&head, 2, 0, "test").expect("artifact");
        assert!(artifact.validate().is_ok());
        assert_eq!(artifact.to_head().expect("head"), head);
        artifact.kind = "nope".to_string();
        assert!(artifact.validate().is_err());
    }

    #[test]
    fn splitmix_is_deterministic() {
        let mut first = SplitMix64::new(7);
        let mut second = SplitMix64::new(7);
        for _ in 0..16 {
            assert_eq!(first.next_u64(), second.next_u64());
        }
        let mut values = vec![1, 2, 3, 4, 5, 6, 7, 8];
        first.shuffle(&mut values);
        let mut expected = vec![1, 2, 3, 4, 5, 6, 7, 8];
        second.shuffle(&mut expected);
        assert_eq!(values, expected);
    }
}
