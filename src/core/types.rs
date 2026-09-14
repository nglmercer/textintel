use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const FINGERPRINT_SCHEMA_VERSION: u32 = 2;
pub const API_VERSION: &str = "0.2.0";

fn default_fingerprint_schema_version() -> u32 {
    FINGERPRINT_SCHEMA_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LanguageCandidate {
    pub language: String,
    pub probability: f64,
}

impl LanguageCandidate {
    pub fn new(language: impl Into<String>, probability: f64) -> Self {
        Self {
            language: language.into(),
            probability,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageSegment {
    pub text: String,
    /// UTF-8 byte offset into the original message.
    pub start: usize,
    /// UTF-8 byte offset just after the segment.
    pub end: usize,
    pub language_candidates: Vec<LanguageCandidate>,
    pub segment_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfusableCharacter {
    pub character: String,
    /// Unicode scalar index in the original message.
    pub index: usize,
    pub script: String,
    pub confusable_with: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UnicodeFeatures {
    pub scripts: Vec<String>,
    #[serde(default)]
    pub script_extensions: Vec<String>,
    pub mixed_scripts: bool,
    pub invisible_characters: Vec<String>,
    pub unusual_whitespace: Vec<String>,
    pub combining_characters: Vec<String>,
    pub bidirectional_controls: Vec<String>,
    #[serde(default)]
    pub variation_selectors: Vec<String>,
    #[serde(default)]
    pub joiner_characters: Vec<String>,
    #[serde(default)]
    pub full_width_characters: Vec<String>,
    #[serde(default)]
    pub malformed_graphemes: Vec<String>,
    pub confusable_characters: Vec<ConfusableCharacter>,
    pub confusable_skeleton: Option<String>,
    pub suspicious_unicode_score: f64,
    pub nfc: String,
    pub nfkc: String,
    pub casefolded: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CharacterFeatures {
    pub length: usize,
    pub letters: usize,
    pub digits: usize,
    pub whitespace: usize,
    pub punctuation: usize,
    pub other: usize,
    pub ngrams_2: BTreeMap<String, usize>,
    pub ngrams_3: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CharacterSimilarity {
    pub levenshtein: f64,
    pub damerau_levenshtein: f64,
    pub jaro: f64,
    pub jaro_winkler: f64,
    pub ngram_similarity: f64,
    pub lcs: f64,
    pub combined: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LexicalFeatures {
    pub tokens: Vec<String>,
    pub lemmas: Vec<String>,
    pub stop_words: Vec<String>,
    pub word_ngrams: Vec<String>,
    pub token_ngrams: Vec<String>,
    pub jaccard_ready: BTreeSet<String>,
    pub simhash: u64,
    pub minhash: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SymbolReading {
    pub text: String,
    pub language: Option<String>,
    pub probability: f64,
    pub reading_type: String,
}

impl SymbolReading {
    pub fn new(
        text: impl Into<String>,
        language: Option<impl Into<String>>,
        probability: f64,
        reading_type: impl Into<String>,
    ) -> Self {
        Self {
            text: text.into(),
            language: language.map(|value| value.into()),
            probability,
            reading_type: reading_type.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SymbolConcept {
    pub id: String,
    pub probability: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SymbolInstance {
    pub raw: String,
    pub start: usize,
    pub end: usize,
    pub kind: String,
    pub unicode_name: Option<String>,
    pub concepts: Vec<SymbolConcept>,
    pub readings: Vec<SymbolReading>,
}

/// A possible spoken or decoded reading of a message or message fragment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpokenCandidate {
    pub text: String,
    pub language: Option<String>,
    pub probability: f64,
    pub source_transformations: Vec<Transformation>,
    /// Compatibility-friendly alias for callers that think in confidence.
    pub confidence: f64,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PhoneticCandidate {
    pub source: String,
    pub language: String,
    #[serde(default)]
    pub dialect: Option<String>,
    pub ipa: Option<String>,
    pub phonemes: Vec<String>,
    #[serde(default)]
    pub stress: Option<Vec<usize>>,
    #[serde(default)]
    pub syllables: usize,
    #[serde(default)]
    pub articulatory_features: Vec<String>,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Transformation {
    pub source: String,
    pub replacement: String,
    pub transformation_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelAvailability {
    pub available: bool,
    pub confidence: f64,
    pub source: String,
}

impl ChannelAvailability {
    pub fn available(source: impl Into<String>, confidence: f64) -> Self {
        Self {
            available: true,
            confidence: confidence.clamp(0.0, 1.0),
            source: source.into(),
        }
    }

    pub fn unavailable(source: impl Into<String>) -> Self {
        Self {
            available: false,
            confidence: 0.0,
            source: source.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecodedCandidate {
    pub text: String,
    pub score: f64,
    pub transformations: Vec<Transformation>,
    pub language: Option<String>,
    pub lexical_score: f64,
    pub phonetic_score: f64,
    pub context_score: f64,
    pub symbol_score: f64,
    #[serde(default)]
    pub confidence_gap: f64,
    #[serde(default)]
    pub strong: bool,
}

impl DecodedCandidate {
    /// Rank-aware confidence: the raw score discounted by rank separation.
    /// A lonely top candidate keeps its score; a contested one is downrated.
    pub fn confidence(&self) -> f64 {
        let score = self.score.clamp(0.0, 1.0);
        score * (0.5 + 0.5 * self.confidence_gap.clamp(0.0, 1.0))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObfuscationFeatures {
    pub detected: bool,
    pub score: f64,
    pub leet_score: f64,
    pub homoglyph_score: f64,
    pub unicode_score: f64,
    pub fragmentation_score: f64,
    pub symbol_substitution_score: f64,
    pub repetition_score: f64,
    pub leetspeak: bool,
    pub repetition: bool,
    pub punctuation_flood: bool,
    pub mixed_scripts: bool,
    pub confusables: bool,
    pub flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageFingerprint {
    #[serde(default = "default_fingerprint_schema_version")]
    pub schema_version: u32,
    pub raw: String,
    pub normalized: Option<String>,
    #[serde(default)]
    pub normalization_views: BTreeMap<String, String>,
    #[serde(default)]
    pub transformations: Vec<Transformation>,
    pub language_candidates: Vec<LanguageCandidate>,
    pub segments: Vec<MessageSegment>,
    pub tokens: Vec<String>,
    pub lemmas: Vec<String>,
    pub char_features: CharacterFeatures,
    pub unicode_features: UnicodeFeatures,
    pub symbols: Vec<SymbolInstance>,
    pub lexical_features: LexicalFeatures,
    pub semantic_embeddings: BTreeMap<String, Vec<f32>>,
    pub spoken_candidates: Vec<SpokenCandidate>,
    pub phonetic_candidates: Vec<PhoneticCandidate>,
    pub rebus_candidates: Vec<DecodedCandidate>,
    pub obfuscation_features: ObfuscationFeatures,
    #[serde(default)]
    pub channel_availability: BTreeMap<String, ChannelAvailability>,
    pub metadata: BTreeMap<String, String>,
}

impl MessageFingerprint {
    pub fn top_language(&self) -> Option<&str> {
        self.language_candidates
            .first()
            .map(|candidate| candidate.language.as_str())
    }

    pub fn decoded_texts(&self) -> impl Iterator<Item = &str> {
        self.rebus_candidates
            .iter()
            .map(|candidate| candidate.text.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComparisonResult {
    pub score: f64,
    pub semantic: Option<f64>,
    pub lexical: Option<f64>,
    pub character: Option<f64>,
    pub visual: Option<f64>,
    pub phonetic: Option<f64>,
    pub symbolic: Option<f64>,
    pub decoded_similarity: Option<f64>,
    pub obfuscation_similarity: Option<f64>,
    /// Short alias retained for callers from the Python MVP.
    pub obfuscation: Option<f64>,
    pub explanations: Vec<String>,
    pub evidence: Vec<String>,
    pub weights_used: BTreeMap<String, f64>,
    #[serde(default)]
    pub channel_confidence: BTreeMap<String, f64>,
    #[serde(default)]
    pub channel_available: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpamResult {
    pub probability: f64,
    pub labels: Vec<String>,
    pub reasons: Vec<String>,
    #[serde(default)]
    pub features: SpamFeatures,
    #[serde(default)]
    pub calibrated: bool,
    #[serde(default)]
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SpamFeatures {
    pub url_count: usize,
    pub email_count: usize,
    pub emoji_count: usize,
    pub number_ratio: f64,
    pub emoji_ratio: f64,
    pub uppercase_ratio: f64,
    pub punctuation_ratio: f64,
    pub entropy: f64,
    pub repetition_score: f64,
    pub obfuscation_score: f64,
    pub pattern_score: f64,
    pub decoded_similarity: f64,
    pub semantic_pattern_similarity: f64,
    pub mixed_scripts: bool,
    pub confusable_count: usize,
}

fn default_pattern_threshold() -> f64 {
    0.75
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Pattern {
    pub id: String,
    pub examples: Vec<String>,
    #[serde(default)]
    pub negative_examples: Vec<String>,
    #[serde(default = "default_pattern_threshold")]
    pub threshold: f64,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub enabled_channels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PatternMatch {
    pub id: String,
    pub score: f64,
    pub matched_example: String,
    pub explanations: Vec<String>,
    #[serde(default)]
    pub negative_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchResult {
    pub id: String,
    pub score: f64,
    pub comparison: ComparisonResult,
    #[serde(default)]
    pub candidate_count: usize,
    #[serde(default)]
    pub retrieval_channels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchCandidateSet {
    pub records: Vec<(String, MessageFingerprint)>,
    #[serde(default)]
    pub channels: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum DuplicateMode {
    #[default]
    Combined,
    NearExact,
    Lexical,
    Semantic,
    Phonetic,
    Decoded,
    Visual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DuplicateResult {
    pub duplicate: bool,
    pub score: f64,
    pub reason: String,
    #[serde(default)]
    pub mode: DuplicateMode,
}
