use std::collections::BTreeMap;
use std::path::PathBuf;

use textintel::lexical::character::{character_similarity, levenshtein};
use textintel::lexical::similarity::lexical_similarity;
use textintel::lexical::tokenizer::{stop_words, tokenize};
use textintel::normalization::leetspeak::detect_leet;
use textintel::visual::unicode_features::analyze_unicode;
use textintel::{
    EngineConfig, LookupStatus, ResourceLoader, SimilarityWeights, StaticEmbeddingProvider,
    TextIntelError, TextIntelligence,
};

fn engine() -> TextIntelligence {
    TextIntelligence::default()
}

#[test]
fn character_metrics_are_independent_and_bounded() {
    let score = character_similarity("compra", "compra");
    assert_eq!(score.combined, 1.0);
    assert_eq!(score.levenshtein, 1.0);
    assert!(character_similarity("comprar", "c0mpr4r").combined > 0.4);
    assert_eq!(levenshtein("kitten", "sitting"), 3);
    assert_eq!(levenshtein("", "ab"), 2);
}

#[test]
fn lexical_tokenization_and_similarity_work_without_models() {
    let tokens = tokenize("bro compra NOW");
    assert!(tokens.iter().any(|token| token == "bro"));
    assert!(tokens.iter().any(|token| token == "compra"));
    assert!(tokens.iter().any(|token| token == "NOW"));
    assert_eq!(lexical_similarity("gana dinero", "gana dinero"), 1.0);
    assert!(lexical_similarity("gana dinero", "xyz") < 0.2);
}

#[test]
fn unicode_confusable_and_leet_features_are_exposed() {
    let mixed = "pаypal"; // Cyrillic а
    let features = analyze_unicode(mixed);
    assert!(features.mixed_scripts || !features.confusable_characters.is_empty());
    assert_eq!(features.confusable_skeleton.as_deref(), Some("paypal"));
    assert!(detect_leet("c0mpr4 ah0r4"));
    let fp = engine().analyze("c0mpr4 ah0r4").unwrap();
    assert!(fp.obfuscation_features.leetspeak);
    assert!(fp.obfuscation_features.detected);

    let unusual = analyze_unicode("hola\u{200b}\u{00a0}mundo");
    assert!(!unusual.unusual_whitespace.is_empty());
    assert!(!unusual.invisible_characters.is_empty());
}

#[test]
fn mvp_examples_preserve_raw_and_decode_rebus_candidates() {
    let e = engine();
    let raw = "Fr4🏠do!!!";
    assert_eq!(e.analyze(raw).unwrap().raw, raw);

    let case_result = e.compare("COMPRA AHORA", "compra ahora").unwrap();
    assert!(case_result.character.unwrap() > 0.7);
    assert!(case_result.semantic.is_none());
    assert!(case_result.phonetic.is_none());

    let leet = e.compare("c0mpr4 ah0r4", "compra ahora").unwrap();
    assert!(leet.decoded_similarity.unwrap() > 0.5);
    assert!(leet.score > 0.5);

    let decoded = e.decode("Fra🏠do").unwrap();
    assert!(decoded
        .iter()
        .any(|candidate| candidate.text.eq_ignore_ascii_case("fracasado")));
    assert!(e.compare("Fra🏠do", "fracasado").unwrap().score > 0.45);

    let numeric = e.decode("salU2").unwrap();
    assert!(numeric
        .iter()
        .any(|candidate| candidate.text.eq_ignore_ascii_case("saludos")));

    let fire = e.analyze("🔥").unwrap();
    assert!(
        fire.symbols
            .iter()
            .flat_map(|symbol| symbol.readings.iter())
            .count()
            > 1
    );
    assert!(e.decode("🔥").unwrap().len() > 1);
}

#[test]
fn visual_repetition_language_switch_and_negative_case() {
    let e = engine();
    let homoglyph = e.analyze("pаypal").unwrap();
    assert!(
        homoglyph.unicode_features.mixed_scripts
            || !homoglyph.unicode_features.confusable_characters.is_empty()
    );
    assert!(
        homoglyph.obfuscation_features.confusables || homoglyph.obfuscation_features.mixed_scripts
    );
    assert!(e.compare("paypal", "pаypal").unwrap().visual.unwrap() > 0.7);

    let repeated = e.analyze("GAAAAANAAAA DINEROOOO").unwrap();
    assert!(repeated.obfuscation_features.repetition);
    assert!(
        e.compare("GAAAAANAAAA DINEROOOO", "gana dinero")
            .unwrap()
            .score
            > 0.4
    );

    let switched = e.analyze("bro compra NOW").unwrap();
    assert!(
        switched.language_candidates.len() >= 2
            || switched
                .segments
                .iter()
                .any(|segment| segment.language_candidates.len() > 1)
    );
    assert!(
        e.compare("bro compra NOW", "bro compra ahora")
            .unwrap()
            .score
            > 0.3
    );

    let positive = e.compare("Fra🏠do", "fracasado").unwrap().score;
    let negative = e.compare("Fra🏠do", "ferrocarril").unwrap().score;
    assert!(positive > negative + 0.12);
    assert!(negative < 0.7);
}

