//! Provider boundary for typed decisions. Implementations score one
//! bounded question over shared state and return a probability
//! distribution — classification, never prose generation.

use std::sync::Arc;

use crate::core::capabilities::ProviderCapabilities;
use crate::core::error::ProviderError;

use super::types::{DecisionModelInfo, DecisionRequest, DecisionResponse, MAX_DECISION_BATCH};

/// Bounded probabilistic classification over shared state.
///
/// Implementations must be deterministic for a fixed request and must
/// return full distributions (see [`DecisionResponse::validate_against`]).
/// Providers never analyze raw text themselves: the engine attaches
/// fingerprints to the request before dispatch, and adapters error when
/// required evidence is missing.
pub trait DecisionProvider: Send + Sync {
    fn decide(&self, request: &DecisionRequest) -> Result<DecisionResponse, ProviderError>;

    fn decide_batch(
        &self,
        requests: &[DecisionRequest],
    ) -> Result<Vec<DecisionResponse>, ProviderError> {
        if requests.len() > MAX_DECISION_BATCH {
            return Err(ProviderError::new(
                self.capabilities().provider,
                format!(
                    "batch of {} exceeds maximum {MAX_DECISION_BATCH}",
                    requests.len()
                ),
            ));
        }
        requests
            .iter()
            .map(|request| self.decide(request))
            .collect()
    }

    fn capabilities(&self) -> ProviderCapabilities;

    /// Static model facts for diagnostics and `decision-model-info`.
    /// Defaults to an unidentified architecture under this provider's name.
    fn model_info(&self) -> DecisionModelInfo {
        DecisionModelInfo::new(self.capabilities().provider, "unknown")
    }

    fn health_check(&self) -> Result<(), ProviderError> {
        Ok(())
    }
}

/// Shared ownership preserves provider behavior: every method (including
/// batch, capability, and model-info introspection) forwards to the inner
/// provider.
impl<T: DecisionProvider + ?Sized> DecisionProvider for Arc<T> {
    fn decide(&self, request: &DecisionRequest) -> Result<DecisionResponse, ProviderError> {
        (**self).decide(request)
    }

    fn decide_batch(
        &self,
        requests: &[DecisionRequest],
    ) -> Result<Vec<DecisionResponse>, ProviderError> {
        (**self).decide_batch(requests)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        (**self).capabilities()
    }

    fn model_info(&self) -> DecisionModelInfo {
        (**self).model_info()
    }

    fn health_check(&self) -> Result<(), ProviderError> {
        (**self).health_check()
    }
}

pub type SharedDecisionProvider = Arc<dyn DecisionProvider>;

/// Validate a request's serializable fields, attributing failures to
/// `provider`. Evidence attachment is checked separately by each provider.
pub fn validate_request(provider: &str, request: &DecisionRequest) -> Result<(), ProviderError> {
    request
        .validate()
        .map_err(|message| ProviderError::new(provider, message))
}
