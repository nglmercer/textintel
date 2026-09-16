use std::collections::{BTreeMap, BTreeSet};

use crate::core::capabilities::{CapabilityLevel, ProviderCapabilities};
use crate::core::error::ProviderError;
use crate::core::providers::LanguageDetectionProvider;
use crate::core::types::LanguageCandidate;
use crate::normalization::unicode::casefold_text;
use crate::resources::ResourceLoader;
use crate::visual::scripts::scripts_in;

/// Combined-evidence floor for a language prediction: weaker best
/// evidence ranks `unknown` first instead of guessing.
const ABSTAIN_THRESHOLD: f64 = 0.4;

/// Local language detector fusing three evidence sources over the same
/// resource profiles: character n-gram cosine (robust to typos and long
/// texts), exact word matches weighted by inverse document frequency
/// (decisive for short phrases, stop-words down-weighted), and a Unicode
/// script gate (a query sharing no script with a profile is nearly
/// impossible for that language).
///
/// Ambiguous evidence is discounted, not forced: a word found in K language
/// profiles contributes 1/K of a unique word, and genuinely uncertain
/// inputs (weak, contested evidence) rank `unknown` first instead of
/// guessing. Zero-network and deterministic.
#[derive(Debug, Clone)]
pub struct NgramLanguageDetector {
    profiles: BTreeMap<String, BTreeMap<String, f64>>,
    word_sets: BTreeMap<String, BTreeSet<String>>,
    /// Inverse document frequency per word, normalized so a word unique to
    /// one language weighs 1.0 and a word in every language weighs 0.0.
    idf: BTreeMap<String, f64>,
    profile_scripts: BTreeMap<String, BTreeSet<String>>,
    max_candidates: usize,
}

impl NgramLanguageDetector {
    pub fn from_resources(resources: &ResourceLoader) -> Self {
        let profile_texts = resources.profile_texts();
        let profiles: BTreeMap<String, BTreeMap<String, f64>> = profile_texts
            .iter()
            .map(|(language, texts)| (language.clone(), build_profile(texts)))
            .filter(|(_, profile)| !profile.is_empty())
            .collect();
        let mut word_sets: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut profile_scripts: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (language, texts) in &profile_texts {
            let words = word_sets.entry(language.clone()).or_default();
            let scripts = profile_scripts.entry(language.clone()).or_default();
            for text in texts {
                words.extend(query_words(text));
                scripts.extend(scripts_in(text));
            }
        }
        // Drop languages with neither n-grams nor words: they can never win
        // and would only flatten the IDF weights.
        word_sets.retain(|language, words| {
            !words.is_empty()
                || profiles
                    .get(language)
                    .is_some_and(|profile: &BTreeMap<String, f64>| !profile.is_empty())
        });
        let languages = word_sets.len().max(1) as f64;
        let mut document_frequency: BTreeMap<String, usize> = BTreeMap::new();
        for words in word_sets.values() {
            for word in words {
                *document_frequency.entry(word.clone()).or_default() += 1;
            }
        }
        let normalizer = languages.ln().max(f64::EPSILON);
        let idf = document_frequency
            .into_iter()
            .map(|(word, frequency)| {
                (
                    word,
                    (languages / frequency.max(1) as f64).ln() / normalizer,
                )
            })
            .collect();
        Self {
            profiles,
            word_sets,
            idf,
            profile_scripts,
            max_candidates: 8,
        }
    }

    pub fn with_max_candidates(mut self, max_candidates: usize) -> Self {
        self.max_candidates = max_candidates.max(1);
        self
    }

    pub fn languages(&self) -> Vec<String> {
        self.word_sets.keys().cloned().collect()
    }

