//! Fast local probabilistic typed decisions (TextIntel S1).
//!
//! This module implements an independent System-One-style classification
//! layer: bounded outputs, probability distributions, and calibrated
//! confidence — never open-ended text generation. It is our own
//! architecture, designed from publicly observable behavior and
//! TextIntel's existing strengths; it makes no claim about any
//! proprietary system's internals.
//!
//! ```text
//! DecisionRequest (state + one question)
//!   → engine prepares fingerprints
//!   → DecisionProvider scores candidates
//!   → calibration
//!   → DecisionResponse (distribution + Accept/Escalate)
//! ```
//!
//! One module per responsibility: [`types`] (requests, answers,
//! responses), [`provider`] (the trait), [`adapters`] (existing-model
//! baselines), [`scoring`] (softmax and distribution math),
//! [`calibration`] (temperature/bias, NLL/Brier/ECE), [`artifact`]
//! (versioned model envelopes), [`fusion`] (versioned fingerprint
//! features), [`interaction`] (v2 shared-state scorer + head trainer),
//! [`transformer`] (v1 cross-encoder prompts and backbone validation),
//! [`dataset`] (JSONL datasets), and [`eval`] (metrics, risk/coverage,
//! gates).

pub mod adapters;
pub mod artifact;
pub mod calibration;
pub mod dataset;
pub mod eval;
pub mod fusion;
pub mod interaction;
pub mod provider;
pub mod scoring;
pub mod transformer;
pub mod types;

pub use adapters::{ADAPTER_ACCEPT_THRESHOLD, SimilarityDecisionProvider, SpamDecisionProvider};
pub use artifact::{
    ARCHITECTURE_CANDIDATE_CROSS_ENCODER, BACKBONE_MODEL_TYPE_BERT, DECISION_ARTIFACT_KIND,
    DECISION_ARTIFACT_VERSION, DecisionArtifact, DecisionBackbone, DecisionCalibration,
    DecisionDatasetRef, DecisionHead,
};
pub use calibration::{
    CalibrationSample, TaskCalibration, TemperatureBias, TemperatureFit, TemperatureScaling,
    brier_score, expected_calibration_error, fit_temperature, nll_loss,
};
pub use dataset::{DecisionDataset, DecisionExample, DecisionSplit};
pub use eval::{
    COVERAGE_LEVELS, CoveragePoint, DecisionEvalReport, check_decision_gates, evaluate_decisions,
};
pub use fusion::{
    FUSION_FEATURE_SCHEMA_VERSION, FUSION_FEATURES, fusion_feature_vector, fusion_features,
};
pub use interaction::{
    ARCHITECTURE_STATE_CANDIDATE_INTERACTION, HeadTrainExample, HeadTrainer,
    INTERACTION_ARTIFACT_KIND, INTERACTION_ARTIFACT_VERSION, INTERACTION_EMBEDDING_CACHE,
    INTERACTION_TEMPERATURE, InteractionArtifact, InteractionDecisionProvider, InteractionHead,
    SplitMix64, gelu, gelu_prime, head_loss_accuracy, init_head_xavier, interaction_features,
};
pub use provider::{DecisionProvider, SharedDecisionProvider, validate_request};
pub use scoring::{
    energy_score, entropy, expected_score, is_ood_by_energy, margin, max_probability, softmax,
    softmax_with_temperature, validate_distribution,
};
#[cfg(feature = "decision-transformer")]
pub use transformer::TransformerBackbone;
pub use transformer::{
    CandidatePrompt, CrossEncoderHeadConfig, candidate_prompt, plan_candidate_batch,
    validate_cross_encoder_request,
};
pub use types::{
    DECISION_SCHEMA_VERSION, Decision, DecisionAnswer, DecisionModelInfo, DecisionQuestion,
    DecisionRequest, DecisionResponse, MAX_DECISION_BATCH, MAX_DECISION_CANDIDATES,
    MAX_DECISION_STATE_CHARS, NONE_OF_THE_ABOVE, PROBABILITY_SUM_TOLERANCE, selective_decision,
};
