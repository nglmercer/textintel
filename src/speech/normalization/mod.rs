//! Shared, offline, rule-based TTS text normalization ("TN").
//!
//! Turns written forms (numbers, dates, prices, times, units, ...) into
//! speakable text before chunking and before any provider-specific
//! preprocessing. The pipeline is span based, never a chain of global
//! regex replacements:
//!
//! ```text
//! original text (immutable)
//!   -> input length + language + locale validation   (options.rs)
//!   -> Unicode-aware field scan, protected regions    (lexer.rs)
//!   -> classification with ambiguity policy           (classify.rs)
//!   -> locale-aware spoken forms                      (rules/*.rs)
//!   -> splice + budget checks                         (render.rs)
//! ```
//!
//! Policy: ambiguous input is preserved byte-for-byte and reported as a
//! warning, never guessed. Text outside replaced spans is preserved
//! byte-for-byte. `off` mode returns the input unchanged. Languages without
//! rules pass through unchanged and report `none` coverage.
//!
//! See `docs/SPEECH.md` for coverage and limitations.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

mod classify;

use super::lexer;
mod options;
mod render;
mod rules;
pub mod segments;
mod shared;

#[cfg(test)]
mod tests;

pub use options::{
    AbbreviationPolicy, NormalizationMode, NormalizeLimits, NormalizeOptions, ReadingStyle,
    default_locale_for, is_supported_locale, locales_for,
};

/// Version of the rule set. Participates in [`SpeechText::identity`].
///
/// `tn-v4` shares the formal abbreviation catalogue with the typed pipeline,
/// preserving legacy contexts while extending languages without legacy rules.
/// Cache identities change when semantics change.
pub const RULESET_VERSION: &str = "tn-v4";

/// Stable feature keys used in coverage maps.
pub const FEATURE_KEYS: [&str; 11] = [
    "cardinal_numbers",
    "decimals",
    "ordinals",
    "percentages",
    "currency",
    "dates",
    "times",
    "units",
    "abbreviations",
    "ranges",
    "fractions",
];

/// How completely a language implements a feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CoverageLevel {
    /// All common written forms of the feature are converted.
    Full,
    /// Some forms are converted; the rest are preserved.
    Partial,
    /// Nothing is converted (pass-through).
    None,
}

/// Category of a replaced span.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Number,
    Decimal,
    Ordinal,
    Percentage,
    Currency,
    Date,
    Time,
    Unit,
    Abbreviation,
    Range,
    Fraction,
}

/// Confidence attached to a change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    High,
    Medium,
}

/// One replaced span of the original text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextChange {
    pub kind: ChangeKind,
    /// Byte offset into the ORIGINAL text (char boundary).
    pub start: usize,
    /// Byte offset into the ORIGINAL text, exclusive (char boundary).
    pub end: usize,
    /// Spoken form that replaces `original[start..end]`.
    pub replacement: String,
    pub confidence: Confidence,
}

/// Machine-readable warning codes (serialized as snake_case).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningCode {
    AmbiguousNumber,
    AmbiguousDate,
    InvalidDate,
    AmbiguousCurrency,
    AmbiguousTime,
    AmbiguousFraction,
    NumberTooLarge,
    StructuredData,
}

impl WarningCode {
    /// Stable snake_case name, identical to the serde form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AmbiguousNumber => "ambiguous_number",
            Self::AmbiguousDate => "ambiguous_date",
            Self::InvalidDate => "invalid_date",
            Self::AmbiguousCurrency => "ambiguous_currency",
            Self::AmbiguousTime => "ambiguous_time",
            Self::AmbiguousFraction => "ambiguous_fraction",
            Self::NumberTooLarge => "number_too_large",
            Self::StructuredData => "structured_data",
        }
    }
}

/// A preserved span that the caller may want to review.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizationWarning {
    pub code: WarningCode,
    /// Byte offsets into the ORIGINAL text.
    pub start: usize,
    pub end: usize,
}

/// Result of [`normalize`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NormalizedText {
    /// Text to synthesize.
    pub spoken: String,
    /// Replaced spans, ordered by `start`, non-overlapping.
    pub changes: Vec<TextChange>,
    /// Empty when `report_ambiguities` is false. Ordered by `start`.
    pub warnings: Vec<NormalizationWarning>,
    /// Per-feature coverage for this language/locale.
    pub coverage: BTreeMap<String, CoverageLevel>,
    pub mode: NormalizationMode,
    pub language: String,
    pub locale: String,
    pub ruleset_version: &'static str,
    /// Selected abbreviation scope; omitted from JSON for the contextual default.
    #[serde(default, skip_serializing_if = "AbbreviationPolicy::is_contextual")]
    pub abbreviation_policy: AbbreviationPolicy,
}

/// Errors from option resolution and normalization. Messages never contain
/// user text.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum NormalizeError {
    #[error("input exceeds {max_chars} characters")]
    InputTooLong { max_chars: usize },
    #[error("spoken text would exceed {max_chars} characters")]
    OutputTooLong { max_chars: usize },
    #[error("spoken text expansion exceeds the configured ratio")]
    ExpansionTooLarge,
    #[error("speech reading options or input structure are invalid")]
    InvalidReadingOptions,
    #[error("speech language is not a valid language code")]
    InvalidLanguage,
    #[error("locale is not a valid BCP-47 tag")]
    InvalidLocale,
    #[error("locale is not supported for this language")]
    UnsupportedLocale,
    #[error("locale does not belong to the speech language")]
    LocaleLanguageMismatch,
}

