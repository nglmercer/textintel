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
    /// Provenance of this reading (pack name, `lexicon:…`, `rule:…`).
    #[serde(default)]
    pub source: Option<String>,
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
            source: None,
        }
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }
}

/// Stable namespaced concept identifier (e.g. `concept:building.house`).
/// Bare legacy ids (`house`, `money`, `love`) are normalized on load; see
/// [`crate::resources::canonical_concept_id`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SymbolConcept {
    pub id: String,
    pub probability: f64,
    /// Provenance of this concept (pack name or `builtin:…`).
    #[serde(default)]
    pub source: Option<String>,
}

impl SymbolConcept {
    pub fn new(id: impl Into<String>, probability: f64) -> Self {
        Self {
            id: id.into(),
            probability,
            source: None,
        }
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }
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

/// One explainable rewrite step. Old payloads with only
/// `source`/`replacement`/`transformation_type` still deserialize; every new
/// field is optional provenance. (`Eq` is intentionally absent: `confidence`
/// is a float.)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Transformation {
    pub source: String,
    pub replacement: String,
    pub transformation_type: String,
    /// UTF-8 byte offset of the rewritten span in the original message.
    #[serde(default)]
    pub start: Option<usize>,
    /// UTF-8 byte offset just after the rewritten span.
    #[serde(default)]
    pub end: Option<usize>,
    /// Original UTF-8 slice that was rewritten (`message[start..end]`).
    #[serde(default)]
    pub span: Option<String>,
    /// Provider-estimated confidence in `[0.0, 1.0]` when known.
    #[serde(default)]
    pub confidence: Option<f64>,
    /// Which subsystem produced this step (`rebus`, `normalization`, …).
    #[serde(default)]
    pub provider: Option<String>,
    /// Language of the replacement when known.
    #[serde(default)]
    pub language: Option<String>,
}

impl Transformation {
    pub fn new(
        source: impl Into<String>,
        replacement: impl Into<String>,
        transformation_type: impl Into<String>,
    ) -> Self {
        Self {
            source: source.into(),
            replacement: replacement.into(),
            transformation_type: transformation_type.into(),
            start: None,
            end: None,
            span: None,
            confidence: None,
            provider: None,
            language: None,
        }
    }

    /// Attach the original UTF-8 span (`start..end` byte offsets plus the
    /// sliced text). Callers must pass char-boundary offsets.
    pub fn with_span(mut self, start: usize, end: usize, span: impl Into<String>) -> Self {
        self.start = Some(start);
        self.end = Some(end);
        self.span = Some(span.into());
        self
    }

    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = Some(confidence.clamp(0.0, 1.0));
        self
    }

    pub fn with_provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = Some(provider.into());
        self
    }

    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = Some(language.into());
        self
    }

    /// One-line human explanation (`4 = a (leetspeak)`), used by the CLI.
    pub fn explain(&self) -> String {
        match (&self.span, &self.language) {
            (Some(span), Some(language)) => format!(
                "{} = {} ({}; {})",
                span, self.replacement, self.transformation_type, language
            ),
            (Some(span), None) => {
                format!(
                    "{} = {} ({})",
                    span, self.replacement, self.transformation_type
                )
            }
            (None, _) => format!(
                "{} = {} ({})",
                self.source, self.replacement, self.transformation_type
            ),
        }
    }
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