#[test]
fn configured_weights_and_limits_are_honored() {
    let weights = SimilarityWeights {
        character: 1.0,
        lexical: 0.0,
        visual: 0.0,
        decoded: 0.0,
        obfuscation: 0.0,
        semantic: 0.0,
        phonetic: 0.0,
        symbolic: 0.0,
    };
    let config = EngineConfig {
        similarity_weights: weights,
        max_input_length: 3,
        ..EngineConfig::default()
    };
    let e = TextIntelligence::new(config);
    let result = e.compare("abc", "abd").unwrap();
    assert_eq!(result.weights_used.get("character"), Some(&1.0));
    assert_eq!(result.score, result.character.unwrap());
    assert!(matches!(
        e.analyze("abcd"),
        Err(TextIntelError::InputTooLong { .. })
    ));
}

#[test]
fn semantic_and_phonetic_providers_are_optional_channels() {
    let mut values = BTreeMap::new();
    values.insert("same".to_string(), vec![1.0, 0.0]);
    let mut config = EngineConfig {
        semantic: true,
        phonetic: true,
        ..EngineConfig::default()
    };
    config.similarity_weights.semantic = 0.5;
    config.similarity_weights.phonetic = 0.2;
    let e =
        TextIntelligence::new(config).with_embedding_provider(StaticEmbeddingProvider::new(values));
    let same = e.analyze("same").unwrap();
    assert!(same.semantic_embeddings.contains_key("default"));
    assert!(!same.phonetic_candidates.is_empty());
    let comparison = e.compare("same", "same").unwrap();
    assert_eq!(comparison.semantic, Some(1.0));
    assert!(comparison.phonetic.unwrap_or(0.0) > 0.0);
}

#[test]
fn patterns_spam_duplicates_and_search_use_fingerprints() {
    let e = engine();
    e.add_pattern(
        "scam_prize",
        vec![
            "ganaste un premio".to_string(),
            "reclama tu premio".to_string(),
        ],
    )
    .unwrap();
    let matches = e.match_patterns("ganaste un premio").unwrap();
    assert_eq!(
        matches.first().map(|item| item.id.as_str()),
        Some("scam_prize")
    );
    assert!(matches[0].score > 0.7);
    let spam = e
        .detect_spam("ganaste un premio https://example.test")
        .unwrap();
    assert!(!spam.labels.is_empty());

    let duplicate = e.duplicate("COMPRA AHORA", "compra ahora", 0.8).unwrap();
    assert!(duplicate.duplicate);

    e.add_document("one", "gana dinero").unwrap();
    e.add_document("two", "ferrocarril").unwrap();
    let results = e.find_similar("gana 💰", 2).unwrap();
    assert_eq!(results.first().map(|item| item.id.as_str()), Some("one"));
    assert_eq!(e.document_count().unwrap(), 2);
    assert!(e.remove_document("two").unwrap());
}

#[test]
fn urls_and_stop_words_keep_their_own_evidence() {
    let tokens = tokenize("the and https://example.test");
    assert!(tokens.iter().any(|token| token == "https://example.test"));
    assert_eq!(stop_words(&tokens), vec!["the", "and"]);
    let segments = engine()
        .analyze("the and https://example.test")
        .unwrap()
        .segments;
    assert!(segments.iter().any(|segment| segment.segment_type == "url"));
}

