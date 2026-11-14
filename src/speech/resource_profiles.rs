//! Bounded resource seeds derived from the speech data already shipped.
//!
//! This module only reads leaf lexicons and exact number composition. It must
//! not call the normalizer, an engine, or `embedded_resources`: it is used
//! while that resource singleton is being initialized.
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::OnceLock;

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::lexicons::{currency_name, month, spoken_unit_pattern, unit_forms};
use super::numbers::verbalize_number;
use super::types::{NumberReading, NumberValue, SpeechLocale};
use crate::core::types::{SymbolConcept, SymbolReading};
use crate::resources::{LanguagePack, LexiconEntry, ResourceError, SymbolPack, SymbolResource};

pub(crate) const MAX_PROFILE_TERMS: usize = 1024;
pub(crate) const MAX_PROFILE_TERM_CHARS: usize = 128;
pub(crate) const REVISION: &str = "speech-domain-seed-v2";
const SOURCE: &str = "derived:src/speech/data/numbers.json+src/speech/data/lexicons.json";
const SYMBOL_SOURCE: &str = "derived:src/speech/data/numbers.json+src/speech/data/lexicons.json+src/speech/data/symbol-labels.json";
const LICENSE: &str = "See repository LICENSE and src/speech/data/LICENSE-UNICODE.txt; symbol annotations retain src/speech/data/LICENSE-CLDR-ANNOTATIONS.txt; Marathi numeric entries also retain src/speech/data/LICENSE-NUMBERTEXT.txt";

pub(crate) struct DerivedPacks {
    pub language_json: String,
    pub symbol_json: String,
}

fn error(locale: SpeechLocale, message: &str) -> ResourceError {
    ResourceError::Validation {
        path: PathBuf::from(format!("<derived:speech:{}>", locale.code())),
        message: message.to_owned(),
    }
}

fn number(locale: SpeechLocale, integer: u32) -> Option<String> {
    verbalize_number(
        &NumberValue {
            negative: false,
            integer: integer.to_string(),
            fraction: None,
        },
        locale,
        NumberReading::Cardinal,
    )
}

fn symbol_labels(locale: SpeechLocale) -> Result<&'static BTreeMap<String, String>, ResourceError> {
    static LABELS: OnceLock<BTreeMap<String, BTreeMap<String, String>>> = OnceLock::new();
    LABELS
        .get_or_init(|| {
            serde_json::from_str(include_str!("data/symbol-labels.json"))
                .expect("valid bundled CLDR symbol labels")
        })
        .get(locale.code())
        .ok_or_else(|| error(locale, "bundled symbol labels lack the requested language"))
}

fn reading(token: &str, label: String, locale: SpeechLocale, kind: &str) -> SymbolResource {
    SymbolResource {
        token: token.into(),
        unicode_name: None,
        concepts: vec![],
        readings: vec![
            SymbolReading::new(label, Some(locale.code()), 0.5, kind).with_source(SYMBOL_SOURCE),
        ],
    }
}

fn percent_label(locale: SpeechLocale) -> Option<String> {
    let zero = NumberValue {
        negative: false,
        integer: "0".into(),
        fraction: None,
    };
    let words = number(locale, 0)?;
    let spoken = super::verbalize::quantity(locale, "concentr-percent", &zero)?;
    spoken
        .strip_prefix(&words)
        .or_else(|| spoken.strip_suffix(&words))
        .map(|label| label.trim().to_owned())
        .filter(|label| !label.is_empty() && label.chars().any(char::is_alphabetic))
}

fn with_digest(pack: &impl Serialize, locale: SpeechLocale) -> Result<String, ResourceError> {
    let mut value = serde_json::to_value(pack)
        .map_err(|_| error(locale, "could not serialize generated speech resource"))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| error(locale, "generated speech resource must be an object"))?;
    object.remove("sha256");
    let canonical = serde_json::to_vec(&value)
        .map_err(|_| error(locale, "could not serialize generated speech resource"))?;
    let digest = Sha256::digest(canonical)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    value
        .as_object_mut()
        .expect("validated resource object")
        .insert("sha256".into(), digest.into());
    serde_json::to_string(&value)
        .map_err(|_| error(locale, "could not serialize generated speech resource"))
}

