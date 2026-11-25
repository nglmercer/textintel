use textintel::speech::expressions::{self, DurationComponent, DurationUnit, ExpressionValue};
use textintel::speech::{NumberValue, ReadingStyle, SpeechLocale, SpeechOptions};

fn options(locale: SpeechLocale) -> SpeechOptions {
    let mut options = SpeechOptions::new(locale);
    options.reading_style = ReadingStyle::Natural;
    options.decimal_separator = Some('.');
    options
}
fn literal(n: &str) -> NumberValue {
    NumberValue {
        negative: false,
        integer: n.into(),
        fraction: None,
    }
}

#[test]
fn basic_mathematical_expression_contract_is_shared_by_all_twenty_locales() {
    for locale in SpeechLocale::ALL {
        let opts = options(locale);
        for (source, kind) in [
            ("2e3", "scientific"),
            ("1/2", "fraction"),
            ("10–15", "range"),
            ("1h30min", "duration"),
        ] {
            let (end, value) = expressions::parse_at(source, 0, &opts)
                .unwrap_or_else(|| panic!("{} {source}", locale.code()));
            assert_eq!(end, source.len());
            assert_eq!(expressions::kind(&value), kind);
            let spoken = expressions::render(&value, &opts)
                .unwrap_or_else(|| panic!("{} {source}", locale.code()));
            assert!(!spoken.is_empty());
            assert_ne!(spoken, source, "{} {source}", locale.code());
        }
    }
}

#[test]
fn scientific_values_and_operator_forms_share_one_exact_decimal_parser() {
    let opts = options(SpeechLocale::En);
    for source in ["2.50e-3", "2.50 × 10^-3", "2.50*10^-3", "2.50 x10^-3"] {
        let (end, value) = expressions::parse_at(source, 0, &opts).unwrap();
        assert_eq!(end, source.len());
        assert_eq!(
            value,
            ExpressionValue::Scientific {
                mantissa: NumberValue {
                    negative: false,
                    integer: "2".into(),
                    fraction: Some("50".into())
                },
                exponent: -3
            }
        );
        assert_eq!(
            expressions::render(&value, &opts).as_deref(),
            Some("two point five zero times ten to the power of minus three")
        );
    }
    let mut opts = options(SpeechLocale::Es);
    opts.decimal_separator = Some(',');
    let (_, value) = expressions::parse_at("٢٫٥٠e−٣", 0, &options(SpeechLocale::Ar)).unwrap();
    assert_eq!(
        value,
        ExpressionValue::Scientific {
            mantissa: NumberValue {
                negative: false,
                integer: "2".into(),
                fraction: Some("50".into())
            },
            exponent: -3
        }
    );
    let (_, value) = expressions::parse_at("2,50e-3", 0, &opts).unwrap();
    assert_eq!(
        expressions::render(&value, &opts).as_deref(),
        Some("dos coma cinco cero por diez elevado a menos tres")
    );
}

#[test]
fn invalid_constructs_do_not_produce_partially_valid_expressions() {
    let opts = options(SpeechLocale::En);
    for source in [
        "1/0",
        "1/-0.00",
        "1/2/3",
        "1.e3",
        "1e",
        "1e+",
        "1e10000",
        "1e00001",
        "1e3abc",
        "1/2abc",
        "2024-01-01",
        "123-456-789",
        "11st",
        "12nd",
        "13rd",
        "1th",
        "2er",
        "1h30min15min",
    ] {
        assert!(
            expressions::parse_at(source, 0, &opts).is_none(),
            "{source}"
        );
    }
    for source in ["1/2", "1e3", "10-15", "1h30min"] {
        assert!(
            expressions::parse_at(source, 0, &SpeechOptions::new(SpeechLocale::En)).is_none(),
            "{source}"
        );
    }
    assert!(expressions::parse_at("📅 1/2", 1, &opts).is_none());
    assert!(expressions::parse_at("1/2", 99, &opts).is_none());
}

#[test]
fn expression_spans_preserve_bytes_and_leave_sentence_punctuation_outside() {
    let opts = options(SpeechLocale::En);
    for (source, start, expected) in [
        ("📚 1/2.", 5, "1/2"),
        ("📚 10–15,", 5, "10–15"),
        ("2e3.", 0, "2e3"),
    ] {
        let (end, _) = expressions::parse_at(source, start, &opts).unwrap();
        assert_eq!(&source[start..end], expected);
    }
    let (end, _) = expressions::parse_at("1h30min 42 words", 0, &opts).unwrap();
    assert_eq!(end, "1h30min".len());
}

