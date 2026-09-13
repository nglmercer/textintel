use crate::core::providers::SymbolKnowledgeProvider;
use crate::core::types::{SymbolConcept, SymbolReading};
use crate::resources::embedded_resources;

/// Compatibility wrapper around the embedded symbol resources.
/// Applications with larger or domain-specific packs should inject their own
/// `ResourceLoader` through `TextIntelligence::with_resources`.
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultSymbolKnowledge;

pub fn readings_for_token(token: &str, max_readings: usize) -> Vec<SymbolReading> {
    SymbolKnowledgeProvider::readings(embedded_resources(), token, max_readings)
}

pub fn concepts_for_token(token: &str) -> Vec<SymbolConcept> {
    SymbolKnowledgeProvider::concepts(embedded_resources(), token)
}

pub fn unicode_name_for_token(token: &str) -> Option<String> {
    SymbolKnowledgeProvider::unicode_name(embedded_resources(), token)
}

impl SymbolKnowledgeProvider for DefaultSymbolKnowledge {
    fn readings(&self, token: &str, max_readings: usize) -> Vec<SymbolReading> {
        readings_for_token(token, max_readings)
    }

    fn concepts(&self, token: &str) -> Vec<SymbolConcept> {
        concepts_for_token(token)
    }

    fn unicode_name(&self, token: &str) -> Option<String> {
        unicode_name_for_token(token)
    }
}
