use textintel::core::providers::G2PProvider;
use textintel::phonetic::g2p::RuleBasedG2PProvider;
use textintel::speech::SpeechLocale;

#[test]
fn unsupported_latin_language_does_not_inherit_english_rule_confidence() {
    let provider = RuleBasedG2PProvider;
    let english = provider.phonemize("hello", "en").unwrap();
    let unsupported = provider.phonemize("hello", "vi").unwrap();
    // The fallback remains available, while the language claim is bounded by
    // the actual es/en rule implementation rather than the script alone.
    assert_eq!(unsupported.phonemes, english.phonemes);
    assert!(english.confidence > 0.15);
    assert!(unsupported.confidence <= 0.15);
    assert_eq!(unsupported.language, "vi");
    assert_eq!(provider.capabilities().languages, vec!["es", "en"]);
}

#[test]
fn all_twenty_callers_receive_honest_and_consistent_g2p_diagnostics() {
    let provider = RuleBasedG2PProvider;
    for locale in SpeechLocale::ALL {
        let full = provider.phonemize("casa", locale.code()).unwrap();
        let (phonemes, confidence) = provider.phonemes("casa", locale.code()).unwrap();
        assert_eq!(phonemes, full.phonemes, "{locale:?}");
        assert_eq!(confidence, full.confidence, "{locale:?}");
        assert!(confidence.is_finite());
        if !matches!(locale, SpeechLocale::En | SpeechLocale::Es) {
            assert!(confidence <= 0.15, "unsupported rules for {locale:?}");
        }
    }
}

#[test]
fn regional_spanish_uses_spanish_rules_without_claiming_a_dialect_model() {
    let provider = RuleBasedG2PProvider;
    let base = provider.phonemize("calle y niño", "es").unwrap();
    let regional = provider.phonemize("calle y niño", "ES-MX").unwrap();
    assert_eq!(base.phonemes, ["k", "a", "ʝ", "e", "i", "n", "i", "ɲ", "o"]);
    assert_eq!(regional.phonemes, base.phonemes);
    assert_eq!(regional.confidence, base.confidence);
    assert_eq!(regional.language, "ES-MX");
    assert!(regional.dialect.is_none());
    assert_eq!(
        provider.phonemes("calle y niño", "es_MX").unwrap().0,
        base.phonemes
    );
}

#[test]
fn rule_families_accept_case_and_tag_separators_without_widening_coverage() {
    let provider = RuleBasedG2PProvider;
    for (base, tags) in [
        ("es", ["es-MX", "ES_mx", " es-419 "]),
        ("en", ["en-GB", "EN_us", " en-Latn-US "]),
    ] {
        let expected = provider.phonemize("calle y niño", base).unwrap();
        for tag in tags {
            let candidate = provider.phonemize("calle y niño", tag).unwrap();
            assert_eq!(candidate.phonemes, expected.phonemes, "{tag}");
            assert_eq!(candidate.confidence, expected.confidence, "{tag}");
            assert_eq!(candidate.language, tag);
            assert!(candidate.dialect.is_none());
            let lean = provider.phonemes("calle y niño", tag).unwrap();
            assert_eq!(lean, (candidate.phonemes, candidate.confidence));
        }
    }
    for tag in ["xx-es", "pt-BR", "hi-IN", "zh-Hans", "_es", ""] {
        let candidate = provider.phonemize("calle", tag).unwrap();
        assert!(candidate.confidence <= 0.15, "{tag}");
        assert_eq!(candidate.language, tag);
    }
}

#[test]
fn no_phonemes_and_mixed_scripts_keep_existing_low_confidence_contracts() {
    let provider = RuleBasedG2PProvider;
    assert_eq!(provider.phonemize("🎙️", "en").unwrap().confidence, 0.0);
    assert!(provider.phonemize("hello привет", "en").unwrap().confidence < 0.65);
    assert!(provider.phonemize("hello", "zz").unwrap().confidence <= 0.15);
}
