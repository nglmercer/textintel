//! Typed, lossless spans for speech-oriented text normalization.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeechLocale {
    En,
    Es,
    Zh,
    Hi,
    Ar,
    Fr,
    Pt,
    Ru,
    Bn,
    Ur,
    Id,
    De,
    Ja,
    Pa,
    Mr,
    Te,
    Tr,
    Ta,
    Vi,
    Ko,
}
impl SpeechLocale {
    pub const ALL: [Self; 20] = [
        Self::En,
        Self::Es,
        Self::Zh,
        Self::Hi,
        Self::Ar,
        Self::Fr,
        Self::Pt,
        Self::Ru,
        Self::Bn,
        Self::Ur,
        Self::Id,
        Self::De,
        Self::Ja,
        Self::Pa,
        Self::Mr,
        Self::Te,
        Self::Tr,
        Self::Ta,
        Self::Vi,
        Self::Ko,
    ];
    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Es => "es",
            Self::Zh => "zh",
            Self::Hi => "hi",
            Self::Ar => "ar",
            Self::Fr => "fr",
            Self::Pt => "pt",
            Self::Ru => "ru",
            Self::Bn => "bn",
            Self::Ur => "ur",
            Self::Id => "id",
            Self::De => "de",
            Self::Ja => "ja",
            Self::Pa => "pa",
            Self::Mr => "mr",
            Self::Te => "te",
            Self::Tr => "tr",
            Self::Ta => "ta",
            Self::Vi => "vi",
            Self::Ko => "ko",
        }
    }
}
impl std::str::FromStr for SpeechLocale {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let tag = value.replace('_', "-");
        let language = super::locales::LANGUAGES.iter().find(|entry| {
            entry.code.eq_ignore_ascii_case(&tag)
                || entry
                    .typed_aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(&tag))
        });
        language.and_then(|entry| Self::ALL.into_iter().find(|v|v.code()==entry.code))
            .ok_or_else(||format!("unsupported speech reading convention {value:?}; select a supported base language or documented regional variant"))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateOrder {
    DayMonthYear,
    MonthDayYear,
    YearMonthDay,
    RejectAmbiguous,
}
/// How written numbers and times are read.
///
/// [`ReadingStyle::Precise`] is the compatibility default and retains literal
/// numeric precision. The typed grammar and versioned normalizer have their
/// own documented phrase conventions. [`ReadingStyle::Natural`] opts into
/// conversational wording; [`ReadingStyle::Digits`] reads applicable integers
/// digit by digit. A style does not relax calendar or clock validation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReadingStyle {
    /// Literal, high-precision reading. Compatibility default.
    #[default]
    Precise,
    /// Conversational wording for the supported language and semantic domain.
    Natural,
    /// Digit-by-digit reading where the language provides it.
    Digits,
}

impl ReadingStyle {
    /// Parse `"precise"`, `"natural"` or `"digits"` (trimmed, case-insensitive).
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "precise" | "literal" => Some(Self::Precise),
            "natural" => Some(Self::Natural),
            "digits" | "digit" => Some(Self::Digits),
            _ => None,
        }
    }

    /// Stable lowercase name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Precise => "precise",
            Self::Natural => "natural",
            Self::Digits => "digits",
        }
    }
}

/// Explicit semantic context supplied by a TTS caller (similar to say-as).
/// Auto never resolves a date/number/time collision from the language alone.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeechInterpretation {
    #[default]
    Auto,
    Number,
    Date,
    Time,
    Currency,
    Measure,
    Duration,
}
/// Rendering notation for languages with distinct pronunciation and written forms.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumberNotation {
    #[default]
    Phonetic,
    Orthographic,
}

/// How currency names retain their localized lexical qualifiers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CurrencyNaming {
    /// Preserve the selected reading style's established naming convention.
    #[default]
    Style,
    /// Use full localized names, retaining qualifiers in the shared lexicon.
    Full,
}
impl CurrencyNaming {
    pub fn parse(value: &str) -> Option<Self> {
        if value.trim().eq_ignore_ascii_case("style") {
            Some(Self::Style)
        } else if value.trim().eq_ignore_ascii_case("full") {
            Some(Self::Full)
        } else {
            None
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Style => "style",
            Self::Full => "full",
        }
    }
}

