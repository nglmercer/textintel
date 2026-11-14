//! Independent semantic review regressions over the public speech APIs.
use textintel::speech::expressions::ExpressionValue;
use textintel::speech::{
    NumberNotation, NumberValue, ReadingStyle, SpeechInterpretation, SpeechLocale, SpeechOptions,
    SpeechValue, normalize_for_speech, parse_for_speech,
};

fn options(locale: SpeechLocale, interpretation: SpeechInterpretation) -> SpeechOptions {
    let mut options = SpeechOptions::new(locale);
    options.reading_style = ReadingStyle::Natural;
    options.number_notation = NumberNotation::Orthographic;
    options.interpret_as = interpretation;
    options.decimal_separator = Some('.');
    options
}

#[test]
fn numeric_paths_are_protected_before_date_and_quantity_grammars() {
    for locale in SpeechLocale::ALL {
        let opts = options(locale, SpeechInterpretation::Auto);
        for source in [
            "/2024-03-12",
            "/2024/03/12.csv",
            "./3kg",
            "~/2024-03-12",
            "C:\\2024-03-12",
            "C:/2024/03/12",
            "2024/03/12.csv",
        ] {
            let result = normalize_for_speech(source, &opts).unwrap();
            assert_eq!(result.text, source, "{locale:?}: {result:?}");
            assert!(!result.fully_supported, "{locale:?}: {result:?}");
            let document = parse_for_speech(source, &opts).unwrap();
            assert_eq!(document.spans.len(), 1, "{locale:?}: {document:?}");
            assert_eq!(document.spans[0].source, source);
            assert!(matches!(
                document.spans[0].value,
                SpeechValue::Identifier { .. }
            ));
        }
        for source in ["1/2", ".50 kg", "S/ 12.50"] {
            let document = parse_for_speech(source, &opts).unwrap();
            assert!(
                !document.spans.iter().any(
                    |span| matches!(&span.value, SpeechValue::Identifier { kind } if kind == "path")
                ),
                "{locale:?}: {document:?}"
            );
        }
    }
}

#[test]
fn unicode_arabic_separator_meanings_cannot_be_reinterpreted_by_locale_options() {
    for locale in SpeechLocale::ALL {
        let mut opts = options(locale, SpeechInterpretation::Number);
        for decimal in [None, Some('.'), Some('٫')] {
            opts.decimal_separator = decimal;
            let document = parse_for_speech("١٬٢٣٤", &opts).unwrap();
            assert_eq!(document.spans.len(), 1);
            assert!(
                matches!(&document.spans[0].value, SpeechValue::Number { value, .. } if value.integer == "1234" && value.fraction.is_none()),
                "{locale:?}: {document:?}"
            );
            let document = parse_for_speech("١٫٢٣٤", &opts).unwrap();
            assert!(
                matches!(&document.spans[0].value, SpeechValue::Number { value, .. } if value.integer == "1" && value.fraction.as_deref() == Some("234")),
                "{locale:?}: {document:?}"
            );
        }
        opts.decimal_separator = Some(',');
        for source in ["١٬٢٣٤", "١٫٢٣٤"] {
            let document = parse_for_speech(source, &opts).unwrap();
            assert_eq!(document.spans.len(), 1);
            assert!(matches!(
                document.spans[0].value,
                SpeechValue::Ambiguous { .. }
            ));
            let result = normalize_for_speech(source, &opts).unwrap();
            assert_eq!(result.text, source);
            assert!(!result.fully_supported);
        }
        opts.interpret_as = SpeechInterpretation::Currency;
        let source = "USD ١٬٢٣٤";
        let result = normalize_for_speech(source, &opts).unwrap();
        assert_eq!(result.text, source);
        assert!(!result.fully_supported);
    }
}
fn assert_fraction(value: &ExpressionValue, negative: bool, numerator: &str, denominator: &str) {
    assert_eq!(
        value,
        &ExpressionValue::Fraction {
            numerator: NumberValue {
                negative,
                integer: numerator.into(),
                fraction: None
            },
            denominator: NumberValue {
                negative: false,
                integer: denominator.into(),
                fraction: None
            },
        }
    );
}