/// One bounded entity mention: type (`person`, `organization`, `url`,
/// `email`, `mention`, `number`, `currency`, `date`, `time`), normalized
/// value, UTF-8 byte span into the source text, confidence, provider, and
/// ambient language when known.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EntityMention {
    pub entity_type: String,
    pub value: String,
    /// UTF-8 byte offset into the source text.
    pub start: usize,
    /// UTF-8 byte offset just after the mention.
    pub end: usize,
    pub confidence: f64,
    pub provider: String,
    #[serde(default)]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageFingerprint {
    #[serde(default = "default_fingerprint_schema_version")]
    pub schema_version: u32,
    pub raw: String,
    pub normalized: Option<String>,
    #[serde(default)]
    pub normalization_views: BTreeMap<String, String>,
    /// Provider confidence per `transliteration:*` view, keyed by the same
    /// view name. Fresh fingerprints always carry it; entries missing from
    /// pre-v1.0 stored payloads score at face value (1.0).
    #[serde(default)]
    pub transliteration_confidence: BTreeMap<String, f64>,
    /// Fraction of words found in the lexicon (any language), in
    /// `[0.0, 1.0]`, measured on the intended reading: when the top rebus
    /// candidate differs from the raw text, coverage runs over the
    /// candidate's words. URLs, emoji, and number-only tokens are not words
    /// and never count. Payloads predating coverage recording read 0.0,
    /// which the scorer treats as "no validity evidence" (graceful
    /// degradation to pre-validity behavior, never a false claim).
    #[serde(default)]
    pub lexicon_coverage: f64,
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
    /// Bounded entity mentions (see [`EntityMention`]). Payloads predating
    /// entity extraction read empty, which the scorer treats as "no entity
    /// evidence" (agreement and conflict both 0.0, never a penalty).
    #[serde(default)]
    pub entities: Vec<EntityMention>,
    pub metadata: BTreeMap<String, String>,
}

