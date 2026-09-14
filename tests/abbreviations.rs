//! Resource-driven chat abbreviations (§7): mappings live in versioned packs,
//! never in core. These tests prove decoding follows the resources.

use textintel::core::providers::AbbreviationProvider;
use textintel::core::types::SymbolReading;
use textintel::engine::TextIntelligence;
use textintel::resources::ResourceLoader;

#[test]
fn embedded_packs_drive_english_expansions() {
    let loader = ResourceLoader::embedded().expect("embedded resources must load");
    let readings = loader.abbreviation_readings("u", None, 8);
    let texts: Vec<&str> = readings.iter().map(|item| item.text.as_str()).collect();
    assert!(texts.contains(&"you"), "missing 'you': {texts:?}");
    assert!(texts.contains(&"tu"), "missing 'tu': {texts:?}");
    assert!(
        readings
            .iter()
            .all(|item| item.reading_type == "chat_abbreviation"),
        "unexpected reading types: {readings:?}"
    );

    let great = loader.abbreviation_readings("gr8", None, 8);
    assert_eq!(great.len(), 1);
    assert_eq!(great[0].text, "great");
    assert_eq!(great[0].language.as_deref(), Some("en"));
}

#[test]
fn abbreviation_lookup_is_case_insensitive_and_language_filtered() {
    let loader = ResourceLoader::embedded().expect("embedded resources must load");
    let upper = loader.abbreviation_readings("U", None, 8);
    assert!(upper.iter().any(|item| item.text == "you"));

    let spanish = loader.abbreviation_readings("u", Some(&["es".to_string()]), 8);
    assert!(!spanish.is_empty());
    assert!(
        spanish
            .iter()
            .all(|item| item.language.as_deref() == Some("es")),
        "language filter leaked: {spanish:?}"
    );

    let que = loader.abbreviation_readings("q", Some(&["es".to_string()]), 8);
    assert!(que.iter().any(|item| item.text == "que"), "{que:?}");
}

/// An abbreviation provider with no entries removes the expansions. If core
/// ever hardcoded `gr8 → great`, this would keep passing with the provider
/// emptied — it must fail instead.
#[derive(Debug, Default)]
struct EmptyAbbreviations;

impl AbbreviationProvider for EmptyAbbreviations {
    fn abbreviation_readings(
        &self,
        _token: &str,
        _languages: Option<&[String]>,
        _max_readings: usize,
    ) -> Vec<SymbolReading> {
        Vec::new()
    }
}

#[test]
fn decoding_follows_the_configured_abbreviation_provider() {
    // "ur" is a pure-letter token, so it reaches the abbreviation branch
    // (alphanumeric tokens like "gr8" take the leet path by design).
    let engine = TextIntelligence::default();
    let decoded = engine.decode("ur").expect("decode must succeed");
    assert!(
        decoded.iter().any(|item| item.text == "your"),
        "embedded packs should decode 'ur' to 'your': {:?}",
        decoded.iter().map(|item| &item.text).collect::<Vec<_>>()
    );

    let without = TextIntelligence::default().with_abbreviation_provider(EmptyAbbreviations);
    let decoded = without.decode("ur").expect("decode must succeed");
    assert!(
        decoded.iter().all(|item| item.text != "your"),
        "empty packs must not decode 'your': {:?}",
        decoded.iter().map(|item| &item.text).collect::<Vec<_>>()
    );
}

#[test]
fn invalid_abbreviation_packs_are_rejected() {
    let mut loader = ResourceLoader::default();
    let bad_schema = r#"{"schema_version": 99, "language": "en", "entries": []}"#;
    assert!(loader
        .load_abbreviation_json(bad_schema, "bad-schema.json")
        .is_err());

    let bad_probability = r#"{"schema_version": 1, "language": "en",
        "entries": [{"token": "u", "readings": [{"text": "you", "probability": 2.0}]}]}"#;
    assert!(loader
        .load_abbreviation_json(bad_probability, "bad-probability.json")
        .is_err());

    let empty_token = r#"{"schema_version": 1, "language": "en",
        "entries": [{"token": "  ", "readings": [{"text": "you"}]}]}"#;
    assert!(loader
        .load_abbreviation_json(empty_token, "empty-token.json")
        .is_err());

    let empty_language = r#"{"schema_version": 1, "language": "  ", "entries": []}"#;
    assert!(loader
        .load_abbreviation_json(empty_language, "empty-language.json")
        .is_err());
}