/// Reuse canonical spellout, month labels, and localized unit nouns. The seed
/// is a narrow domain vocabulary, with no invented stop words or fake corpus.
pub(crate) fn derive(locale: SpeechLocale) -> Result<DerivedPacks, ResourceError> {
    let mut terms = BTreeSet::new();
    let mut symbols = Vec::new();
    for integer in 0..=99 {
        terms.insert(
            number(locale, integer)
                .ok_or_else(|| error(locale, "bundled numeric seed lacks a basic cardinal"))?,
        );
    }
    for integer in (0..=10).chain([100]) {
        let reading = number(locale, integer)
            .ok_or_else(|| error(locale, "bundled numeric seed lacks a symbol reading"))?;
        symbols.push(SymbolResource {
            token: integer.to_string(),
            unicode_name: None,
            concepts: vec![SymbolConcept::new("concept:math.number", 0.5)],
            readings: vec![
                SymbolReading::new(reading, Some(locale.code()), 0.5, "speech_number_seed")
                    .with_source(SOURCE),
            ],
        });
    }
    for m in 1..=12 {
        terms.insert(
            month(locale, m)
                .ok_or_else(|| error(locale, "bundled speech seed lacks a month name"))?
                .to_owned(),
        );
    }
    let unit_keys: BTreeSet<&str> = unit_forms(locale).iter().map(|(_, key)| *key).collect();
    for key in unit_keys {
        for category in ["one", "two", "few", "many", "other"] {
            if let Some((pattern, _)) = spoken_unit_pattern(locale, key, category) {
                let label = pattern.replace("{0}", "").trim().to_owned();
                // Profile lexical words come from localized noun patterns;
                // neutral SI aliases stay out of language-identification data.
                if label.chars().any(char::is_alphabetic) && !label.contains(['{', '}']) {
                    terms.insert(label);
                }
            }
        }
    }
    symbols.push(reading(
        "%",
        percent_label(locale).ok_or_else(|| {
            error(
                locale,
                "bundled speech data lacks a spoken percentage label",
            )
        })?,
        locale,
        "speech_unit_seed",
    ));
    for (token, label) in symbol_labels(locale)? {
        if label.is_empty() || label.chars().count() > MAX_PROFILE_TERM_CHARS {
            return Err(error(locale, "bundled symbol label exceeds its bounds"));
        }
        symbols.push(reading(token, label.clone(), locale, "cldr_annotation_tts"));
    }
    // These are conventional spoken symbol names, not a currency-resolution
    // policy. The speech parser still preserves ambiguous symbols without an
    // explicit source convention; symbol resources expose possible readings.
    for (token, code) in [
        ("$", "USD"),
        ("£", "GBP"),
        ("¥", "JPY"),
        ("€", "EUR"),
        ("₹", "INR"),
    ] {
        let label = currency_name(locale, code, "other")
            .ok_or_else(|| error(locale, "bundled speech data lacks a currency symbol name"))?;
        symbols.push(reading(token, label.into(), locale, "cldr_currency_name"));
    }
    if symbols.len() != 35 {
        return Err(error(locale, "derived baseline symbol set is incomplete"));
    }
    if terms.len() > MAX_PROFILE_TERMS
        || terms
            .iter()
            .any(|word| word.is_empty() || word.chars().count() > MAX_PROFILE_TERM_CHARS)
    {
        return Err(error(
            locale,
            "derived speech profile exceeds its built-in bounds",
        ));
    }
    let language = LanguagePack {
        schema_version: 1,
        language: locale.code().into(),
        name: format!("{} speech-domain seed", locale.code()),
        entries: terms
            .into_iter()
            .map(|word| LexiconEntry {
                word,
                lemma: None,
                stop_word: false,
                weight: 0.5,
                source: Some(SOURCE.into()),
            })
            .collect(),
        words: vec![],
        stop_words: vec![],
        slang: vec![],
        examples: vec![],
        source: Some(SOURCE.into()),
        license: Some(LICENSE.into()),
        revision: Some(REVISION.into()),
        sha256: None,
    };
    let symbols = SymbolPack {
        schema_version: 1,
        language: Some(locale.code().into()),
        name: format!("{} shared speech symbols", locale.code()),
        symbols,
        source: Some(SYMBOL_SOURCE.into()),
        license: Some(LICENSE.into()),
        revision: Some(REVISION.into()),
        sha256: None,
    };
    Ok(DerivedPacks {
        language_json: with_digest(&language, locale)?,
        symbol_json: with_digest(&symbols, locale)?,
    })
}
