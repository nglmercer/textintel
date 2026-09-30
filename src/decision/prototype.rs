//! Small task-specific prototype classifiers over explicit local encoders.
use std::collections::BTreeMap;
use std::sync::Arc;

use super::{
    CalibrationSample, DecisionAnswer, DecisionModelInfo, DecisionProvider, DecisionQuestion,
    DecisionRequest, DecisionResponse, HeadTrainExample, fit_temperature, selective_decision,
    softmax_with_temperature, validate_request,
};
use crate::core::capabilities::{CapabilityLevel, ModelMetadata, ProviderCapabilities};
use crate::core::error::ProviderError;
use crate::core::providers::EmbeddingProvider;
use crate::semantic::similarity::cosine;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrototypeArtifact {
    pub artifact_version: u32,
    pub kind: String,
    pub embedding_model: ModelMetadata,
    pub criteria: BTreeMap<String, String>,
    pub prototypes: BTreeMap<String, Vec<f32>>,
    pub temperature: f64,
    pub dataset_version: String,
    pub revision: Option<String>,
    #[serde(default)]
    pub metrics: BTreeMap<String, f64>,
    #[serde(default)]
    pub training_config: BTreeMap<String, String>,
}

fn normalize(vector: &mut [f32]) {
    let norm = vector
        .iter()
        .map(|v| f64::from(*v).powi(2))
        .sum::<f64>()
        .sqrt() as f32;
    if norm > 0.0 {
        for value in vector {
            *value /= norm;
        }
    }
}

impl PrototypeArtifact {
    /// Blend labeled state centroids with their fixed criterion descriptions.
    /// Training and validation rows use the interaction feature layout.
    pub fn fit(
        metadata: ModelMetadata,
        criteria: BTreeMap<String, String>,
        train: &[HeadTrainExample],
        valid: &[HeadTrainExample],
        blend: f32,
        dataset_version: String,
    ) -> Result<Self, String> {
        if !blend.is_finite()
            || !(0.0..=1.0).contains(&blend)
            || train.is_empty()
            || valid.is_empty()
        {
            return Err("prototype fitting needs non-empty splits and blend within [0,1]".into());
        }
        let dim = metadata.dimensions;
        if !(1..=4096).contains(&dim) {
            return Err("prototype dimensions must be within 1..=4096".into());
        }
        let labels: Vec<_> = criteria.keys().cloned().collect();
        let mut vectors = vec![vec![0.0f32; dim]; labels.len()];
        let mut counts = vec![0usize; labels.len()];
        for example in train.iter().chain(valid) {
            if example.candidates.len() != labels.len()
                || example.gold >= labels.len()
                || example
                    .candidates
                    .iter()
                    .any(|row| row.len() < 4 * dim || row.iter().any(|v| !v.is_finite()))
            {
                return Err("prototype feature layout does not match labels/backbone".into());
            }
        }
        for example in train {
            counts[example.gold] += 1;
            for (value, state) in vectors[example.gold]
                .iter_mut()
                .zip(&example.candidates[0][..dim])
            {
                *value += state;
            }
        }
        for (index, vector) in vectors.iter_mut().enumerate() {
            if counts[index] == 0 && blend == 0.0 {
                return Err("centroid class has no training examples".into());
            }
            normalize(vector);
            let description = &train[0].candidates[index][dim..2 * dim];
            for (value, candidate) in vector.iter_mut().zip(description) {
                *value = (1.0 - blend) * *value + blend * candidate;
            }
            normalize(vector);
        }
        let prototypes: BTreeMap<_, _> = labels.into_iter().zip(vectors).collect();
        let samples: Vec<_> = valid
            .iter()
            .map(|example| {
                CalibrationSample::new(
                    prototypes
                        .values()
                        .map(|vector| cosine(&example.candidates[0][..dim], vector))
                        .collect(),
                    example.gold,
                )
            })
            .collect::<Result<_, _>>()?;
        let calibrated = fit_temperature(&samples)?;
        let artifact = Self {
            artifact_version: 1,
            kind: "textintel_prototype_classifier".into(),
            embedding_model: metadata,
            criteria,
            prototypes,
            temperature: calibrated.temperature,
            dataset_version,
            revision: None,
            metrics: BTreeMap::from([("valid_nll_calibrated".into(), calibrated.nll_after)]),
            training_config: BTreeMap::from([("prototype_blend".into(), blend.to_string())]),
        };
        artifact.validate()?;
        Ok(artifact)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.artifact_version != 1
            || self.kind != "textintel_prototype_classifier"
            || self.criteria.len() < 2
            || self.criteria.len() > super::MAX_DECISION_CANDIDATES
            || self.criteria.keys().ne(self.prototypes.keys())
            || !(1..=4096).contains(&self.embedding_model.dimensions)
            || self.prototypes.values().any(|v| {
                v.len() != self.embedding_model.dimensions || v.iter().any(|x| !x.is_finite())
            })
            || !self.temperature.is_finite()
            || self.temperature <= 0.0
            || self.metrics.values().any(|value| !value.is_finite())
        {
            return Err("invalid prototype artifact layout, version, or calibration".into());
        }
        DecisionQuestion::Choice {
            instructions: "prototype classification".into(),
            criteria: self.criteria.clone(),
        }
        .validate()?;
        Ok(())
    }

    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        let artifact: Self =
            serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        artifact.validate()?;
        Ok(artifact)
    }
}

