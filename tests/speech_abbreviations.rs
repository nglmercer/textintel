use textintel::speech::{
    AbbreviationPolicy, SpeechDocument, SpeechLocale, SpeechOptions, SpeechSpan, SpeechValue,
    abbreviations, normalize_for_speech, parse_for_speech, verbalize_document_with_options,
};

fn id(locale: SpeechLocale, written: &str) -> String {
    abbreviations::entries()
        .iter()
        .find(|entry| entry.locale == locale.code() && entry.written == written)
        .unwrap_or_else(|| panic!("missing {locale:?} {written}"))
        .id
        .clone()
}

#[test]
fn documented_contexts_expand_through_the_same_typed_catalog() {
    // Expected utterances are written independently of parser predictions.
    for (locale, source, expected) in [
        (SpeechLocale::En, "Dr. Smith", "doctor Smith"),
        (SpeechLocale::Es, "Dra. López", "doctora López"),
        (SpeechLocale::Fr, "Dr. Dupont", "docteur Dupont"),
        (SpeechLocale::De, "Nr. 5", "Nummer fünf"),
        (SpeechLocale::Pt, "Dra. Silva", "doutora Silva"),
        (SpeechLocale::Hi, "डॉ. शर्मा", "डॉक्टर शर्मा"),
        (SpeechLocale::Bn, "ডা. রহিম", "ডাক্তার রহিম"),
        (SpeechLocale::Bn, "ড. রহমান", "ডক্টর রহমান"),
        (SpeechLocale::Pa, "ਡਾ: ਸਿੰਘ", "ਡਾਕਟਰ ਸਿੰਘ"),
        (SpeechLocale::Mr, "मा. देशमुख", "माननीय देशमुख"),
        (SpeechLocale::Ta, "உதா. சொல்", "உதாரணம் சொல்"),
        (SpeechLocale::Ru, "г-н Иванов", "господин Иванов"),
        (SpeechLocale::Id, "Dr. Sari", "doktor Sari"),
        (SpeechLocale::Id, "dr. Sari", "dokter Sari"),
        (SpeechLocale::Tr, "Dr. Yılmaz", "Doktor Yılmaz"),
        (SpeechLocale::Vi, "TS Nguyễn", "Tiến sĩ Nguyễn"),
        (SpeechLocale::Ja, "(株) 山田", "株式会社 山田"),
        (SpeechLocale::Ko, "(주) 한빛", "주식회사 한빛"),
    ] {
        let options = SpeechOptions::new(locale);
        let document = parse_for_speech(source, &options).unwrap();
        assert!(
            document
                .spans
                .iter()
                .any(|span| matches!(span.value, SpeechValue::Abbreviation { .. })),
            "{locale:?} {source}: {document:?}"
        );
        let result = verbalize_document_with_options(&document, &options).unwrap();
        assert!(result.fully_supported, "{locale:?} {source}: {result:?}");
        assert_eq!(result.text, expected, "{locale:?} {source}");
        assert_eq!(normalize_for_speech(source, &options).unwrap(), result);
        let mut cursor = 0;
        for span in document.spans {
            assert_eq!(span.start, cursor);
            assert_eq!(source.get(span.start..span.end), Some(span.source.as_str()));
            cursor = span.end;
        }
        assert_eq!(cursor, source.len());
    }
}

#[test]
fn source_offsets_case_and_alias_punctuation_are_semantic() {
    let opts = SpeechOptions::new(SpeechLocale::Id);
    let text = "🙂 Dr. Sari, dr. Rina.";
    let doc = parse_for_speech(text, &opts).unwrap();
    let spans: Vec<_> = doc
        .spans
        .iter()
        .filter(|s| matches!(s.value, SpeechValue::Abbreviation { .. }))
        .collect();
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].start, "🙂 ".len());
    assert_eq!(spans[0].source, "Dr.");
    assert_eq!(spans[1].source, "dr.");
    assert_ne!(spans[0].value, spans[1].value);
    assert_eq!(
        normalize_for_speech(text, &opts).unwrap().text,
        "🙂 doktor Sari, dokter Rina."
    );
    let opts = SpeechOptions::new(SpeechLocale::Pa);
    assert!(
        parse_for_speech("ਡਾ. ਸਿੰਘ", &opts)
            .unwrap()
            .spans
            .iter()
            .all(|s| !matches!(s.value, SpeechValue::Abbreviation { .. }))
    );
}

#[test]
fn bibliographic_reading_requires_explicit_policy_and_does_not_guess_a_title_gender() {
    for (locale, source, expected) in [
        (SpeechLocale::Ar, "كتاب (د.ت.)", "كتاب (دون تاريخ النشر)"),
        (SpeechLocale::Ar, "كتاب (د.م.)", "كتاب (دون مكان النشر)"),
        (SpeechLocale::Ur, "ص 12", "صفحہ نمبر بارہ"),
    ] {
        let mut opts = SpeechOptions::new(locale);
        let doc = parse_for_speech(source, &opts).unwrap();
        assert!(
            doc.spans
                .iter()
                .all(|s| !matches!(s.value, SpeechValue::Abbreviation { .. }))
        );
        opts.abbreviation_policy = AbbreviationPolicy::Bibliographic;
        let result = normalize_for_speech(source, &opts).unwrap();
        assert!(result.fully_supported, "{source}: {result:?}");
        assert_eq!(result.text, expected);
    }
    let opts = SpeechOptions::new(SpeechLocale::Ar);
    for source in ["د. علي", "د. فاطمة", "أ.د. سليم"] {
        let result = normalize_for_speech(source, &opts).unwrap();
        assert_eq!(result.text, source);
    }
}