#[test]
fn explicit_ordinals_validate_markers_and_use_language_specific_morphology() {
    for (locale, source, gold) in [
        (SpeechLocale::En, "21st", "twenty first"),
        (SpeechLocale::En, "12th", "twelfth"),
        (SpeechLocale::Es, "23º", "vigésimo tercero"),
        (SpeechLocale::Es, "23ª", "vigésima tercera"),
        (SpeechLocale::Pt, "2ª", "segunda"),
        (SpeechLocale::Fr, "1re", "première"),
        (SpeechLocale::Fr, "5e", "cinquième"),
        (SpeechLocale::De, "3. Platz", "dritte"),
        (SpeechLocale::Tr, "4. sıra", "dördüncü"),
        (SpeechLocale::Zh, "第12", "第十二"),
        (SpeechLocale::Ko, "제12", "제십이"),
        (SpeechLocale::Ko, "12번째", "열두 번째"),
        (SpeechLocale::Id, "ke-3", "ketiga"),
        (SpeechLocale::Vi, "thứ 4", "thứ tư"),
    ] {
        let opts = options(locale);
        let (_, value) = expressions::parse_at(source, 0, &opts)
            .unwrap_or_else(|| panic!("{} {source}", locale.code()));
        assert_eq!(
            expressions::render(&value, &opts).as_deref(),
            Some(gold),
            "{} {source}",
            locale.code()
        );
    }
}

#[test]
fn public_expression_values_fail_closed_on_invalid_or_unbounded_values() {
    let opts = options(SpeechLocale::En);
    for value in [
        ExpressionValue::Scientific {
            mantissa: literal("1"),
            exponent: i32::MIN,
        },
        ExpressionValue::Fraction {
            numerator: literal("1"),
            denominator: literal("0"),
        },
        ExpressionValue::Range {
            from: literal("x"),
            to: literal("2"),
        },
        ExpressionValue::Ordinal {
            value: u64::MAX,
            marker: "th".into(),
        },
        ExpressionValue::Duration { components: vec![] },
        ExpressionValue::Duration {
            components: vec![
                DurationComponent {
                    unit: DurationUnit::Hours,
                    value: 1,
                },
                DurationComponent {
                    unit: DurationUnit::Hours,
                    value: 2,
                },
            ],
        },
    ] {
        assert!(expressions::render(&value, &opts).is_none(), "{value:?}");
    }
}

#[test]
fn vulgar_fraction_symbols_keep_their_exact_rational_value() {
    let opts = options(SpeechLocale::En);
    for (text, n, d) in [("½", "1", "2"), ("⅔", "2", "3"), ("⅞", "7", "8")] {
        let (end, value) = expressions::parse_at(text, 0, &opts).unwrap();
        assert_eq!(end, text.len());
        assert_eq!(
            value,
            ExpressionValue::Fraction {
                numerator: literal(n),
                denominator: literal(d)
            }
        );
    }
    assert!(expressions::parse_at("½kgXYZ", 0, &opts).is_none());
}

#[test]
fn mixed_fractions_keep_whole_and_rational_parts_exact_and_agree_with_nouns() {
    for locale in SpeechLocale::ALL {
        let opts = options(locale);
        let (end, value) = expressions::parse_at("3 3/5", 0, &opts).unwrap();
        assert_eq!(end, 5);
        assert_eq!(
            value,
            ExpressionValue::MixedFraction {
                whole: literal("3"),
                numerator: literal("3"),
                denominator: literal("5")
            }
        );
        assert!(
            expressions::render(&value, &opts).is_some(),
            "{}",
            locale.code()
        );
    }
    let es = options(SpeechLocale::Es);
    let (_, value) = expressions::parse_at("3 3/5m", 0, &es).unwrap();
    assert_eq!(
        expressions::render(&value, &es).as_deref(),
        Some("tres y tres quintos")
    );
    let (_, value) = expressions::parse_at("1 1/2", 0, &es).unwrap();
    assert_eq!(
        expressions::render_with_gender(&value, &es, true).as_deref(),
        Some("una y media")
    );
    let en = options(SpeechLocale::En);
    let (_, value) = expressions::parse_at("3 3/5", 0, &en).unwrap();
    assert_eq!(
        expressions::render(&value, &en).as_deref(),
        Some("three and three fifths")
    );
    let (_, value) = expressions::parse_at("1/2kg", 0, &en).unwrap();
    assert_eq!(
        expressions::render(&value, &en).as_deref(),
        Some("one half")
    );
}

