use std::sync::OnceLock;

use crate::core::capabilities::{CapabilityLevel, ProviderCapabilities};
use crate::core::error::ProviderError;
use crate::core::providers::{
    AbbreviationProvider, LanguageDetectionProvider, LexiconProvider, SymbolKnowledgeProvider,
};
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

    fn frequency(&self, word: &str, languages: Option<&[String]>) -> Option<f64> {
        self.language_index.frequency(word, languages)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("resource_lexicon")
            .with_languages(self.languages())
            .with_quality(CapabilityLevel::Basic)
    }
}

impl LanguageDetectionProvider for ResourceLoader {
    fn detect(&self, text: &str) -> Result<Vec<LanguageCandidate>, ProviderError> {
        Ok(self.detect_languages(text))
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("resource_profile_detector")
            .with_languages(self.languages())
            .with_quality(CapabilityLevel::Basic)
    }
}

fn sort_readings(readings: &mut [SymbolReading]) {
    readings.sort_by(|left, right| {
        right
            .probability
            .total_cmp(&left.probability)
            .then_with(|| left.text.cmp(&right.text))
            .then_with(|| left.language.cmp(&right.language))
    });
}

/// Same semantics as the rebus decoding filter: empty constraints and
/// `und`/`unknown` readings always pass so neutral packs stay visible.
fn symbol_language_allowed(language: Option<&str>, languages: Option<&[String]>) -> bool {
    let Some(languages) = languages.filter(|values| !values.is_empty()) else {
        return true;
    };
    let Some(language) = language else {
        return true;
    };
    language == "und"
        || languages.iter().any(|candidate| {
            candidate.eq_ignore_ascii_case(language)
                || candidate.eq_ignore_ascii_case("unknown")
                || candidate.eq_ignore_ascii_case("und")
        })
}

impl SymbolKnowledgeProvider for ResourceLoader {
    fn readings(&self, token: &str, max_readings: usize) -> Vec<SymbolReading> {
        let mut readings = self
            .symbol_index
            .get(token)
            .map(|symbol| symbol.readings.clone())
            .unwrap_or_default();
        sort_readings(&mut readings);
        readings.truncate(max_readings);
        readings
    }

    fn readings_in_languages(
        &self,
        token: &str,
        max_readings: usize,
        languages: Option<&[String]>,
    ) -> Vec<SymbolReading> {
        let mut readings = self
            .symbol_index
            .get(token)
            .map(|symbol| symbol.readings.clone())
            .unwrap_or_default()
            .into_iter()
            .filter(|reading| symbol_language_allowed(reading.language.as_deref(), languages))
            .collect::<Vec<_>>();
        sort_readings(&mut readings);
        readings.truncate(max_readings);
        readings
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

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("resource_symbols")
            .with_languages(self.symbol_pack_languages())
            .with_quality(CapabilityLevel::Basic)
    }
}

impl AbbreviationProvider for ResourceLoader {
    fn abbreviation_readings(
        &self,
        token: &str,
        languages: Option<&[String]>,
        max_readings: usize,
    ) -> Vec<SymbolReading> {
        self.abbreviation_readings(token, languages, max_readings)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("resource_abbreviations")
            .with_languages(self.abbreviation_languages())
            .with_quality(CapabilityLevel::Basic)
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

    fn frequency(&self, word: &str, languages: Option<&[String]>) -> Option<f64> {
        embedded().frequency(word, languages)
    }
}