/// Normalize `text` for speech.
///
/// Pure, offline and deterministic. Never panics; runs in time linear in the
/// input length (inputs above `max_input_chars` are rejected before any
/// scanning beyond the length check).
pub fn normalize(text: &str, opts: &NormalizeOptions) -> Result<NormalizedText, NormalizeError> {
    let limits = opts.limits;
    let input_chars = render::count_chars_bounded(text, limits.max_input_chars).ok_or(
        NormalizeError::InputTooLong {
            max_chars: limits.max_input_chars,
        },
    )?;
    let language_rules = rules::for_language(&opts.language);
    let mut coverage = if shared::uses_shared(&opts.language) {
        shared::coverage(&opts.language, opts.abbreviation_policy)
    } else {
        rules::coverage_map(language_rules)
    };
    if opts.language.parse::<crate::speech::SpeechLocale>().is_ok() {
        for (key, level) in shared::coverage(&opts.language, opts.abbreviation_policy) {
            if coverage[&key] == CoverageLevel::None && level != CoverageLevel::None {
                coverage.insert(key, level);
            }
        }
    }

    if opts.abbreviation_policy == AbbreviationPolicy::Preserve {
        coverage.insert("abbreviations".into(), CoverageLevel::None);
    }

    let (changes, warnings) = match opts.mode {
        NormalizationMode::Off => (Vec::new(), Vec::new()),
        NormalizationMode::Auto
            if matches!(text.trim_start().chars().next(), Some('{' | '['))
                && serde_json::from_str::<serde_json::Value>(text)
                    .is_ok_and(|v| v.is_object() || v.is_array()) =>
        {
            (
                Vec::new(),
                vec![NormalizationWarning {
                    code: WarningCode::StructuredData,
                    start: 0,
                    end: text.len(),
                }],
            )
        }
        NormalizationMode::Auto => {
            let source = lexer::LexicalSource::new(text);
            if shared::uses_shared(&opts.language) {
                shared::classify(&source, opts)?
            } else {
                let (mut changes, mut warnings) = classify::classify(&source, language_rules, opts);
                shared::augment(&source, opts, language_rules, &mut changes, &mut warnings)?;
                (changes, warnings)
            }
        }
    };
    let spoken = if changes.is_empty() {
        text.to_string()
    } else {
        render::splice(text, &changes)
    };
    if opts.mode == NormalizationMode::Auto {
        render::check_budget(&spoken, input_chars, &limits)?;
    }
    Ok(NormalizedText {
        spoken,
        changes,
        warnings: if opts.report_ambiguities {
            warnings
        } else {
            Vec::new()
        },
        coverage,
        mode: opts.mode,
        language: opts.language.clone(),
        locale: opts.locale.clone(),
        ruleset_version: RULESET_VERSION,
        abbreviation_policy: opts.abbreviation_policy,
    })
}

/// Typed marker for text that has been through [`normalize`] (or `off`).
///
/// Synthesis entry points take this type rather than raw user strings so
/// that text is prepared exactly once.
#[derive(Clone, Debug)]
pub struct SpeechText {
    spoken: String,
    identity: String,
    changed: bool,
}

impl SpeechText {
    /// Normalize `text` and keep only the spoken form and identity.
    pub fn prepare(text: &str, opts: &NormalizeOptions) -> Result<Self, NormalizeError> {
        let normalized = normalize(text, opts)?;
        let changed = normalized.spoken != text;
        Ok(Self {
            identity: format!(
                "{}|{}|{}|{}|{}|abbr={}",
                RULESET_VERSION,
                opts.mode.as_str(),
                opts.language,
                opts.locale,
                opts.reading_style.as_str(),
                opts.abbreviation_policy.as_str()
            ),
            spoken: normalized.spoken,
            changed,
        })
    }

    /// Spoken text.
    pub fn as_str(&self) -> &str {
        &self.spoken
    }

    /// Consume into the spoken text.
    pub fn into_string(self) -> String {
        self.spoken
    }

    /// Stable identity, e.g. `"tn-v4|auto|es|es-PE|precise|abbr=contextual"`. Contains no user text.
    pub fn identity(&self) -> &str {
        &self.identity
    }

    /// `true` when the spoken text differs from the original.
    pub fn changed(&self) -> bool {
        self.changed
    }
}

/// Normalization coverage of one supported language.
#[derive(Clone, Debug, Serialize)]
pub struct LanguageCoverage {
    pub language: &'static str,
    pub locales: &'static [&'static str],
    pub default_locale: &'static str,
    /// Overall level: `full` only when every feature is full, `none` when
    /// every feature is none, `partial` otherwise.
    pub rules: CoverageLevel,
    pub features: BTreeMap<&'static str, CoverageLevel>,
}

/// One entry per supported language, for `/api/info` and the UI.
pub fn language_coverage() -> Vec<LanguageCoverage> {
    crate::speech::locales::LANGUAGES
        .iter()
        .map(|entry| {
            let (language, locales, default_locale) =
                (entry.code, entry.locales, entry.default_locale);
            let language_rules = rules::for_language(language);
            let shared_scope = shared::coverage(language, AbbreviationPolicy::Contextual);
            let features: BTreeMap<&'static str, CoverageLevel> = rules::Feature::ALL
                .iter()
                .map(|feature| {
                    (
                        feature.key(),
                        if language.parse::<crate::speech::SpeechLocale>().is_ok()
                            && language_rules.coverage(*feature) == CoverageLevel::None
                        {
                            shared_scope[feature.key()]
                        } else {
                            language_rules.coverage(*feature)
                        },
                    )
                })
                .collect();
            LanguageCoverage {
                language,
                locales,
                default_locale,
                rules: rules::overall(features.values().copied()),
                features,
            }
        })
        .collect()
}
