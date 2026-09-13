use std::collections::BTreeMap;

use crate::core::error::ProviderError;
use crate::core::providers::LanguageDetectionProvider;
use crate::core::types::LanguageCandidate;

const ES: &[&str] = &[
    "el",
    "la",
    "los",
    "las",
    "de",
    "que",
    "y",
    "en",
    "un",
    "una",
    "es",
    "por",
    "con",
    "para",
    "compra",
    "comprar",
    "ahora",
    "gana",
    "ganar",
    "dinero",
    "saludos",
    "saludo",
    "casa",
    "hogar",
    "fracasado",
    "hola",
    "gracias",
    "si",
    "sí",
    "siempre",
    "premio",
    "reclama",
    "tu",
    "dos",
    "fuego",
    "amor",
    "plata",
    "vivienda",
];
const EN: &[&str] = &[
    "the", "and", "of", "to", "a", "in", "is", "you", "that", "it", "for", "on", "with", "bro",
    "now", "buy", "money", "house", "home", "hi", "hello", "fire", "hot", "lit", "always", "love",
    "cash", "please", "claim", "your", "prize",
];
const PT: &[&str] = &[
    "o", "a", "os", "as", "de", "que", "e", "do", "da", "para", "com", "não", "dinheiro", "casa",
    "agora", "comprar",
];
const FR: &[&str] = &[
    "le",
    "la",
    "les",
    "de",
    "des",
    "et",
    "un",
    "une",
    "est",
    "pour",
    "avec",
    "bonjour",
    "maison",
    "argent",
    "acheter",
    "maintenant",
];

fn word_set(words: &[&str], word: &str) -> bool {
    words.contains(&word)
}

pub fn detect_languages(text: &str) -> Vec<LanguageCandidate> {
    let words = text
        .split(|ch: char| !ch.is_alphabetic())
        .filter(|word| !word.is_empty())
        .map(|word| {
            word.chars()
                .flat_map(char::to_lowercase)
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    let mut scores = BTreeMap::<String, f64>::new();
    for word in &words {
        if word_set(ES, word) {
            *scores.entry("es".to_string()).or_default() += 2.0;
        }
        if word_set(EN, word) {
            *scores.entry("en".to_string()).or_default() += 2.0;
        }
        if word_set(PT, word) {
            *scores.entry("pt".to_string()).or_default() += 1.0;
        }
        if word_set(FR, word) {
            *scores.entry("fr".to_string()).or_default() += 1.5;
        }
    }
    for ch in text.chars().flat_map(char::to_lowercase) {
        if "áéíóúñü¿¡".contains(ch) {
            *scores.entry("es".to_string()).or_default() += 1.0;
        }
        if "ãõç".contains(ch) {
            *scores.entry("pt".to_string()).or_default() += 1.0;
        }
        if "àâæçéèêëîïôœùûüÿ".contains(ch) {
            *scores.entry("fr".to_string()).or_default() += 0.6;
        }
    }
    if scores.is_empty() {
        if words.is_empty() {
            return vec![LanguageCandidate::new("unknown", 1.0)];
        }
        scores.insert("unknown".to_string(), 1.0);
        scores.insert("es".to_string(), 0.3);
        scores.insert("en".to_string(), 0.3);
    }
    let total: f64 = scores.values().sum();
    let mut candidates: Vec<_> = scores
        .into_iter()
        .map(|(language, score)| LanguageCandidate::new(language, score / total.max(f64::EPSILON)))
        .collect();
    candidates.sort_by(|left, right| right.probability.total_cmp(&left.probability));
    let known_mass: f64 = candidates
        .iter()
        .map(|candidate| candidate.probability)
        .sum();
    if candidates.len() == 1 && candidates[0].language != "unknown" && known_mass < 0.95 {
        candidates.push(LanguageCandidate::new("unknown", 1.0 - known_mass));
    }
    // Always keep probabilities normalized after adding uncertainty.
    let sum: f64 = candidates
        .iter()
        .map(|candidate| candidate.probability)
        .sum();
    for candidate in &mut candidates {
        candidate.probability /= sum.max(f64::EPSILON);
    }
    candidates
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultLanguageDetector;

impl LanguageDetectionProvider for DefaultLanguageDetector {
    fn detect(&self, text: &str) -> Result<Vec<LanguageCandidate>, ProviderError> {
        Ok(detect_languages(text))
    }
}