impl MessageFingerprint {
    /// `(text, confidence)` for every `transliteration:*` view. Views without
    /// a recorded confidence (pre-v1.0 payloads) report 1.0; non-finite
    /// confidences (hostile input) report 0.0 instead of poisoning the score.
    pub fn transliteration_views(&self) -> Vec<(&str, f64)> {
        self.normalization_views
            .iter()
            .filter(|(name, _)| name.starts_with("transliteration:"))
            .map(|(name, value)| {
                let confidence = self
                    .transliteration_confidence
                    .get(name)
                    .map(|value| {
                        if value.is_finite() {
                            value.clamp(0.0, 1.0)
                        } else {
                            0.0
                        }
                    })
                    .unwrap_or(1.0);
                (value.as_str(), confidence)
            })
            .collect()
    }

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ComparisonResult {
    pub score: f64,
    pub semantic: Option<f64>,
    pub lexical: Option<f64>,
    pub character: Option<f64>,
    pub visual: Option<f64>,
    pub phonetic: Option<f64>,
    pub symbolic: Option<f64>,
    pub decoded_similarity: Option<f64>,
    /// Best cross-view string similarity (`None` when neither side carries a
    /// transliteration view). Raw text similarity, unweighted by confidence.
    #[serde(default)]
    pub transliteration_similarity: Option<f64>,
    /// Provider confidence behind `transliteration_similarity` (the minimum
    /// over the converted views forming the best pair; 1.0 when the best
    /// pair needs no conversion). The decision-relevant evidence is the
    /// product `similarity * confidence`, which is also what view matches
    /// contribute to `decoded_similarity` instead of an unconditional 1.0.
    #[serde(default)]
    pub transliteration_confidence: Option<f64>,
    pub obfuscation_similarity: Option<f64>,
    /// Short alias retained for callers from the Python MVP.
    pub obfuscation: Option<f64>,
    /// Lexicon validity of the pair: the weaker side's `lexicon_coverage`.
    /// Confusable pairs (`their`/`there`) score near 1.0 while typo pairs
    /// (`bonjour`/`binjour`) score near 0.0, letting the trained model
    /// separate "both valid, different words" from "one side misspelled".
    #[serde(default)]
    pub lexicon_validity: f64,
    /// 1.0 when both sides are exactly one alphabetic token, else 0.0.
    /// Scopes the validity evidence: single-word near-duplicates are usually
    /// distinct words, while multi-word near-duplicates are usually matches.
    #[serde(default)]
    pub single_word_pair: f64,
    /// Character similarity of the swapped words when both sides carry the
    /// same number (≥ 2) of alphabetic tokens differing in exactly one
    /// position, else 0.0. Contextual confusables (`ensure`/`insure` in the
    /// same sentence) score high while near-duplicate sentences with an
    /// unrelated word swap (`now`/`today`) score low.
    #[serde(default)]
    pub swapped_word_similarity: f64,
    /// 1.0 when both sides reduce to the same non-empty alphanumeric string
    /// (casefolded, punctuation and spacing stripped), else 0.0. Catches
    /// punctuation-only variants (`yes`/`yes!`) that confusable penalties
    /// would otherwise reject: both sides are valid words, yet the pair is
    /// a genuine match.
    #[serde(default)]
    pub normalized_identity: f64,
    /// 1.0 when both sides use alphabetic scripts and share none (e.g.
    /// Latin vs Cyrillic), else 0.0. Separates cross-script positives
    /// (`da`/`да`) from same-script confusables: validity evidence applies
    /// within a script, while cross-script pairs route through decoded and
    /// transliteration evidence instead.
    #[serde(default)]
    pub cross_script_pair: f64,
    /// Strongest confidence among exact cross-side matches (rebus
    /// candidate or transliteration view equal to the other side's raw,
    /// normalized, candidate, or view text), else 0.0. Exact decoding
    /// (`cheque`→`check`, `привет`→`privet`) far outweighs fuzzy
    /// resemblance, so this separates genuine variants from confusables
    /// that merely look alike; ambiguous low-rank decodes (e.g. `gr8`→
    /// `grate`) contribute only their weak confidence.
    #[serde(default)]
    pub exact_decode: f64,
    /// Phonetic similarity of the swapped words under the same gating as
    /// `swapped_word_similarity` (equal token counts ≥ 2, exactly one
    /// differing position), scaled by pair lexicon validity and suppressed
    /// to 0.0 on cross-language switches, else 0.0. Contextual confusables
    /// swap same-sounding words (`dairy`/`diary`); near-duplicate sentences
    /// swap unrelated words (`package`/`parcel`).
    #[serde(default)]
    pub swapped_phonetic: f64,
    /// Confusable-swap interaction: `swapped_word_similarity` × ungated
    /// swapped-word phonetic similarity, kept only when every word on both
    /// sides is lexicon-valid AND both swapped words are lexicon words
    /// themselves, else 0.0. This is the contextual-hard-negative
    /// signature — an otherwise identical sentence pair whose one differing
    /// word is real, look-alike, and sound-alike on both sides
    /// (`right`/`rite`, `complement`/`compliment`). Typo pairs fail the
    /// all-valid test (`thamk` is not a word), abbreviation pairs fail the
    /// swapped-words test (`vc` counts toward coverage through `você` but is
    /// not a word), paraphrases fail the swap gate, and language switches
    /// (`mi`/`my` in known-different-language segments) are suppressed
    /// outright — none of them pays the confusable penalty.
    #[serde(default)]
    pub confusable_swap: f64,
    /// Single-word exact decoding: `single_word_pair` × `exact_decode`.
    /// A lone exact read (`cheque`→`check`) is the variant signature, while
    /// single-word confusables (`their`/`there`) decode to nothing; the
    /// interaction lets the linear model acquit decoded singles without
    /// acquitting undecodable ones.
    #[serde(default)]
    pub single_word_exact: f64,
    /// Cross-script agreement: `cross_script_pair` × top-language
    /// agreement. A cross-script pair whose sides detect as the SAME
    /// language (pinyin `hao` and `好` both read as Chinese) is
    /// transliteration-shaped; cross-script pairs in different languages
    /// (`net`/`нет`) stay at 0. Lets the model trust same-language
    /// cross-script evidence without trusting cross-language look-alikes.
    #[serde(default)]
    pub cross_script_agreement: f64,
    /// Substring containment between the alphanumeric folds (see
    /// scorer): 1.0 when one side contains the other with enough
    /// substance (`morning` in `good morning`, `早上` in `早上好`).
    #[serde(default)]
    pub substring_containment: f64,
    /// Swap-gate validity × cubed swapped-word character similarity,
    /// suppressed on cross-language switches. Real-word look-alike swaps
    /// (`money`/`honey`) score high; typo swaps (one side misspelled) score
    /// 0, and unrelated-word swaps (`grown`/`expanded`) score near zero
    /// through the cube. Lets the model penalize malapropisms without
    /// taxing synonym swaps.
    #[serde(default)]
    pub valid_swap_similarity: f64,
    /// Entity agreement in `[0.0, 1.0]` (see
    /// [`crate::entities::entity_agreement`]): same or compatible entities
    /// across the pair. Empty on either side reads `0.0` (no bonus, never a
    /// penalty).
    #[serde(default)]
    pub entity_agreement: f64,
    /// Entity conflict in `[0.0, 1.0]` (see
    /// [`crate::entities::entity_conflict`]): shared entity types with
    /// disjoint values. Empty on either side reads `0.0` (never a penalty).
    #[serde(default)]
    pub entity_conflict: f64,
    /// Language/context compatibility discount for transliteration evidence
    /// in `[0.0, 1.0]` (see
    /// [`crate::transliteration::transliteration_compatibility`]). The
    /// decision-relevant transliteration evidence is
    /// `similarity × confidence × compatibility`.
    #[serde(default)]
    pub transliteration_compatibility: f64,
    /// Transformer-only semantic cosine: the semantic channel value when
    /// both sides carry Production-quality embeddings, else `0.0`. Lets the
    /// model trust contextual multilingual evidence without trusting the
    /// feature-hash fallback equally.
    #[serde(default)]
    pub contextual_semantic: f64,
    /// Semantic cosine when the top languages differ, else `0.0`.
    /// Cross-language matches route here; same-language pairs read `0.0`.
    #[serde(default)]
    pub cross_language_semantic: f64,
    /// Semantic cosine when lexical similarity is low (`< 0.3`), else
    /// `0.0`. Paraphrases with disjoint lexicons route here; pairs with
    /// lexical support read `0.0`.
    #[serde(default)]
    pub semantic_without_lexical_overlap: f64,
    /// Cross-script transliteration evidence backed by semantics:
    /// `weighted × semantic_cosine × cross_script`, else `0.0`. Same-script
    /// pairs read `0.0` (their Latin→X views are spurious byproducts, not
    /// transliteration links).
    #[serde(default)]
    pub transliteration_semantic_agreement: f64,
    /// Cross-script look-alike without semantic support where both sides
    /// are lexicon-valid words in different languages:
    /// `weighted × (1 - semantic) × validity × language_mismatch ×
    /// cross_script`, else `0.0`. The false-friend signature
    /// (valid-but-different words); genuine transliterations (one side
    /// usually lexicon-invalid, e.g. romanizations) read near `0.0`. When
    /// semantic evidence is absent the semantic factor is neutral (`1.0`)
    /// so the validity signature still fires.
    #[serde(default)]
    pub transliteration_semantic_conflict: f64,
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

/// Per-stage timing diagnostics in microseconds. Carries durations only —
/// never input text, embeddings, or other user data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct StageTimings {
    pub normalization_micros: f64,
    pub language_micros: f64,
    pub symbols_micros: f64,
    pub rebus_micros: f64,
    pub semantic_micros: f64,
    pub phonetic_micros: f64,
    pub comparison_micros: f64,
    pub total_micros: f64,
}

impl StageTimings {
    /// Stage name → microseconds, in pipeline order.
    pub fn as_map(&self) -> BTreeMap<String, f64> {
        [
            ("normalization", self.normalization_micros),
            ("language", self.language_micros),
            ("symbols", self.symbols_micros),
            ("rebus", self.rebus_micros),
            ("semantic", self.semantic_micros),
            ("phonetic", self.phonetic_micros),
            ("comparison", self.comparison_micros),
            ("total", self.total_micros),
        ]
        .into_iter()
        .map(|(name, value)| (name.to_string(), value))
        .collect()
    }

    /// Stage names in pipeline order.
    pub fn stage_names() -> &'static [&'static str] {
        &[
            "normalization",
            "language",
            "symbols",
            "rebus",
            "semantic",
            "phonetic",
            "comparison",
            "total",
        ]
    }
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