#[test]
fn no_partial_expansion_in_protected_fields_words_or_unknown_inflections() {
    let opts = SpeechOptions::new(SpeechLocale::En);
    for source in [
        "`Dr. Smith`",
        "<x title='Dr. Smith'>",
        "https://example.org/Dr.Smith",
        "www.example.org/Dr.Smith",
        "Dr.@example.org",
        "/tmp/Dr.Smith.txt",
        "Dr.txt",
        "xDr. Smith",
        "Dr.3 Smith",
        "Dr. Smith_12",
        "@Dr. Smith",
        "Dr.\nSmith",
        "Dr. smith",
        "Dr.",
        "{\"title\":\"Dr. Smith\"}",
    ] {
        let doc = parse_for_speech(source, &opts).unwrap();
        assert!(
            doc.spans
                .iter()
                .all(|s| !matches!(s.value, SpeechValue::Abbreviation { .. })),
            "{source}: {doc:?}"
        );
    }
    let opts = SpeechOptions::new(SpeechLocale::Ru);
    assert!(
        parse_for_speech("г-ну Иванову", &opts)
            .unwrap()
            .spans
            .iter()
            .all(|s| !matches!(s.value, SpeechValue::Abbreviation { .. }))
    );
}

#[test]
fn all_twenty_locales_have_explicit_policy_routes_and_honest_inventory_scope() {
    for locale in SpeechLocale::ALL {
        for policy in [
            AbbreviationPolicy::Preserve,
            AbbreviationPolicy::Contextual,
            AbbreviationPolicy::Bibliographic,
        ] {
            let mut opts = SpeechOptions::new(locale);
            opts.abbreviation_policy = policy;
            let doc = parse_for_speech("42", &opts).unwrap();
            assert!(
                matches!(&doc.spans[0].value, SpeechValue::Number { value, .. } if value.integer == "42")
            );
        }
    }
    for locale in [SpeechLocale::Zh, SpeechLocale::Te] {
        assert_eq!(abbreviations::declared_coverage(locale), 0);
        assert!(
            normalize_for_speech("12 kg", &SpeechOptions::new(locale))
                .unwrap()
                .fully_supported
        );
    }
    let mut opts = SpeechOptions::new(SpeechLocale::En);
    opts.abbreviation_policy = AbbreviationPolicy::Preserve;
    assert_eq!(
        normalize_for_speech("Dr. Smith", &opts).unwrap().text,
        "Dr. Smith"
    );
}

#[test]
fn public_abbreviation_ir_is_bound_to_catalog_identity_locale_source_and_policy() {
    let make = |locale, source: &str, end: usize, id| {
        let mut spans = vec![SpeechSpan {
            start: 0,
            end,
            source: source[..end].into(),
            value: SpeechValue::Abbreviation { id },
        }];
        if end < source.len() {
            spans.push(SpeechSpan {
                start: end,
                end: source.len(),
                source: source[end..].into(),
                value: SpeechValue::Text,
            });
        }
        SpeechDocument {
            text: source.into(),
            locale,
            spans,
        }
    };
    let opts = SpeechOptions::new(SpeechLocale::En);
    let doctor = id(SpeechLocale::En, "Dr.");
    let doc = make(SpeechLocale::En, "Dr. Smith", 3, doctor.clone());
    assert_eq!(
        verbalize_document_with_options(&doc, &opts).unwrap().text,
        "doctor Smith"
    );
    for invalid in [
        make(SpeechLocale::En, "Mr. Smith", 3, doctor.clone()),
        make(SpeechLocale::En, "Dr. Smith", 3, "unknown-id".into()),
        make(SpeechLocale::En, "Dr.", 3, doctor.clone()),
    ] {
        let result = verbalize_document_with_options(&invalid, &opts).unwrap();
        assert!(!result.fully_supported);
        assert!(result.readings[0].reason.is_some());
        assert_eq!(result.text, invalid.text);
    }
    let mut opts = opts;
    opts.abbreviation_policy = AbbreviationPolicy::Preserve;
    assert!(
        !verbalize_document_with_options(&doc, &opts)
            .unwrap()
            .fully_supported
    );
    let opts = SpeechOptions::new(SpeechLocale::De);
    assert!(
        !verbalize_document_with_options(&make(SpeechLocale::De, "Dr. Smith", 3, doctor), &opts)
            .unwrap()
            .fully_supported
    );
    let doc = make(
        SpeechLocale::Ar,
        "د.ت.",
        "د.ت.".len(),
        id(SpeechLocale::Ar, "د.ت."),
    );
    let mut opts = SpeechOptions::new(SpeechLocale::Ar);
    assert!(
        !verbalize_document_with_options(&doc, &opts)
            .unwrap()
            .fully_supported
    );
    opts.abbreviation_policy = AbbreviationPolicy::Bibliographic;
    assert!(
        verbalize_document_with_options(&doc, &opts)
            .unwrap()
            .fully_supported
    );
}
