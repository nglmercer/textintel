use textintel::RuleBasedEntityProvider;
use textintel::core::providers::EntityProvider;
use textintel::core::unicode::ascii_decimal_digit;
use textintel::speech::SpeechLocale;

fn localized(text: &str, digits: &str) -> String {
    let alphabet: Vec<char> = digits.chars().collect();
    text.chars()
        .map(|ch| ch.to_digit(10).map_or(ch, |index| alphabet[index as usize]))
        .collect()
}

#[test]
fn speech_and_entity_digit_values_share_twenty_language_source_offsets() {
    let samples = [
        (SpeechLocale::En, "0123456789"),
        (SpeechLocale::Es, "0123456789"),
        (SpeechLocale::Zh, "０１２３４５６７８９"),
        (SpeechLocale::Hi, "०१२३४५६७८९"),
        (SpeechLocale::Ar, "٠١٢٣٤٥٦٧٨٩"),
        (SpeechLocale::Fr, "0123456789"),
        (SpeechLocale::Pt, "0123456789"),
        (SpeechLocale::Ru, "0123456789"),
        (SpeechLocale::Bn, "০১২৩৪৫৬৭৮৯"),
        (SpeechLocale::Ur, "۰۱۲۳۴۵۶۷۸۹"),
        (SpeechLocale::Id, "0123456789"),
        (SpeechLocale::De, "0123456789"),
        (SpeechLocale::Ja, "０１２３４５６７８９"),
        (SpeechLocale::Pa, "੦੧੨੩੪੫੬੭੮੯"),
        (SpeechLocale::Mr, "०१२३४५६७८९"),
        (SpeechLocale::Te, "౦౧౨౩౪౫౬౭౮౯"),
        (SpeechLocale::Tr, "0123456789"),
        (SpeechLocale::Ta, "௦௧௨௩௪௫௬௭௮௯"),
        (SpeechLocale::Vi, "0123456789"),
        (SpeechLocale::Ko, "０１２３４５６７８９"),
    ];
    assert_eq!(samples.map(|sample| sample.0), SpeechLocale::ALL);
    let provider = RuleBasedEntityProvider::default();
    for (locale, digits) in samples {
        for (source, entity_type, normalized) in [
            ("12345.67", "number", "12345.67"),
            ("€12.50", "currency", "EUR 12.50"),
            ("12.50 EUR", "currency", "EUR 12.50"),
            ("2024-02-29", "date", "2024-02-29"),
            ("29/02/2024", "date", "29/02/2024"),
            ("23:59:58", "time", "23:59:58"),
        ] {
            let source = localized(source, digits);
            let text = format!("📌 ({source}) ✓");
            let found = provider.extract(&text, Some(locale.code()));
            let matches: Vec<_> = found
                .iter()
                .filter(|e| e.entity_type == entity_type)
                .collect();
            assert_eq!(matches.len(), 1, "{locale:?}: {text}: {found:?}");
            let mention = matches[0];
            assert_eq!(mention.value, normalized, "{locale:?}: {text}");
            assert_eq!(&text[mention.start..mention.end], source);
            assert_eq!(mention.language.as_deref(), Some(locale.code()));
            assert!(found.windows(2).all(|pair| pair[0].end <= pair[1].start));
        }
    }
}

#[test]
fn native_entity_digits_retain_calendar_boundary_and_claim_policies() {
    let provider = RuleBasedEntityProvider::default();
    for source in [
        "२०२३-०२-२९",
        "२०२४-०४-३१",
        "२४:००",
        "१२३:४५",
        "शक्ति२०२४-०२-२९",
        "२०२४-०२-२९தமிழ்",
        "२९/०२/२०२४-९९",
    ] {
        let found = provider.extract(source, None);
        assert!(
            !found
                .iter()
                .any(|e| matches!(e.entity_type.as_str(), "date" | "time")),
            "{source}: {found:?}"
        );
    }
    for source in ["शक्ति१२३", "१२३தமிழ்", "a१२३", "१२३\u{301}"]
    {
        assert!(
            !provider
                .extract(source, None)
                .iter()
                .any(|e| e.entity_type == "number"),
            "{source}"
        );
    }
    let source = "https://example.org/२०२४-०२-२९";
    let found = provider.extract(source, None);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].entity_type, "url");
    assert_eq!(&source[found[0].start..found[0].end], source);
    for source in ["१२٫५", "۱۲٬۵۰۰"] {
        let found = provider.extract(source, None);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].entity_type, "number");
        assert_eq!(
            found[0].value,
            if source.contains('٫') {
                "12.5"
            } else {
                "12,500"
            }
        );
    }
    let text = "截止２０２４-０２-２９下午２３:５９";
    let found = provider.extract(text, Some("zh"));
    for (kind, value, source) in [
        ("date", "2024-02-29", "２０２４-０２-２９"),
        ("time", "23:59", "２３:５９"),
    ] {
        let entity = found.iter().find(|e| e.entity_type == kind).unwrap();
        assert_eq!(entity.value, value);
        assert_eq!(&text[entity.start..entity.end], source);
    }
}

#[test]
fn digit_conversion_does_not_promote_non_decimal_number_symbols() {
    for ch in ['²', 'Ⅷ', '①', '一', '½', 'a', '\u{301}'] {
        assert_eq!(ascii_decimal_digit(ch), None, "{ch}");
    }
    for (source, expected) in [('۰', '0'), ('৯', '9'), ('੫', '5'), ('௭', '7'), ('𝟠', '8')]
    {
        assert_eq!(ascii_decimal_digit(source), Some(expected));
    }
}