#[test]
fn signed_vulgar_fractions_keep_the_sign_in_numeric_ir_for_all_twenty_locales() {
    for locale in SpeechLocale::ALL {
        let opts = options(locale, SpeechInterpretation::Number);
        for (source, numerator, denominator) in
            [("-½", "1", "2"), ("−⅔", "2", "3"), ("-⅞", "7", "8")]
        {
            let document = parse_for_speech(source, &opts).unwrap();
            assert_eq!(document.spans.len(), 1, "{locale:?}: {document:?}");
            let span = &document.spans[0];
            assert_eq!(
                (span.start, span.end, span.source.as_str()),
                (0, source.len(), source)
            );
            let SpeechValue::Expression { value } = &span.value else {
                panic!("{locale:?}: {document:?}");
            };
            assert_fraction(value, true, numerator, denominator);
            let negative = normalize_for_speech(source, &opts).unwrap();
            let positive =
                normalize_for_speech(source.trim_start_matches(['-', '−']), &opts).unwrap();
            assert!(negative.fully_supported, "{locale:?}: {negative:?}");
            assert_ne!(negative.text, positive.text, "{locale:?}: {source}");
            assert!(
                !negative.text.starts_with(['-', '−']),
                "sign must be verbalized: {locale:?}: {negative:?}"
            );
        }
    }
    assert_eq!(
        normalize_for_speech(
            "-½",
            &options(SpeechLocale::En, SpeechInterpretation::Number)
        )
        .unwrap()
        .text,
        "minus one half"
    );
}

#[test]
fn negative_vulgar_fraction_measures_retain_the_complete_signed_operand() {
    for locale in SpeechLocale::ALL {
        let opts = options(locale, SpeechInterpretation::Measure);
        for (source, numerator, denominator) in [("-½kg", "1", "2"), ("−⅔ kg", "2", "3")] {
            let document = parse_for_speech(source, &opts).unwrap();
            assert_eq!(document.spans.len(), 1, "{locale:?}: {document:?}");
            assert_eq!(document.spans[0].source, source);
            let SpeechValue::MeasuredExpression { unit, value } = &document.spans[0].value else {
                panic!("{locale:?}: {document:?}");
            };
            assert_eq!(unit, "mass-kilogram");
            assert_fraction(value, true, numerator, denominator);
            let negative = normalize_for_speech(source, &opts).unwrap();
            let positive =
                normalize_for_speech(source.trim_start_matches(['-', '−']), &opts).unwrap();
            assert!(negative.fully_supported, "{locale:?}: {negative:?}");
            assert_ne!(negative.text, positive.text, "{locale:?}: {source}");
            assert!(
                !negative.text.starts_with(['-', '−']),
                "{locale:?}: {negative:?}"
            );
        }
    }
}

#[test]
fn signed_fraction_spans_keep_prose_and_utf8_offsets_unchanged() {
    for locale in SpeechLocale::ALL {
        let opts = options(locale, SpeechInterpretation::Number);
        let source = "🙂 −⅔!";
        let document = parse_for_speech(source, &opts).unwrap();
        assert_eq!(
            document
                .spans
                .iter()
                .map(|span| span.source.as_str())
                .collect::<String>(),
            source
        );
        let fraction = document
            .spans
            .iter()
            .find(|span| matches!(span.value, SpeechValue::Expression { .. }))
            .unwrap();
        assert_eq!((fraction.start, fraction.end), ("🙂 ".len(), "🙂 −⅔".len()));
        assert_eq!(fraction.source, "−⅔");
        let SpeechValue::Expression { value } = &fraction.value else {
            unreachable!()
        };
        assert_fraction(value, true, "2", "3");
        let normalized = normalize_for_speech(source, &opts).unwrap();
        assert!(normalized.text.starts_with("🙂 "));
        assert!(normalized.text.ends_with('!'));
        assert!(normalized.fully_supported);
    }
}