#[test]
fn resource_loader_indexes_seed_languages_and_supports_custom_packs() {
    let loader = ResourceLoader::common().unwrap();
    assert_eq!(loader.languages(), vec!["de", "en", "es", "fr", "it", "pt"]);
    assert!(loader.language_count() == 6);
    assert!(loader.word_count() > 100);
    assert!(loader.contains_in_language("A", "en"));
    assert!(loader.contains_in_language("ejemplo", "es"));
    assert!(loader
        .lookup_languages("example")
        .contains(&"en".to_string()));
    let resource_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
    let from_disk = ResourceLoader::from_resource_root(resource_root).unwrap();
    assert_eq!(from_disk.languages(), loader.languages());
    assert_eq!(from_disk.symbol_count(), loader.symbol_count());

    let fingerprint = TextIntelligence::default()
        .with_resources(loader.clone())
        .analyze("this is a example")
        .unwrap();
    assert_eq!(fingerprint.top_language(), Some("en"));
    assert!(fingerprint
        .lexical_features
        .stop_words
        .iter()
        .any(|word| word == "this"));

    let mut custom = ResourceLoader::default();
    custom
        .load_language_json(
            r#"{
                "schema_version": 1,
                "language": "xx",
                "name": "Example",
                "entries": [{"word": "zyx", "lemma": "zyx"}],
                "examples": ["zyx sample"]
            }"#,
            "<test:xx.json>",
        )
        .unwrap();
    assert!(custom.contains_in_language("zyx", "xx"));
    assert_eq!(
        custom.detect_languages("zyx"),
        vec![textintel::core::types::LanguageCandidate::new("xx", 1.0)]
    );
}

#[test]
fn symbol_resources_are_split_by_language() {
    let loader = ResourceLoader::common().unwrap();
    let expected_languages = ["de", "en", "es", "fr", "it", "pt"];
    let expected_tokens = vec![
        "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "100", "❤", "❤️", "🏠", "💰", "🔥",
    ];

    assert_eq!(loader.symbol_count(), 17);
    assert_eq!(loader.symbol_tokens(), expected_tokens);
    for token in &expected_tokens {
        let languages = loader.symbol_languages(token);
        assert!(
            expected_languages
                .iter()
                .all(|language| languages.iter().any(|value| value == language)),
            "token={token} languages={languages:?}"
        );
    }
    assert_eq!(
        loader.symbol_languages("🔥").last().map(String::as_str),
        Some("und")
    );
}

#[test]
fn symbol_pack_languages_are_normalized_and_scoped() {
    let mut loader = ResourceLoader::default();
    loader
        .load_symbol_json(
            r#"{
                "schema_version": 1,
                "language": "PT_BR",
                "symbols": [{
                    "token": "☂",
                    "readings": [{
                        "text": "guarda-chuva",
                        "probability": 0.8,
                        "reading_type": "symbol_reading"
                    }]
                }]
            }"#,
            "<test:pt_br-symbols.json>",
        )
        .unwrap();
    assert_eq!(loader.symbol_languages("☂"), vec!["pt-br"]);

    let error = loader
        .load_symbol_json(
            r#"{
                "schema_version": 1,
                "language": "es",
                "symbols": [{
                    "token": "☀",
                    "readings": [{
                        "text": "sun",
                        "language": "EN",
                        "probability": 0.8,
                        "reading_type": "symbol_reading"
                    }]
                }]
            }"#,
            "<test:conflicting-symbols.json>",
        )
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("conflicts with symbol pack language"));
}

#[test]
fn resource_index_is_stable_and_tracks_ambiguous_terms() {
    let mut loader = ResourceLoader::default();
    loader
        .load_language_json(
            r#"{
                "language": "xx",
                "words": ["10", "2", "a", "á", "z"],
                "stop_words": ["a"]
            }"#,
            "<test:xx/00-09.json>",
        )
        .unwrap();
    loader
        .load_language_json(
            r#"{
                "language": "yy",
                "entries": [{"word": "a", "lemma": "a"}]
            }"#,
            "<test:yy/a.json>",
        )
        .unwrap();

    let keys = loader.index_keys();
    assert_eq!(keys, vec!["2", "10", "a", "z", "á"]);
    assert_eq!(loader.lookup("z").status, LookupStatus::Unique);
    assert_eq!(loader.lookup("missing").status, LookupStatus::NotFound);
    let ambiguous = loader.lookup("a");
    assert_eq!(ambiguous.status, LookupStatus::Ambiguous);
    assert!(ambiguous
        .matches
        .iter()
        .any(|record| record.language == "xx"));
    assert!(ambiguous
        .matches
        .iter()
        .any(|record| record.language == "yy"));
    assert!(ambiguous
        .matches
        .iter()
        .all(|record| record.source.starts_with("<test:")));
    assert_eq!(
        loader.lookup_in_language("a", "xx").status,
        LookupStatus::Unique
    );
    assert_eq!(
        loader.lookup_in_language("a", "zz").status,
        LookupStatus::NotFound
    );
}