#[test]
fn basic_ordinal_meaning_is_available_for_all_twenty_language_conventions() {
    let golds = [
        (SpeechLocale::En, "third"),
        (SpeechLocale::Es, "tercero"),
        (SpeechLocale::Zh, "第三"),
        (SpeechLocale::Hi, "तीसरा"),
        (SpeechLocale::Ar, "الثالث"),
        (SpeechLocale::Fr, "troisième"),
        (SpeechLocale::Pt, "terceiro"),
        (SpeechLocale::Ru, "третий"),
        (SpeechLocale::Bn, "তৃতীয়"),
        (SpeechLocale::Ur, "تیسرا"),
        (SpeechLocale::Id, "ketiga"),
        (SpeechLocale::De, "dritte"),
        (SpeechLocale::Ja, "だいさん"),
        (SpeechLocale::Pa, "ਤੀਜਾ"),
        (SpeechLocale::Mr, "तिसरा"),
        (SpeechLocale::Te, "మూడవ"),
        (SpeechLocale::Tr, "üçüncü"),
        (SpeechLocale::Ta, "மூன்றாவது"),
        (SpeechLocale::Vi, "thứ ba"),
        (SpeechLocale::Ko, "제삼"),
    ];
    assert_eq!(golds.len(), SpeechLocale::ALL.len());
    for (locale, gold) in golds {
        let opts = options(locale);
        let (_, value) = expressions::parse_at("3rd", 0, &opts).unwrap();
        assert_eq!(
            expressions::render(&value, &opts).as_deref(),
            Some(gold),
            "{}",
            locale.code()
        );
    }
}

#[test]
fn explicit_duration_policy_allows_one_component_and_identifiers_keep_boundaries() {
    let mut opts = SpeechOptions::new(SpeechLocale::En);
    opts.interpret_as = textintel::speech::SpeechInterpretation::Duration;
    let (end, value) = expressions::parse_at("1h", 0, &opts).unwrap();
    assert_eq!(end, 2);
    assert_eq!(expressions::kind(&value), "duration");
    assert_eq!(
        expressions::render(&value, &opts).as_deref(),
        Some("one hour")
    );
    assert!(expressions::parse_at("a3rd", 1, &options(SpeechLocale::En)).is_none());
    let (end, _) = expressions::parse_at("第3章", 0, &options(SpeechLocale::Zh)).unwrap();
    assert_eq!(&"第3章"[..end], "第3");
}

#[test]
fn typed_expression_shape_policies_preserve_native_bytes_and_explicit_components() {
    let opts = options(SpeechLocale::En);
    for source in ["+2.50e+123", "2.50 × 10^123", "2.50e−0003"] {
        let (end, value) = expressions::parse_at(source, 0, &opts).unwrap();
        assert_eq!(end, source.len());
        assert_eq!(expressions::kind(&value), "scientific");
    }
    for source in ["1h30", "1h30sec", "45sec", "1h30min15min", "30min1h"] {
        assert!(
            expressions::parse_at(source, 0, &opts).is_none(),
            "{source}"
        );
    }
    for source in ["1hr30min", "1 h 30 min", "0h30min"] {
        let (end, value) = expressions::parse_at(source, 0, &opts).unwrap();
        assert_eq!(end, source.len());
        assert_eq!(expressions::kind(&value), "duration");
    }
    let (_, value) = expressions::parse_at("0h30min", 0, &opts).unwrap();
    assert_eq!(
        expressions::render(&value, &opts).as_deref(),
        Some("zero hours thirty minutes")
    );
    let source = "📚 ١h٣٠min.";
    let start = "📚 ".len();
    let (end, value) = expressions::parse_at(source, start, &opts).unwrap();
    assert_eq!(&source[start..end], "١h٣٠min");
    assert_eq!(
        expressions::render(&value, &opts).as_deref(),
        Some("one hour thirty minutes")
    );
    let source = "📚 ٢٫٥e−٣.";
    let (end, value) = expressions::parse_at(source, start, &opts).unwrap();
    assert_eq!(&source[start..end], "٢٫٥e−٣");
    assert!(matches!(
        value,
        ExpressionValue::Scientific { exponent: -3, .. }
    ));
    for source in [
        "`١h٣٠min ٢e٣`",
        "https://example.org/٢e٣",
        "user٢e٣@example.org",
    ] {
        assert_eq!(
            textintel::speech::normalize_for_speech(source, &opts)
                .unwrap()
                .text,
            source
        );
    }
    for source in ["١h٣٠minABC", "١e٣abc", "1h1000000000000min"] {
        assert!(
            expressions::parse_at(source, 0, &opts).is_none(),
            "{source}"
        );
    }
    let mut explicit = opts.clone();
    explicit.interpret_as = textintel::speech::SpeechInterpretation::Duration;
    let (_, value) = expressions::parse_at("90min", 0, &explicit).unwrap();
    assert_eq!(
        expressions::render(&value, &explicit).as_deref(),
        Some("ninety minutes")
    );
}

