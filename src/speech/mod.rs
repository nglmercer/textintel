//! Lossless parsing and explicit text normalization for speech applications.
//!
//! This path never runs case folding, leetspeak/rebus decoding, repetition
//! collapsing or model inference. Source offsets always refer to original text.
pub mod parser;
pub mod types;
pub use parser::parse_for_speech;
pub use types::*;
pub mod evaluation;
mod lexicons;
pub mod numbers;
pub mod verbalize;
pub use numbers::{cardinal_max, verbalize_number};
pub use verbalize::{normalize_for_speech, verbalize_document};

/// Versioned, offline speech normalization.
pub mod normalization;
/// Shared bounded segmentation after normalization.
pub mod segmentation;
