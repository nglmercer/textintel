//! v1 candidate cross-encoder support: prompt format, candidate batch
//! planning, and backbone validation.
//!
//! v1 scores each candidate with one full transformer pass over a prompt of
//! the form:
//!
//! ```text
//! [CLS]
//!
//! QUESTION:
//! Which category applies?
//!
//! OPTION:
//! billing
//! Payments, invoices and refunds.
//!
//! STATE:
//! customer message
//!
//! [SEP]
//! ```
//!
//! The pooled representation feeds a small MLP head producing one scalar
//! logit per candidate; softmax over candidates yields the distribution.
//! This module owns the pure parts (prompt building, batch planning, head
//! configuration). Candle BERT inference wiring lands in Phase 3; the
//! feature-gated [`TransformerBackbone`] handle validates backbone
//! directories against the already-wired BERT stack so `config.json`
//! mistakes fail early with explicit errors.

use super::artifact::DecisionArtifact;
use super::provider::validate_request;
use super::types::{DecisionQuestion, DecisionRequest};

/// One candidate rendered for cross-encoder scoring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidatePrompt {
    pub option_id: String,
    pub prompt: String,
}

/// Render the v1 cross-encoder prompt for one candidate. Sections are
/// fixed-order (`QUESTION`, `OPTION`, `STATE`); the tokenizer adds the
/// `[CLS]`/`[SEP]` markers. Inputs must be non-blank after trimming.
pub fn candidate_prompt(
    instructions: &str,
    option_id: &str,
    option_description: &str,
    state: &str,
) -> Result<String, String> {
    if instructions.trim().is_empty() {
        return Err("candidate prompt instructions must not be empty".to_string());
    }
    if option_id.trim().is_empty() {
        return Err("candidate prompt option id must not be empty".to_string());
    }
    if option_description.trim().is_empty() {
        return Err("candidate prompt option description must not be empty".to_string());
    }
    if state.trim().is_empty() {
        return Err("candidate prompt state must not be empty".to_string());
    }
    Ok(format!(
        "QUESTION:\n{instructions}\n\nOPTION:\n{option_id}\n{option_description}\n\nSTATE:\n{state}"
    ))
}

/// Plan the v1 candidate batch for a choice request: one prompt per
/// criterion, in sorted-id order. Non-choice questions are rejected —
/// binary/score use their own prompt shapes in Phase 3.
pub fn plan_candidate_batch(request: &DecisionRequest) -> Result<Vec<CandidatePrompt>, String> {
    request.validate()?;
    let DecisionQuestion::Choice {
        instructions,
        criteria,
    } = &request.question
    else {
        return Err("candidate batching supports only choice questions".to_string());
    };
    let mut ids: Vec<&String> = criteria.keys().collect();
    ids.sort_unstable();
    ids.into_iter()
        .map(|id| {
            candidate_prompt(instructions, id, &criteria[id], &request.state).map(|prompt| {
                CandidatePrompt {
                    option_id: id.clone(),
                    prompt,
                }
            })
        })
        .collect()
}

/// Scoring-head configuration resolved from a validated artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrossEncoderHeadConfig {
    pub hidden_size: usize,
    pub output_size: usize,
}

impl CrossEncoderHeadConfig {
    pub fn from_artifact(artifact: &DecisionArtifact) -> Result<Self, String> {
        artifact.validate()?;
        Ok(Self {
            hidden_size: artifact.head.hidden_size,
            output_size: artifact.head.output_size,
        })
    }
}

/// Validated backbone directory for a decision model (feature
/// `decision-transformer`). Opening reuses the already-wired BERT stack,
/// so incompatible checkpoints (wrong `model_type`, mismatched
/// tokenizer, unreadable `model.safetensors`) fail here with explicit
/// errors. Scoring-head inference lands in Phase 3; until then this
/// handle answers identity questions (model id, revision, dimensions)
/// for validation and `decision-model-info`.
#[cfg(feature = "decision-transformer")]
#[derive(Debug)]
pub struct TransformerBackbone {
    directory: std::path::PathBuf,
    provider: crate::semantic::TransformerEmbeddingProvider,
}

#[cfg(feature = "decision-transformer")]
impl TransformerBackbone {
    pub fn open(
        directory: impl AsRef<std::path::Path>,
    ) -> Result<Self, crate::core::error::ProviderError> {
        use crate::core::providers::EmbeddingProvider;
        let provider = crate::semantic::TransformerEmbeddingProvider::open(&directory)?;
        // The embedding provider validates shapes eagerly; decisions only
        // need its identity until the Phase 3 head wires in.
        let _ = provider.model_metadata();
        Ok(Self {
            directory: directory.as_ref().to_path_buf(),
            provider,
        })
    }

    pub fn directory(&self) -> &std::path::Path {
        &self.directory
    }

    pub fn model_metadata(&self) -> Option<crate::core::capabilities::ModelMetadata> {
        use crate::core::providers::EmbeddingProvider;
        self.provider.model_metadata()
    }

    /// Cross-check a decision artifact against this backbone: the hidden
    /// size must match the backbone dimensions.
    pub fn check_artifact(&self, artifact: &DecisionArtifact) -> Result<(), String> {
        use crate::core::providers::EmbeddingProvider;
        artifact.validate()?;
        let Some(metadata) = self.provider.model_metadata() else {
            return Err("backbone reports no model metadata".to_string());
        };
        if artifact.backbone.hidden_size != metadata.dimensions {
            return Err(format!(
                "artifact expects hidden size {}, backbone reports {}",
                artifact.backbone.hidden_size, metadata.dimensions
            ));
        }
        Ok(())
    }
}

/// Validate that a request is batchable by the cross-encoder path under
/// `provider` (shared pre-check for the Phase 3 provider).
pub fn validate_cross_encoder_request(
    provider: &str,
    request: &DecisionRequest,
) -> Result<(), crate::core::error::ProviderError> {
    validate_request(provider, request)
}