#[test]
fn positive_vulgar_sign_matches_existing_positive_ascii_fraction_policy() {
    for locale in SpeechLocale::ALL {
        let opts = options(locale, SpeechInterpretation::Number);
        let ascii = normalize_for_speech("+1/2", &opts).unwrap();
        let vulgar = normalize_for_speech("+½", &opts).unwrap();
        assert!(
            ascii.fully_supported && vulgar.fully_supported,
            "{locale:?}"
        );
        assert_eq!(vulgar.text, ascii.text, "{locale:?}");
        let document = parse_for_speech("+½", &opts).unwrap();
        assert_eq!(document.spans.len(), 1, "{locale:?}: {document:?}");
        let SpeechValue::Expression { value } = &document.spans[0].value else {
            panic!("{locale:?}: {document:?}");
        };
        assert_fraction(value, false, "1", "2");
    }
}

#[test]
fn explicit_number_rejects_dimensional_durations_but_keeps_numeric_expressions() {
    for locale in SpeechLocale::ALL {
        let number_options = options(locale, SpeechInterpretation::Number);
        let duration_options = options(locale, SpeechInterpretation::Duration);
        for source in ["2h30min", "3min15s"] {
            let number = normalize_for_speech(source, &number_options).unwrap();
            assert!(
                !number.fully_supported,
                "Number must not guess Duration: {locale:?}: {number:?}"
            );
            assert_eq!(number.text, source);
            let document = parse_for_speech(source, &number_options).unwrap();
            assert!(!document.spans.iter().any(|span| matches!(
                span.value,
                SpeechValue::Expression {
                    value: ExpressionValue::Duration { .. }
                }
            )));
            let duration = normalize_for_speech(source, &duration_options).unwrap();
            assert!(duration.fully_supported, "{locale:?}: {duration:?}");
            let document = parse_for_speech(source, &duration_options).unwrap();
            assert_eq!(document.spans.len(), 1);
            assert!(matches!(
                document.spans[0].value,
                SpeechValue::Expression {
                    value: ExpressionValue::Duration { .. }
                }
            ));
        }
        for source in ["2e3", "1/2", "4–8", "3rd"] {
            let normalized = normalize_for_speech(source, &number_options).unwrap();
            assert!(
                normalized.fully_supported,
                "dimensionless {locale:?}: {source}: {normalized:?}"
            );
        }
    }
}

#[test]
fn explicit_indian_grouping_uses_the_same_value_for_ascii_and_narrow_spaces() {
    use SpeechLocale::*;
    for locale in [Hi, Bn, Ur, Pa, Mr, Te, Ta] {
        let opts = options(locale, SpeechInterpretation::Number);
        for source in [
            "12 34 567",
            "12\u{202f}34\u{202f}567",
            "12\u{a0}34\u{a0}567",
            "12 34\u{202f}567",
            "12\u{a0}34 567",
            "+12 34 567",
            "-12 34 567",
            "−12 34 567",
        ] {
            let document = parse_for_speech(source, &opts).unwrap();
            assert_eq!(document.spans.len(), 1, "{locale:?}: {document:?}");
            assert_eq!(document.spans[0].source, source);
            let SpeechValue::Number { value, .. } = &document.spans[0].value else {
                panic!("{locale:?}: {document:?}");
            };
            assert_eq!(
                value,
                &NumberValue {
                    negative: source.starts_with(['-', '−']),
                    integer: "1234567".into(),
                    fraction: None
                }
            );
        }
        let ascii = normalize_for_speech("12 34 567", &opts).unwrap();
        let narrow = normalize_for_speech("12\u{202f}34\u{202f}567", &opts).unwrap();
        assert!(ascii.fully_supported && narrow.fully_supported);
        assert_eq!(ascii.text, narrow.text, "{locale:?}");
    }
}