#[test]
fn mixed_numbers_reduce_exactly_and_preserve_the_sign_of_the_complete_quantity() {
    let value = |n: &str, negative: bool| NumberValue {
        negative,
        integer: n.into(),
        fraction: None,
    };
    for (whole, negative, numerator, denominator, gold) in [
        ("1", false, "1", "2", "3"),
        ("1", true, "1", "2", "3"),
        ("3", false, "3", "5", "18"),
        ("0", true, "1", "2", "1"),
    ] {
        let mixed = ExpressionValue::MixedFraction {
            whole: value(whole, negative),
            numerator: value(numerator, false),
            denominator: value(denominator, false),
        };
        let (n, d) = expressions::rational_components(&mixed).unwrap();
        assert_eq!(n, value(gold, negative));
        assert_eq!(d, value(denominator, false));
    }
    let overflow = ExpressionValue::MixedFraction {
        whole: value(&u128::MAX.to_string(), false),
        numerator: value("1", false),
        denominator: value("2", false),
    };
    assert!(expressions::rational_components(&overflow).is_none());
    let zero = ExpressionValue::MixedFraction {
        whole: value("1", false),
        numerator: value("1", false),
        denominator: value("0", false),
    };
    assert!(expressions::rational_components(&zero).is_none());
}

#[test]
fn attached_vulgar_fraction_sign_is_part_of_the_exact_value_and_source_span() {
    let opts = options(SpeechLocale::En);
    for (source, negative, numerator, denominator) in [
        ("-½", true, "1", "2"),
        ("−⅔", true, "2", "3"),
        ("+½", false, "1", "2"),
    ] {
        let (end, value) = expressions::parse_at(source, 0, &opts).unwrap();
        assert_eq!(end, source.len());
        assert_eq!(
            value,
            ExpressionValue::Fraction {
                numerator: NumberValue {
                    negative,
                    integer: numerator.into(),
                    fraction: None
                },
                denominator: literal(denominator),
            }
        );
        let (n, _) = expressions::rational_components(&value).unwrap();
        assert_eq!(n.negative, negative);
    }
    assert!(expressions::parse_at("− ½", 0, &opts).is_none());
    let (end, value) = expressions::parse_at("3 − ½", "3 − ".len(), &opts).unwrap();
    assert_eq!(end, "3 − ½".len());
    assert_eq!(
        expressions::render(&value, &opts).as_deref(),
        Some("one half")
    );
    for (source, expected) in [
        ("📚 −½kg.", "📚 minus one half kilogram."),
        ("-1 1/2kg", "minus one and one half kilograms"),
    ] {
        let result = textintel::speech::normalize_for_speech(source, &opts).unwrap();
        assert_eq!(result.text, expected);
        assert!(result.fully_supported);
    }
}

#[test]
fn russian_range_endpoints_use_genitive_case() {
    let ru = options(SpeechLocale::Ru);
    let (_, value) = expressions::parse_at("10–15", 0, &ru).unwrap();
    assert_eq!(
        expressions::render(&value, &ru).as_deref(),
        Some("от десяти до пятнадцати")
    );
}

#[test]
fn russian_ordinals_compose_final_components_without_cardinal_substitution() {
    let ru = options(SpeechLocale::Ru);
    for (value, gold) in [
        (21, "двадцать первый"),
        (101, "сто первый"),
        (110, "сто десятый"),
        (2021, "две тысячи двадцать первый"),
        (1000, "тысячный"),
    ] {
        assert_eq!(
            expressions::ordinal(value, "st", &ru).as_deref(),
            Some(gold),
            "{value}"
        );
    }
    // Exact non-one thousands use a distinct fused paradigm that is not yet
    // part of the reviewed inventory; they fail closed rather than becoming
    // a cardinal phrase.
    assert_eq!(expressions::ordinal(2000, "st", &ru), None);
}
