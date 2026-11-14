use textintel::speech::evaluation::{
    SpeechCorpus, SpeechEvaluationPolicy, evaluate_speech_with_policy,
};
use textintel::speech::{self, DateOrder, SpeechLocale, SpeechOptions, SpeechValue};

#[test]
fn every_typed_language_is_registered_and_available_to_both_entry_points() {
    let declared = speech::normalization::language_coverage();
    for language in SpeechLocale::ALL {
        let profile = speech::locales::for_language(language.code()).unwrap();
        assert_eq!(
            profile.default_locale.parse::<SpeechLocale>().unwrap(),
            language
        );
        let coverage = declared
            .iter()
            .find(|row| row.language == language.code())
            .unwrap();
        assert!(
            coverage
                .features
                .values()
                .any(|level| *level != speech::normalization::CoverageLevel::None)
        );
        let options = speech::normalization::NormalizeOptions::resolve(
            speech::normalization::NormalizationMode::Auto,
            language.code(),
            Some(profile.default_locale),
            Default::default(),
            true,
        )
        .unwrap();
        for source in ["12", "2024-12-31", "15:30", "3 kg", "3%", "3 USD"] {
            let result = speech::normalization::normalize(source, &options).unwrap();
            assert_ne!(
                result.spoken,
                source,
                "{}: {source}, {:?}",
                language.code(),
                result.warnings
            );
            assert!(!result.changes.is_empty());
        }
    }
}

#[test]
fn code_tags_and_json_use_the_shared_protection_policy() {
    for language in SpeechLocale::ALL {
        let options = SpeechOptions::new(language);
        for source in [
            "`42 kg and 12`",
            "```json\n{\"value\":42}\n```",
            "<span data-value=\"42 kg\">",
            "{\"value\":42}",
            "[1,2,3]",
        ] {
            let document = speech::parse_for_speech(source, &options).unwrap();
            assert!(
                document.spans.iter().all(|span| matches!(
                    span.value,
                    SpeechValue::Identifier { .. } | SpeechValue::Text
                )),
                "{language:?}: {document:?}"
            );
            let result = speech::normalize_for_speech(source, &options).unwrap();
            assert_eq!(result.text, source);
        }
    }
}

#[test]
fn dictionary_matches_combining_scripts_and_respects_protected_regions() {
    let dictionary =
        speech::dictionary::Dictionary::new(&[("हिन्दी", "हिंदी"), ("word", "spoken")]).unwrap();
    let source = "हिन्दी word `word` <word data=\"word\">";
    let matches = dictionary.matches(source);
    assert_eq!(matches.len(), 2);
    assert_eq!(&source[matches[0].0..matches[0].1], "हिन्दी");
    assert_eq!(&source[matches[1].0..matches[1].1], "word");
}

#[test]
fn contradictory_gold_is_visible_and_remains_in_the_denominator() {
    let mut corpus = SpeechCorpus::from_file("data/speech/reference-nemo.json").unwrap();
    corpus
        .cases
        .retain(|c| c.locale == SpeechLocale::Ru && c.text == "13:05");
    assert_eq!(corpus.cases.len(), 2);
    let report = evaluate_speech_with_policy(&corpus, &SpeechEvaluationPolicy::default()).unwrap();
    assert_eq!(report.totals.cases, 2);
    assert_eq!(report.totals.spoken_cases, 2);
    assert_eq!(report.reference_conflicts.len(), 1);
    assert_eq!(report.exact_spoken_ceiling, 1);
    assert!(report.totals.spoken_exact <= report.exact_spoken_ceiling);
}

#[test]
fn shared_calendar_keeps_century_rules_and_front_end_year_policies() {
    use textintel::core::calendar::valid_date;
    assert!(valid_date(2000, 2, 29));
    assert!(!valid_date(1900, 2, 29));
    assert!(!valid_date(2100, 2, 29));
    assert!(!valid_date(0, 1, 1));
    assert!(!valid_date(2024, 4, 31));
    for language in SpeechLocale::ALL {
        let mut options = SpeechOptions::new(language);
        options.date_order = DateOrder::DayMonthYear;
        let document = speech::parse_for_speech("29/02/2023", &options).unwrap();
        assert!(
            document
                .spans
                .iter()
                .all(|span| !matches!(span.value, SpeechValue::Date { .. }))
        );
        let result = speech::normalize_for_speech(&document.text, &options).unwrap();
        assert!(!result.fully_supported);
        assert_eq!(result.text, "29/02/2023");
    }
}

#[test]
fn shared_dictionary_supports_all_scripts_multiword_entries_and_unspaced_cjk() {
    use speech::dictionary::Dictionary;
    for key in [
        "hello",
        "hola",
        "你好",
        "नमस्ते",
        "مرحبا",
        "bonjour",
        "olá",
        "привет",
        "হ্যালো",
        "سلام",
        "halo",
        "hallo",
        "こんにちは",
        "ਸਤ ਸ੍ਰੀ ਅਕਾਲ",
        "नमस्कार",
        "హలో",
        "merhaba",
        "வணக்கம்",
        "xin chào",
        "안녕하세요",
    ] {
        let dictionary = Dictionary::new(&[(key, "spoken")]).unwrap();
        let input = format!("({key}) `{key}`");
        let matches = dictionary.matches(&input);
        assert_eq!(matches.len(), 1, "{key}");
        assert_eq!(&input[matches[0].0..matches[0].1], key);
    }
    let dictionary = Dictionary::new(&[
        ("北京", "city"),
        ("北京市", "capital"),
        ("New York", "city"),
    ])
    .unwrap();
    let source = "我在北京市工作，New York!";
    let matches = dictionary.matches(source);
    assert_eq!(matches.len(), 2);
    assert_eq!(&source[matches[0].0..matches[0].1], "北京市");
    assert_eq!(matches[0].2, "capital");
    assert_eq!(&source[matches[1].0..matches[1].1], "New York");
}