/// Explicit source policy for a redundant AM/PM marker on a 24-hour clock.
/// Rendered clock values always use canonical fields independently of this policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClockPolicy {
    /// AM/PM clock hours must be in 1..=12.
    #[default]
    Strict,
    /// Normalize 13..=23 PM and 0 AM, retaining the same time of day.
    /// Contradictory periods and out-of-range fields remain unresolved.
    NormalizeRedundantPeriod,
}
impl ClockPolicy {
    pub fn parse(value: &str) -> Option<Self> {
        if value.trim().eq_ignore_ascii_case("strict") {
            Some(Self::Strict)
        } else if value
            .trim()
            .eq_ignore_ascii_case("normalize-redundant-period")
        {
            Some(Self::NormalizeRedundantPeriod)
        } else {
            None
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::NormalizeRedundantPeriod => "normalize-redundant-period",
        }
    }
}
/// How to read an impossible Gregorian date with known individual components.
/// A literal reading never turns those components into a valid calendar value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvalidDatePolicy {
    /// Preserve the source of an invalid date and report its diagnostic.
    #[default]
    Preserve,
    /// Read known atomic components literally, retaining invalidity and source.
    ReadComponents,
}
impl InvalidDatePolicy {
    pub fn parse(value: &str) -> Option<Self> {
        if value.trim().eq_ignore_ascii_case("preserve") {
            Some(Self::Preserve)
        } else if value.trim().eq_ignore_ascii_case("read-components") {
            Some(Self::ReadComponents)
        } else {
            None
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preserve => "preserve",
            Self::ReadComponents => "read-components",
        }
    }
}
/// Context-bounded abbreviation expansion. Bibliographic conventions require
/// an explicit opt-in; ordinary text never inherits chat/slang substitutions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbbreviationPolicy {
    Preserve,
    #[default]
    Contextual,
    Bibliographic,
}
impl AbbreviationPolicy {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "preserve" => Some(Self::Preserve),
            "contextual" => Some(Self::Contextual),
            "bibliographic" => Some(Self::Bibliographic),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preserve => "preserve",
            Self::Contextual => "contextual",
            Self::Bibliographic => "bibliographic",
        }
    }
    pub(crate) fn is_contextual(&self) -> bool {
        *self == Self::Contextual
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeechOptions {
    pub locale: SpeechLocale,
    pub date_order: DateOrder,
    /// Decimal character; grouping uses the other separator. None preserves ambiguous numbers.
    pub decimal_separator: Option<char>,
    #[serde(default)]
    pub interpret_as: SpeechInterpretation,
    #[serde(default)]
    pub reading_style: ReadingStyle,
    #[serde(default)]
    pub number_notation: NumberNotation,
    #[serde(default)]
    pub currency_naming: CurrencyNaming,
    #[serde(default)]
    pub clock_policy: ClockPolicy,
    #[serde(default)]
    pub invalid_date_policy: InvalidDatePolicy,
    #[serde(default, skip_serializing_if = "AbbreviationPolicy::is_contextual")]
    pub abbreviation_policy: AbbreviationPolicy,
    /// Explicit ISO currency for ambiguous symbols; absent means preserve ambiguity.
    #[serde(default)]
    pub default_currency: Option<String>,
    /// Per-symbol ISO assignments supplied by the caller, e.g. "$" -> "USD".
    #[serde(default)]
    pub currency_symbols: std::collections::BTreeMap<String, String>,
    /// Opt-in written unit aliases mapped to canonical CLDR unit keys.
    #[serde(default)]
    pub unit_aliases: std::collections::BTreeMap<String, String>,
    pub max_input_chars: usize,
    pub max_spans: usize,
}
impl SpeechOptions {
    pub fn validate(&self) -> Result<(), crate::TextIntelError> {
        let invalid = |s: &str| crate::TextIntelError::InvalidConfiguration(s.into());
        if self.max_input_chars == 0 || self.max_spans == 0 {
            return Err(invalid("speech limits must be positive"));
        }
        if !matches!(self.decimal_separator, None | Some('.' | ',' | '٫')) {
            return Err(invalid("speech decimal_separator must be '.', ',' or '٫'"));
        }
        let valid_code =
            |code: &str| code.len() == 3 && code.bytes().all(|b| b.is_ascii_uppercase());
        if self
            .default_currency
            .as_deref()
            .is_some_and(|code| !valid_code(code))
            || self
                .currency_symbols
                .iter()
                .any(|(symbol, code)| symbol.is_empty() || !valid_code(code))
        {
            return Err(invalid(
                "explicit currencies need nonempty symbols and uppercase three-letter ISO codes",
            ));
        }
        let max_entries = super::dictionary::MAX_ENTRIES;
        let max_chars = super::dictionary::MAX_ENTRY_CHARS;
        if self.unit_aliases.len() > max_entries
            || self.currency_symbols.len() > max_entries
            || self.unit_aliases.iter().any(|(alias, key)| {
                alias.chars().count() > max_chars || key.chars().count() > max_chars
            })
            || self
                .currency_symbols
                .keys()
                .any(|alias| alias.chars().count() > max_chars)
        {
            return Err(invalid(
                "speech alias dictionaries exceed their entry or character limits",
            ));
        }
        if self.unit_aliases.iter().any(|(alias, key)| {
            alias.is_empty()
                || key.is_empty()
                || !super::quantities::supports_unit(self.locale, key)
        }) {
            return Err(invalid(
                "unit aliases need a nonempty written form and a supported canonical unit key",
            ));
        }
        Ok(())
    }
    pub fn new(locale: SpeechLocale) -> Self {
        Self {
            locale,
            date_order: DateOrder::RejectAmbiguous,
            decimal_separator: None,
            interpret_as: SpeechInterpretation::Auto,
            reading_style: ReadingStyle::Precise,
            number_notation: NumberNotation::Phonetic,
            currency_naming: CurrencyNaming::Style,
            clock_policy: ClockPolicy::Strict,
            invalid_date_policy: InvalidDatePolicy::Preserve,
            abbreviation_policy: AbbreviationPolicy::Contextual,
            default_currency: None,
            currency_symbols: Default::default(),
            unit_aliases: Default::default(),
            max_input_chars: 8192,
            max_spans: 512,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NumberValue {
    pub negative: bool,
    pub integer: String,
    pub fraction: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumberReading {
    Cardinal,
    Digits,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SpeechValue {
    Text,
    /// A bounded catalog entry, never an arbitrary user-provided replacement.
    Abbreviation {
        id: String,
    },
    Number {
        value: NumberValue,
        reading: NumberReading,
    },
    Date {
        year: u32,
        month: u32,
        day: u32,
    },
    Time {
        hour: u32,
        minute: u32,
        second: Option<u32>,
    },
    Expression {
        value: super::expressions::ExpressionValue,
    },
    Temporal {
        value: super::temporal::TemporalValue,
    },
    InflectedNumber {
        value: NumberValue,
        inflection: super::grammar::NumberInflection,
    },
    MeasuredExpression {
        unit: String,
        value: super::expressions::ExpressionValue,
    },
    LabelledNumber {
        label: String,
        value: NumberValue,
        number_first: bool,
        separator: String,
    },
    Currency {
        code: String,
        value: NumberValue,
    },
    Percent {
        value: NumberValue,
    },
    Measure {
        unit: String,
        value: NumberValue,
    },
    Identifier {
        kind: String,
    },
    Ambiguous {
        reason: String,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeechSpan {
    pub start: usize,
    pub end: usize,
    pub source: String,
    pub value: SpeechValue,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeechDocument {
    pub text: String,
    pub locale: SpeechLocale,
    pub spans: Vec<SpeechSpan>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeechReading {
    pub start: usize,
    pub end: usize,
    pub source: String,
    pub spoken: String,
    pub supported: bool,
    /// An unsupported-reading reason, or a retained invalid-date diagnostic
    /// when its components were explicitly requested as literal speech.
    pub reason: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeechNormalization {
    pub text: String,
    pub locale: SpeechLocale,
    pub readings: Vec<SpeechReading>,
    pub fully_supported: bool,
}
