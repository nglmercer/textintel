use std::sync::OnceLock;

use crate::core::error::ProviderError;
use crate::core::providers::{LanguageDetectionProvider, LexiconProvider, SymbolKnowledgeProvider};
use crate::core::types::{LanguageCandidate, SymbolConcept, SymbolReading};

use super::loader::ResourceLoader;

impl LexiconProvider for ResourceLoader {
    fn contains(&self, word: &str, languages: Option<&[String]>) -> bool {
        self.language_index.contains(word, languages)
    }

    fn starts_with(&self, prefix: &str, languages: Option<&[String]>) -> bool {
        self.language_index.starts_with(prefix, languages)
    }

    fn is_stop_word(&self, word: &str, languages: Option<&[String]>) -> bool {
        self.language_index.is_stop_word(word, languages)
    }

    fn lemma(&self, word: &str, languages: Option<&[String]>) -> Option<String> {
        self.language_index.lemma(word, languages)
    }
}

impl LanguageDetectionProvider for ResourceLoader {
    fn detect(&self, text: &str) -> Result<Vec<LanguageCandidate>, ProviderError> {
        Ok(self.detect_languages(text))
    }
}

impl SymbolKnowledgeProvider for ResourceLoader {
    fn readings(&self, token: &str, max_readings: usize) -> Vec<SymbolReading> {
        self.symbol_index
            .get(token)
            .map(|symbol| symbol.readings.iter().take(max_readings).cloned().collect())
            .unwrap_or_default()
    }

    fn concepts(&self, token: &str) -> Vec<SymbolConcept> {
        self.symbol_index
            .get(token)
            .map(|symbol| symbol.concepts.clone())
            .unwrap_or_default()
    }

    fn unicode_name(&self, token: &str) -> Option<String> {
        self.symbol_index
            .get(token)
            .and_then(|symbol| symbol.unicode_name.clone())
    }
}

/// Shared embedded resources used by compatibility defaults.
pub fn embedded() -> &'static ResourceLoader {
    static EMBEDDED: OnceLock<ResourceLoader> = OnceLock::new();
    EMBEDDED.get_or_init(|| ResourceLoader::embedded().expect("embedded resources must be valid"))
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultLexiconProvider;

impl LexiconProvider for DefaultLexiconProvider {
    fn contains(&self, word: &str, languages: Option<&[String]>) -> bool {
        embedded().contains(word, languages)
    }

    fn starts_with(&self, prefix: &str, languages: Option<&[String]>) -> bool {
        embedded().starts_with(prefix, languages)
    }

    fn is_stop_word(&self, word: &str, languages: Option<&[String]>) -> bool {
        embedded().is_stop_word(word, languages)
    }

    fn lemma(&self, word: &str, languages: Option<&[String]>) -> Option<String> {
        embedded().lemma(word, languages)
    }
}