pub struct PrototypeDecisionProvider {
    embeddings: Arc<dyn EmbeddingProvider>,
    artifact: PrototypeArtifact,
}

impl PrototypeDecisionProvider {
    pub fn new(
        embeddings: Arc<dyn EmbeddingProvider>,
        artifact: PrototypeArtifact,
    ) -> Result<Self, String> {
        artifact.validate()?;
        if embeddings.model_metadata().as_ref() != Some(&artifact.embedding_model) {
            return Err("prototype backbone identity does not match training".into());
        }
        Ok(Self {
            embeddings,
            artifact,
        })
    }
}

impl DecisionProvider for PrototypeDecisionProvider {
    fn needs_fingerprints(&self) -> bool {
        false
    }

    fn decide(&self, request: &DecisionRequest) -> Result<DecisionResponse, ProviderError> {
        let provider = "prototype_decision";
        validate_request(provider, request)?;
        let DecisionQuestion::Choice { criteria, .. } = &request.question else {
            return Err(ProviderError::new(
                provider,
                "prototype models answer fixed choice tasks",
            ));
        };
        if criteria != &self.artifact.criteria {
            return Err(ProviderError::new(
                provider,
                "criteria differ from the trained prototype task",
            ));
        }
        let vectors = self
            .embeddings
            .embed(std::slice::from_ref(&request.state))?;
        if vectors.len() != 1
            || vectors[0].len() != self.artifact.embedding_model.dimensions
            || vectors[0].iter().any(|v| !v.is_finite())
        {
            return Err(ProviderError::new(
                provider,
                "backbone returned invalid state embeddings",
            ));
        }
        let logits: Vec<_> = self
            .artifact
            .prototypes
            .values()
            .map(|v| cosine(&vectors[0], v))
            .collect();
        let probabilities = softmax_with_temperature(&logits, self.artifact.temperature)
            .map_err(|e| ProviderError::new(provider, e))?;
        let probabilities: BTreeMap<_, _> = criteria.keys().cloned().zip(probabilities).collect();
        let (choice, confidence) = probabilities
            .iter()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(id, p)| (id.clone(), *p))
            .expect("validated labels");
        let mut response = DecisionResponse::new(
            DecisionAnswer::Choice {
                choice,
                confidence,
                probabilities,
            },
            selective_decision(confidence, 0.5),
            provider,
        );
        if let Some(task) = &request.task {
            response = response.with_task(task.clone());
        }
        if let Some(revision) = &self.artifact.revision {
            response = response.with_model_revision(revision.clone());
        }
        Ok(response)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("prototype_decision").with_quality(CapabilityLevel::Basic)
    }

    fn model_info(&self) -> DecisionModelInfo {
        DecisionModelInfo::new("prototype_decision", "task_specific_prototype")
            .with_questions(["choice"])
            .with_model(
                self.artifact.embedding_model.model_id.clone(),
                self.artifact.embedding_model.revision.clone(),
            )
    }
}