    fn detect_inner(&self, text: &str) -> Vec<LanguageCandidate> {
        let query = build_profile(&[text.to_string()]);
        let words = query_words(text);
        let query_scripts: BTreeSet<String> = scripts_in(text).into_iter().collect();
        if (query.is_empty() || self.profiles.is_empty()) && words.is_empty() {
            return vec![LanguageCandidate::new("unknown", 1.0)];
        }
        let total_weight: f64 = words
            .iter()
            .map(|word| self.idf.get(word).copied().unwrap_or(0.0))
            .sum();
        let cjk_query = cjk_chars(text);
        let mut scores = Vec::new();
        for (language, profile) in &self.profiles {
            let cosine = if query.is_empty() {
                0.0
            } else {
                cosine_profile(&query, profile)
            };
            // Exact word evidence: each matched word contributes its IDF
            // discounted by its own promiscuity (a word in K profiles is
            // worth 1/K), over the recognizable mass of the query. Unknown
            // words carry no signal either way.
            let mut word_score = 0.0;
            if let Some(known) = self.word_sets.get(language) {
                if total_weight > 0.0 {
                    let mut matched_weight = 0.0;
                    for word in &words {
                        if known.contains(word) {
                            let idf = self.idf.get(word).copied().unwrap_or(0.0);
                            let spread = self
                                .word_sets
                                .values()
                                .filter(|set| set.contains(word))
                                .count()
                                .max(1) as f64;
                            matched_weight += idf / spread;
                        }
                    }
                    word_score = (matched_weight / total_weight).clamp(0.0, 1.0);
                }
                // Spaceless-script queries (Han, kana, Hangul) match pack
                // words by substring: segmentation-free coverage of the
                // query's CJK characters.
                if !cjk_query.is_empty() {
                    word_score = word_score.max(cjk_coverage(&cjk_query, known));
                }
            }
            let gate = match (self.profile_scripts.get(language), query_scripts.is_empty()) {
                (Some(supported), false)
                    if !supported.is_empty()
                        && !supported
                            .iter()
                            .any(|script| query_scripts.contains(script)) =>
                {
                    0.05
                }
                _ => 1.0,
            };
            // Either channel suffices: exact words decide short phrases,
            // n-grams carry long and typo'd texts. Corroboration is a
            // bonus the temperature sharpening below does not need.
            let combined = cosine.max(word_score) * gate;
            if combined > 0.0 {
                scores.push((language.clone(), combined));
            }
        }
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
        // Weak, contested evidence abstains: `unknown` carries the missing
        // mass in odds form against the temperature-softmaxed top candidate
        // (which always scores exactly 1.0), so `unknown` ranks first
        // exactly when the best combined evidence is below
        // `ABSTAIN_THRESHOLD` (tuned on the validation split; the sweep
        // is recorded in PRODUCTION_GAP_ANALYSIS.md).
        let unknown_mass = ((1.0 - max_score) / max_score.max(f64::EPSILON)
            * (ABSTAIN_THRESHOLD / (1.0 - ABSTAIN_THRESHOLD)))
            .clamp(0.0, 1.0e6);
        let temperature = 0.08;
        let mut candidates = scores
            .into_iter()
            .map(|(language, score)| {
                LanguageCandidate::new(language, ((score - max_score) / temperature).exp())
            })
            .collect::<Vec<_>>();
        candidates.push(LanguageCandidate::new("unknown", unknown_mass));
        candidates.sort_by(|left, right| right.probability.total_cmp(&left.probability));
        candidates.truncate(self.max_candidates.max(1));
        normalize(candidates)
    }
}

impl LanguageDetectionProvider for NgramLanguageDetector {
    fn detect(&self, text: &str) -> Result<Vec<LanguageCandidate>, ProviderError> {
        Ok(self.detect_inner(text))
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("local_char_ngram_language_v1")
            .with_version("resource-profile-2")
            .with_languages(self.languages())
            .with_quality(CapabilityLevel::Basic)
    }
}

/// Casefolded alphabetic words of a text. Splitting mirrors lexicon word
/// extraction so profile words and query words compare apples to apples.
fn query_words(text: &str) -> Vec<String> {
    casefold_text(text)
        .split(|character: char| !character.is_alphabetic())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

/// Characters of the spaceless CJK scripts (Han, Hiragana, Katakana,
/// Hangul) in reading order. Empty for alphabetically-spaced text.
fn cjk_chars(text: &str) -> Vec<char> {
    text.chars()
        .filter(|character| {
            matches!(
                character,
                '\u{3400}'..='\u{4dbf}'
                    | '\u{4e00}'..='\u{9fff}'
                    | '\u{3040}'..='\u{309f}'
                    | '\u{30a0}'..='\u{30ff}'
                    | '\u{ac00}'..='\u{d7af}'
            )
        })
        .collect()
}

/// Fraction of `query` CJK characters covered by `words` entries occurring
/// as substrings (longest match wins per position; single characters are
/// legitimate words in these scripts).
fn cjk_coverage(query: &[char], words: &BTreeSet<String>) -> f64 {
    if query.is_empty() {
        return 0.0;
    }
    let text: String = query.iter().collect();
    let mut covered = vec![false; query.len()];
    for word in words {
        if !word.chars().all(|character| {
            matches!(
                character,
                '\u{3400}'..='\u{4dbf}'
                    | '\u{4e00}'..='\u{9fff}'
                    | '\u{3040}'..='\u{309f}'
                    | '\u{30a0}'..='\u{30ff}'
                    | '\u{ac00}'..='\u{d7af}'
            )
        }) {
            continue;
        }
        for index in text.char_indices().map(|(index, _)| index) {
            if text[index..].starts_with(word) {
                let position = text[..index].chars().count();
                let length = word.chars().count();
                for slot in covered.iter_mut().skip(position).take(length) {
                    *slot = true;
                }
            }
        }
    }
    covered.iter().filter(|slot| **slot).count() as f64 / query.len() as f64
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