#[test]
fn grouping_adaptation_never_swallows_prose_phone_or_date_boundaries() {
    for locale in SpeechLocale::ALL {
        let opts = options(locale, SpeechInterpretation::Number);
        let source = "12 birds, 34 fish, 567 trees";
        let document = parse_for_speech(source, &opts).unwrap();
        let integers: Vec<_> = document
            .spans
            .iter()
            .filter_map(|span| match &span.value {
                SpeechValue::Number { value, .. } => Some(value.integer.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(integers, ["12", "34", "567"], "{locale:?}: {document:?}");
        assert_eq!(
            document
                .spans
                .iter()
                .map(|span| span.source.as_str())
                .collect::<String>(),
            source
        );
        let phone = "+1 234 567 8901";
        let normalized = normalize_for_speech(phone, &opts).unwrap();
        assert!(!normalized.fully_supported, "{locale:?}: {normalized:?}");
        assert_eq!(normalized.text, phone);
        let mut date_options = opts;
        date_options.interpret_as = SpeechInterpretation::Date;
        let document = parse_for_speech("2024-03-12", &date_options).unwrap();
        assert_eq!(document.spans.len(), 1, "{locale:?}: {document:?}");
        assert!(
            matches!(&document.spans[0].value,
                SpeechValue::Temporal { value: textintel::speech::temporal::TemporalValue::Date { year: Some(year), month: Some(3), day: Some(12), .. } } if year == "2024"
            ),
            "{locale:?}: {document:?}"
        );
    }
    let english = parse_for_speech(
        "12 34 567",
        &options(SpeechLocale::En, SpeechInterpretation::Number),
    )
    .unwrap();
    assert!(!english.spans.iter().any(|span| matches!(&span.value, SpeechValue::Number { value, .. } if value.integer == "1234567")));
}

#[test]
fn indian_grouped_quantity_values_share_the_exact_numeric_scanner() {
    use SpeechLocale::*;
    for locale in [Hi, Bn, Ur, Pa, Mr, Te, Ta] {
        for (suffix, hint) in [
            (" USD", SpeechInterpretation::Currency),
            (" kg", SpeechInterpretation::Measure),
            ("%", SpeechInterpretation::Measure),
        ] {
            let opts = options(locale, hint);
            for written in [
                "12 34 567",
                "12 34\u{202f}567",
                "12\u{a0}34 567",
                "+12 34 567",
                "-12 34 567",
                "−12 34 567",
            ] {
                let source = format!("{written}{suffix}");
                let document = parse_for_speech(&source, &opts).unwrap();
                assert_eq!(document.spans.len(), 1, "{locale:?}: {document:?}");
                assert_eq!(document.spans[0].source, source);
                let value = match &document.spans[0].value {
                    SpeechValue::Currency { code, value } if suffix == " USD" => {
                        assert_eq!(code, "USD");
                        value
                    }
                    SpeechValue::Measure { unit, value } if suffix == " kg" => {
                        assert_eq!(unit, "mass-kilogram");
                        value
                    }
                    SpeechValue::Percent { value } if suffix == "%" => value,
                    _ => panic!("{locale:?}: {document:?}"),
                };
                assert_eq!(
                    value,
                    &NumberValue {
                        negative: written.starts_with(['-', '−']),
                        integer: "1234567".into(),
                        fraction: None
                    }
                );
                assert!(
                    normalize_for_speech(&source, &opts)
                        .unwrap()
                        .fully_supported,
                    "{locale:?}: {source}"
                );
            }
        }
    }
}

#[test]
fn natural_auto_keeps_phone_shapes_while_explicit_number_keeps_ranges() {
    for locale in SpeechLocale::ALL {
        let auto = options(locale, SpeechInterpretation::Auto);
        for source in ["123-4567", "555-0123", "+1-234-567-8901"] {
            let document = parse_for_speech(source, &auto).unwrap();
            assert_eq!(document.spans.len(), 1, "{locale:?}: {document:?}");
            assert!(
                matches!(&document.spans[0].value, SpeechValue::Identifier { kind } if kind == "telephone")
            );
            let normalized = normalize_for_speech(source, &auto).unwrap();
            assert_eq!(normalized.text, source);
            assert!(!normalized.fully_supported);
        }
        for source in ["10-15", "123–4567"] {
            let document = parse_for_speech(source, &auto).unwrap();
            assert!(
                matches!(
                    document.spans[0].value,
                    SpeechValue::Expression {
                        value: ExpressionValue::Range { .. }
                    }
                ),
                "{locale:?}: {document:?}"
            );
        }
        let numeric = options(locale, SpeechInterpretation::Number);
        let document = parse_for_speech("123-4567", &numeric).unwrap();
        assert!(
            matches!(
                document.spans[0].value,
                SpeechValue::Expression {
                    value: ExpressionValue::Range { .. }
                }
            ),
            "{locale:?}: {document:?}"
        );
    }
}

#[test]
fn common_filename_extensions_protect_original_tokens_and_byte_offsets() {
    for locale in SpeechLocale::ALL {
        let opts = options(locale, SpeechInterpretation::Auto);
        for filename in [
            "3kg.txt",
            "2024-03-12.json",
            "12:30.wav",
            "clip27.mp3",
            "12.50USD.csv",
            "123-4567.PDF",
        ] {
            let document = parse_for_speech(filename, &opts).unwrap();
            assert_eq!(document.spans.len(), 1, "{locale:?}: {document:?}");
            assert!(
                matches!(&document.spans[0].value, SpeechValue::Identifier { kind } if kind == "filename")
            );
            let normalized = normalize_for_speech(filename, &opts).unwrap();
            assert_eq!(normalized.text, filename);
            assert!(!normalized.fully_supported);
        }
        let source = "🙂 (3kg.txt), 4 kg。";
        let document = parse_for_speech(source, &opts).unwrap();
        let filename = document.spans.iter().find(|span| matches!(&span.value, SpeechValue::Identifier { kind } if kind == "filename")).unwrap();
        assert_eq!(filename.source, "3kg.txt");
        assert_eq!(
            (filename.start, filename.end),
            ("🙂 (".len(), "🙂 (3kg.txt".len())
        );
        assert_eq!(
            document
                .spans
                .iter()
                .map(|span| span.source.as_str())
                .collect::<String>(),
            source
        );
        let normalized = normalize_for_speech(source, &opts).unwrap();
        let reading = normalized
            .readings
            .iter()
            .find(|reading| reading.source == "3kg.txt")
            .unwrap();
        assert_eq!(reading.spoken, "3kg.txt");
        for source in ["3kg.", "3kg。", "3kg，4kg"] {
            let document = parse_for_speech(source, &opts).unwrap();
            assert!(!document.spans.iter().any(
                |span| matches!(&span.value, SpeechValue::Identifier { kind } if kind == "filename")
            ));
            assert!(
                normalize_for_speech(source, &opts).unwrap().fully_supported,
                "{locale:?}: {document:?}"
            );
        }
        for source in [
            "https://example.org/3kg.txt",
            "www.3kg.txt",
            "user@3kg.txt",
            "@clip27.mp3",
            "#clip27.csv",
        ] {
            let document = parse_for_speech(source, &opts).unwrap();
            assert!(!document.spans.iter().any(|span| matches!(&span.value, SpeechValue::Identifier { kind } if kind == "filename")), "{locale:?}: {document:?}");
            assert_eq!(normalize_for_speech(source, &opts).unwrap().text, source);
        }
    }
}

#[test]
fn common_filename_guard_does_not_capture_named_calendar_months() {
    for (locale, source) in [
        (SpeechLocale::De, "1. Jan. 2024"),
        (SpeechLocale::Es, "1 de enero de 2024"),
    ] {
        let opts = options(locale, SpeechInterpretation::Date);
        let document = parse_for_speech(source, &opts).unwrap();
        assert_eq!(document.spans.len(), 1, "{document:?}");
        assert!(
            matches!(&document.spans[0].value, SpeechValue::Temporal { value: textintel::speech::temporal::TemporalValue::Date { year: Some(year), month: Some(1), day: Some(1), .. } } if year == "2024"),
            "{document:?}"
        );
        assert!(normalize_for_speech(source, &opts).unwrap().fully_supported);
    }
}
