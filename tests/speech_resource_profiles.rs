use textintel::core::providers::{LanguageDetectionProvider, SymbolKnowledgeProvider};
use textintel::language::NgramLanguageDetector;
use textintel::speech::SpeechLocale;
use textintel::{ResourceLimits, ResourceLoader};

#[test]
fn embedded_resources_cover_the_twenty_minimum_and_keep_existing_languages() {
    let resources = ResourceLoader::embedded().unwrap();
    let languages = resources.languages();
    let symbols = resources.symbol_pack_languages();
    let detector = NgramLanguageDetector::from_resources(&resources);
    for locale in SpeechLocale::ALL {
        assert!(
            languages.iter().any(|lang| lang == locale.code()),
            "{locale:?}"
        );
        assert!(
            symbols.iter().any(|lang| lang == locale.code()),
            "{locale:?}"
        );
        assert!(
            detector
                .capabilities()
                .languages
                .iter()
                .any(|lang| lang == locale.code()),
            "{locale:?}"
        );
        let scope = vec![locale.code().to_owned()];
        assert!(
            resources
                .readings_in_languages("2", 32, Some(&scope))
                .iter()
                .any(|reading| reading.language.as_deref() == Some(locale.code())),
            "{locale:?}"
        );
    }
    for extra in ["it", "nl", "pl"] {
        assert!(languages.iter().any(|lang| lang == extra));
    }
    assert_eq!(resources.symbol_count(), 35);
    for token in resources.symbol_tokens() {
        for locale in SpeechLocale::ALL {
            assert!(
                resources
                    .symbol_languages(&token)
                    .iter()
                    .any(|lang| lang == locale.code()),
                "{token:?} {locale:?}"
            );
        }
    }
    assert!(
        resources
            .concepts("❤")
            .iter()
            .any(|concept| concept.id == "concept:emotion.love")
    );
    assert!(
        resources
            .concepts("🏠")
            .iter()
            .any(|concept| concept.id == "concept:building.house")
    );
}

#[test]
fn generated_domain_seeds_are_bounded_attributed_hashed_and_idempotent() {
    let mut resources = ResourceLoader::embedded().unwrap();
    let derived: Vec<_> = resources
        .manifest()
        .iter()
        .filter(|pack| pack.origin.starts_with("<derived:speech:"))
        .collect();
    assert_eq!(derived.len(), 7); // Physical prose profiles plus seven derived symbol packs.
    for pack in derived {
        assert!(pack.source.as_deref().unwrap().contains("src/speech/data/"));
        assert!(pack.license.as_deref().unwrap().contains("LICENSE"));
        assert_eq!(pack.revision.as_deref(), Some("speech-domain-seed-v2"));
        let hash = pack.sha256.as_ref().unwrap();
        assert_eq!(hash.len(), 64);
        assert!(hash.bytes().all(|ch| ch.is_ascii_hexdigit()));
        if pack.kind == "language" {
            let language = pack.language.as_deref().unwrap();
            let seed = resources.language_pack(language).unwrap();
            assert!(seed.entries.len() <= 1024);
            assert!(
                seed.entries
                    .iter()
                    .all(|entry| entry.word.chars().count() <= 128)
            );
            assert!(seed.examples.is_empty() && seed.stop_words.is_empty());
        }
    }
    let before = resources.manifest().to_vec();
    resources.add_speech_resources().unwrap();
    assert_eq!(resources.manifest(), before);
    let repeated = ResourceLoader::embedded().unwrap();
    assert_eq!(repeated.manifest(), resources.manifest());
}

#[test]
fn custom_resource_scope_expands_only_when_explicitly_requested() {
    let mut custom = ResourceLoader::default();
    custom
        .load_language_json(
            r#"{"language":"es","revision":"custom","words":["hola","mundo"]}"#,
            "<custom-es>",
        )
        .unwrap();
    assert_eq!(custom.languages(), vec!["es"]);
    let directory = ResourceLoader::from_resource_root("resources").unwrap();
    assert_eq!(directory.language_count(), 23);
    custom.add_speech_resources().unwrap();
    assert_eq!(custom.language_count(), 20);
    assert_eq!(custom.language_packs("es").len(), 1);
    assert_eq!(
        custom.language_pack("es").unwrap().revision.as_deref(),
        Some("custom")
    );
}

#[test]
fn resource_limits_fail_without_leaving_partial_derived_data() {
    for limits in [
        ResourceLimits {
            max_resource_bytes: 1,
            ..Default::default()
        },
        ResourceLimits {
            max_pack_entries: 1,
            ..Default::default()
        },
        ResourceLimits {
            max_symbols: 1,
            ..Default::default()
        },
        ResourceLimits {
            max_readings_per_symbol: 0,
            ..Default::default()
        },
    ] {
        let mut resources = ResourceLoader::with_limits(limits);
        assert!(resources.add_speech_resources().is_err());
        assert!(resources.languages().is_empty());
        assert!(resources.manifest().is_empty());
    }
}

#[test]
fn unseen_native_script_challenges_keep_finite_candidates_and_honest_abstention() {
    let resources = ResourceLoader::embedded().unwrap();
    let detector = NgramLanguageDetector::from_resources(&resources).with_max_candidates(64);
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../data/language/native-prose-heldout.json")).unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        let language = case["language"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        assert!(
            text.split_whitespace()
                .any(|word| !resources.contains_in_language(word, language)),
            "challenge must include unseen words"
        );
        let result = detector.detect(text).unwrap();
        eprintln!("unseen {language}: top={:?}", result.first());
        assert!(
            result
                .iter()
                .any(|candidate| candidate.language == language),
            "{language}: {result:?}"
        );
        assert!(
            result
                .iter()
                .all(|candidate| candidate.probability.is_finite()
                    && (0.0..=1.0).contains(&candidate.probability))
        );
        assert!(
            (result
                .iter()
                .map(|candidate| candidate.probability)
                .sum::<f64>()
                - 1.0)
                .abs()
                < 1e-9
        );
        // A domain seed does not justify asserting perfect general-language
        // classification. `unknown` and competing same-script languages stay.
    }
}
