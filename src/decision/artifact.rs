//! Versioned decision-model artifacts. The JSON envelope pins the
//! decision schema, the fusion feature schema, the backbone identity, the
//! scoring head, and the calibration — and loading rejects anything that
//! does not match this build instead of silently reinterpreting it.
//!
//! Expected layout for `models/decision-v1/`:
//!
//! ```text
//! config.json               this envelope
//! model.safetensors         backbone weights (Phase 3)
//! decision-head.safetensors scoring-head weights (Phase 3)
//! calibration.json          optional calibration override
//! metadata.json             free-form training notes (informational)
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::calibration::TemperatureBias;
use super::fusion::FUSION_FEATURE_SCHEMA_VERSION;
use super::types::DECISION_SCHEMA_VERSION;

/// Envelope version of [`DecisionArtifact`]. Bumped only for breaking
/// envelope changes; schema drift inside is tracked by the pinned
/// decision/feature schema versions instead.
pub const DECISION_ARTIFACT_VERSION: u32 = 1;

/// Artifact kind tag. Anything else is rejected.
pub const DECISION_ARTIFACT_KIND: &str = "textintel_decision";

/// v1 scoring architecture: one full transformer pass per candidate, one
/// scalar logit each, softmax over candidates.
pub const ARCHITECTURE_CANDIDATE_CROSS_ENCODER: &str = "candidate_cross_encoder";

/// Backbone transformer family the current inference stack wires
/// (`model_type` in the backbone `config.json`). ModernBERT-family
/// backbones arrive with Phase 6 (`decision-modernbert`); until then they
/// are rejected with an explicit error, never silently loaded.
pub const BACKBONE_MODEL_TYPE_BERT: &str = "bert";

/// Identity and shape of the backbone transformer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionBackbone {
    pub model_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    pub model_type: String,
    pub hidden_size: usize,
}

/// Scoring head: pooled representation → Linear → GELU → Linear → logits.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionHead {
    pub hidden_size: usize,
    pub output_size: usize,
}

/// Calibration record stored with the artifact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionCalibration {
    /// `temperature_bias`, `temperature`, or `none`.
    pub method: String,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default)]
    pub biases: BTreeMap<String, f64>,
}

fn default_temperature() -> f64 {
    1.0
}

/// Dataset identity behind the trained weights.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DecisionDatasetRef {
    #[serde(default)]
    pub version: String,
}

/// Versioned decision-model envelope (stored as `config.json`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionArtifact {
    pub artifact_version: u32,
    pub kind: String,
    pub architecture: String,
    pub backbone: DecisionBackbone,
    pub decision_schema_version: u32,
    pub feature_schema_version: u32,
    pub head: DecisionHead,
    #[serde(default)]
    pub calibration: Option<DecisionCalibration>,
    #[serde(default)]
    pub dataset: DecisionDatasetRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}

impl DecisionArtifact {
    pub fn from_json(source: &str) -> Result<Self, String> {
        let artifact: Self =
            serde_json::from_str(source).map_err(|error| format!("invalid artifact: {error}"))?;
        artifact.validate()?;
        Ok(artifact)
    }

    pub fn to_json(&self) -> Result<String, String> {
        self.validate()?;
        serde_json::to_string_pretty(self).map_err(|error| error.to_string())
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let source = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        Self::from_json(&source).map_err(|error| format!("{}: {error}", path.display()))
    }

    /// Load `config.json` from a model directory, then apply an optional
    /// sibling `calibration.json` override (same shape as
    /// [`DecisionCalibration`).
    pub fn from_dir(dir: impl AsRef<Path>) -> Result<Self, String> {
        let dir = dir.as_ref();
        let mut artifact = Self::from_file(dir.join("config.json"))?;
        let override_path = dir.join("calibration.json");
        if override_path.is_file() {
            let source = std::fs::read_to_string(&override_path)
                .map_err(|error| format!("cannot read {}: {error}", override_path.display()))?;
            let calibration: DecisionCalibration = serde_json::from_str(&source)
                .map_err(|error| format!("invalid calibration override: {error}"))?;
            artifact.calibration = Some(calibration);
            artifact.validate()?;
        }
        Ok(artifact)
    }

    /// Expected sibling files for a fully materialized model directory.
    /// Inference weights land in Phase 3; until then only `config.json`
    /// (and optionally `calibration.json` / `metadata.json`) exists.
    pub fn expected_layout(dir: impl AsRef<Path>) -> Vec<PathBuf> {
        let dir = dir.as_ref();
        [
            "config.json",
            "model.safetensors",
            "decision-head.safetensors",
            "calibration.json",
            "metadata.json",
        ]
        .into_iter()
        .map(|file| dir.join(file))
        .collect()
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.artifact_version != DECISION_ARTIFACT_VERSION {
            return Err(format!(
                "artifact version {} is not supported (build expects {DECISION_ARTIFACT_VERSION})",
                self.artifact_version
            ));
        }
        if self.kind != DECISION_ARTIFACT_KIND {
            return Err(format!("unsupported artifact kind {:?}", self.kind));
        }
        if self.architecture != ARCHITECTURE_CANDIDATE_CROSS_ENCODER {
            return Err(format!(
                "unsupported decision architecture {:?}",
                self.architecture
            ));
        }
        if self.backbone.model_id.trim().is_empty() {
            return Err("artifact backbone model_id must not be empty".to_string());
        }
        if self.backbone.model_type != BACKBONE_MODEL_TYPE_BERT {
            return Err(format!(
                "backbone model_type {:?} is not wired yet (this build supports {:?}); ModernBERT-family backbones arrive with Phase 6",
                self.backbone.model_type, BACKBONE_MODEL_TYPE_BERT
            ));
        }
        if self.backbone.hidden_size == 0 {
            return Err("artifact backbone hidden_size must be positive".to_string());
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
        if self.head.hidden_size == 0 || self.head.output_size == 0 {
            return Err("artifact head sizes must be positive".to_string());
        }
        if self.head.output_size != 1 {
            return Err(format!(
                "candidate cross-encoder head output_size must be 1, got {}",
                self.head.output_size
            ));
        }
        if let Some(calibration) = &self.calibration {
            match calibration.method.as_str() {
                "temperature_bias" | "temperature" | "none" => {}
                other => {
                    return Err(format!("unsupported calibration method {other:?}"));
                }
            }
            if !calibration.temperature.is_finite() || calibration.temperature <= 0.0 {
                return Err(format!(
                    "calibration temperature {} must be finite and positive",
                    calibration.temperature
                ));
            }
            if calibration.biases.values().any(|value| !value.is_finite()) {
                return Err("calibration biases must be finite".to_string());
            }
        }
        Ok(())
    }

    /// Calibration as the runtime helper, or the identity when the
    /// artifact carries none (or method `none`).
    pub fn temperature_bias(&self) -> Result<TemperatureBias, String> {
        match &self.calibration {
            None => Ok(TemperatureBias::identity()),
            Some(calibration) if calibration.method == "none" => Ok(TemperatureBias::identity()),
            Some(calibration) => {
                TemperatureBias::new(calibration.temperature, calibration.biases.clone())
            }
        }
    }
}
