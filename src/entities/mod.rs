//! Bounded rule-based entity evidence (`Basic` fallback).
//!
//! [`RuleBasedEntityProvider`] extracts URLs, emails, mentions, numbers,
//! currency, reliable dates/times, and name-like spans with deterministic
//! character scanners. No models, no network, no lexicon lookups: every
//! mention carries its UTF-8 byte span, a normalized value, a confidence,
//! the provider name, and the ambient language when known.
//!
//! Entity evidence is exposed independently on
//! [`MessageFingerprint`](crate::core::types::MessageFingerprint) and in
//! [`ComparisonResult`](crate::core::types::ComparisonResult) as
//! `entity_agreement` / `entity_conflict`. Missing evidence reads as `0.0`
//! on both, so entity-free pairs never pay a penalty.

mod currency;
mod datetime;
mod email;
mod mention;
mod names;
mod normalize;
mod numeric;
mod provider;
mod url;

pub use provider::{
    RuleBasedEntityProvider, entity_agreement, entity_conflict, entity_evidence_lines,
};

/// Default cap on mentions per text.
pub const DEFAULT_MAX_ENTITIES: usize = 16;
/// Default cap on mention span length in characters.
pub const DEFAULT_MAX_ENTITY_SPAN: usize = 64;
