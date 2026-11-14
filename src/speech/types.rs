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
        let tag = value.replace('_', "-").to_ascii_lowercase();
        // These are supported reading conventions, not an invitation to
        // silently substitute another script or regional number system.
        let base = match tag.as_str() {
            "en-us" => "en",
            "es-pe" | "es-es" | "es-mx" => "es",
            "zh-cn" | "zh-hans" | "zh-hans-cn" => "zh",
            "hi-in" => "hi",
            "ar-sa" => "ar",
            "fr-fr" => "fr",
            "pt-br" => "pt",
            "ru-ru" => "ru",
            "bn-bd" => "bn",
            "ur-pk" => "ur",
            "id-id" => "id",
            "de-de" => "de",
            "ja-jp" => "ja",
            "pa-in" | "pa-guru" | "pa-guru-in" => "pa",
            "mr-in" => "mr",
            "te-in" => "te",
            "tr-tr" => "tr",
            "ta-in" => "ta",
            "vi-vn" => "vi",
            "ko-kr" => "ko",
            other if !other.contains('-') => other,
            _ => {
                return Err(format!(
                    "unsupported speech reading convention {value:?}; select a supported base language or documented regional variant"
                ));
            }
        };
        Self::ALL
            .into_iter()
            .find(|v| v.code() == base)
            .ok_or_else(|| format!("unsupported speech locale {value:?}"))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateOrder {
    DayMonthYear,
    MonthDayYear,
    RejectAmbiguous,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeechOptions {
    pub locale: SpeechLocale,
    pub date_order: DateOrder,
    /// Decimal character; grouping uses the other separator. None preserves ambiguous numbers.
    pub decimal_separator: Option<char>,
    pub max_input_chars: usize,
    pub max_spans: usize,
}
impl SpeechOptions {
    pub fn new(locale: SpeechLocale) -> Self {
        Self {
            locale,
            date_order: DateOrder::RejectAmbiguous,
            decimal_separator: None,
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
    pub reason: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeechNormalization {
    pub text: String,
    pub locale: SpeechLocale,
    pub readings: Vec<SpeechReading>,
    pub fully_supported: bool,
}
