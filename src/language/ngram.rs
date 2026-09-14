use std::collections::BTreeMap;

use crate::core::capabilities::ProviderCapabilities;
use crate::core::error::ProviderError;
use crate::core::providers::LanguageDetectionProvider;
use crate::core::types::LanguageCandidate;
use crate::normalization::unicode::casefold_text;
use crate::resources::ResourceLoader;

/// Lightweight local character n-gram detector trained from resource-pack
/// samples. It is deliberately model-compatible in shape (top-k probabilities
/// and unknown) while remaining zero-network and deterministic.
#[derive(Debug, Clone)]
pub struct NgramLanguageDetector {
    profiles: BTreeMap<String, BTreeMap<String, f64>>,
    max_candidates: usize,
}

impl NgramLanguageDetector {
    pub fn from_resources(resources: &ResourceLoader) -> Self {
        let profiles = resources
            .profile_texts()
            .into_iter()
            .map(|(language, texts)| (language, build_profile(&texts)))
            .filter(|(_, profile)| !profile.is_empty())
            .collect();
        Self {
            profiles,
            max_candidates: 8,
        }
    }

    pub fn with_max_candidates(mut self, max_candidates: usize) -> Self {
        self.max_candidates = max_candidates.max(1);
        self
    }

    pub fn languages(&self) -> Vec<String> {
        self.profiles.keys().cloned().collect()
    }

    fn detect_inner(&self, text: &str) -> Vec<LanguageCandidate> {
        let query = build_profile(&[text.to_string()]);
        if query.is_empty() || self.profiles.is_empty() {
            return vec![LanguageCandidate::new("unknown", 1.0)];
        }
        let mut scores = self
            .profiles
            .iter()
            .map(|(language, profile)| (language.clone(), cosine_profile(&query, profile)))
            .filter(|(_, score)| *score > 0.0)
            .collect::<Vec<_>>();
        if scores.is_empty() {
            return vec![LanguageCandidate::new("unknown", 1.0)];
        }
        scores.sort_by(|left, right| {
            right
                .1
                .total_cmp(&left.1)
                .then_with(|| left.0.cmp(&right.0))
        });
        let max_score = scores.first().map(|(_, score)| *score).unwrap_or(0.0);
        let temperature = 0.08;
        let mut candidates = scores
            .into_iter()
            .map(|(language, score)| {
                LanguageCandidate::new(language, ((score - max_score) / temperature).exp())
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| right.probability.total_cmp(&left.probability));
        candidates.truncate(self.max_candidates.max(1));
        let confidence = max_score.clamp(0.0, 1.0);
        if confidence < 0.35 {
            candidates.push(LanguageCandidate::new("unknown", 1.0 - confidence));
        }
        normalize(candidates)
    }
}

impl LanguageDetectionProvider for NgramLanguageDetector {
    fn detect(&self, text: &str) -> Result<Vec<LanguageCandidate>, ProviderError> {
        Ok(self.detect_inner(text))
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("local_char_ngram_language_v1")
            .with_version("resource-profile-1")
            .with_languages(self.languages())
    }
}

fn build_profile(texts: &[String]) -> BTreeMap<String, f64> {
    let mut counts = BTreeMap::<String, f64>::new();
    for text in texts {
        let folded = casefold_text(text);
        for n in 2..=4 {
            let chars = folded.chars().collect::<Vec<_>>();
            for window in chars.windows(n) {
                if window.iter().all(|character| character.is_whitespace()) {
                    continue;
                }
                *counts.entry(window.iter().collect()).or_default() += 1.0;
            }
        }
    }
    let norm = counts
        .values()
        .map(|value| value * value)
        .sum::<f64>()
        .sqrt()
        .max(f64::EPSILON);
    counts.values_mut().for_each(|value| *value /= norm);
    counts
}

fn cosine_profile(left: &BTreeMap<String, f64>, right: &BTreeMap<String, f64>) -> f64 {
    left.iter()
        .filter_map(|(key, value)| right.get(key).map(|other| value * other))
        .sum::<f64>()
        .clamp(0.0, 1.0)
}

fn normalize(mut values: Vec<LanguageCandidate>) -> Vec<LanguageCandidate> {
    let total = values
        .iter()
        .map(|candidate| candidate.probability.max(0.0))
        .sum::<f64>()
        .max(f64::EPSILON);
    for candidate in &mut values {
        candidate.probability = candidate.probability.max(0.0) / total;
    }
    values
}
