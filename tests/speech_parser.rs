use textintel::speech::{
    DateOrder, NumberReading, SpeechLocale, SpeechOptions, SpeechValue, parse_for_speech,
};

fn options() -> SpeechOptions {
    let mut options = SpeechOptions::new(SpeechLocale::Es);
    options.decimal_separator = Some('.');
    options
}
fn values(text: &str, options: &SpeechOptions) -> Vec<SpeechValue> {
    parse_for_speech(text, options)
        .unwrap()
        .spans
        .into_iter()
        .filter_map(|span| (!matches!(span.value, SpeechValue::Text)).then_some(span.value))
        .collect()
}
fn one(text: &str, options: &SpeechOptions) -> SpeechValue {
    let document = parse_for_speech(text, options).unwrap();
    assert_eq!(document.spans.len(), 1, "{text}: {:?}", document.spans);
    assert_eq!(document.spans[0].source, text);
    document.spans.into_iter().next().unwrap().value
}

#[test]
fn lossless_unicode_partition_covers_every_input_byte() {
    let input = "你好, el 2024-02-29 pagué PEN 12.50; ٣ kg a las 9:05. 🌎";
    for locale in SpeechLocale::ALL {
        let mut options = options();
        options.locale = locale;
        let parsed = parse_for_speech(input, &options).unwrap();
        let mut cursor = 0;
        for span in &parsed.spans {
            assert_eq!(span.start, cursor);
            assert!(span.start < span.end);
            assert_eq!(&input[span.start..span.end], span.source);
            cursor = span.end;
        }
        assert_eq!(cursor, input.len());
        assert_eq!(
            parsed
                .spans
                .iter()
                .map(|s| s.source.as_str())
                .collect::<String>(),
            input
        );
    }
}

#[test]
fn calendar_validation_preserves_ambiguity_and_iso_order() {
    let mut options = options();
    assert_eq!(
        one("2024-02-29", &options),
        SpeechValue::Date {
            year: 2024,
            month: 2,
            day: 29
        }
    );
    for invalid in [
        "2023-02-29",
        "1900-02-29",
        "2024-04-31",
        "31/31/2024",
        "13/13/2024",
        "01/01/24",
        "0000-01-01",
    ] {
        assert!(
            matches!(one(invalid, &options), SpeechValue::Ambiguous { .. }),
            "{invalid}"
        );
    }
    assert!(matches!(
        one("03/04/2024", &options),
        SpeechValue::Ambiguous { .. }
    ));
    assert_eq!(
        one("31/01/2024", &options),
        SpeechValue::Date {
            year: 2024,
            month: 1,
            day: 31
        }
    );
    assert_eq!(
        one("01/31/2024", &options),
        SpeechValue::Date {
            year: 2024,
            month: 1,
            day: 31
        }
    );
    options.date_order = DateOrder::DayMonthYear;
    assert_eq!(
        one("03/04/2024", &options),
        SpeechValue::Date {
            year: 2024,
            month: 4,
            day: 3
        }
    );
    options.date_order = DateOrder::MonthDayYear;
    assert_eq!(
        one("03/04/2024", &options),
        SpeechValue::Date {
            year: 2024,
            month: 3,
            day: 4
        }
    );
    assert_eq!(
        one("2024-02-29", &options),
        SpeechValue::Date {
            year: 2024,
            month: 2,
            day: 29
        }
    );
}

#[test]
fn parses_clock_fields_and_rejects_invalid_full_times() {
    for (input, hour, minute, second) in [
        ("9:05", 9, 5, None),
        ("23:59:59", 23, 59, Some(59)),
        ("9:05pm", 21, 5, None),
        ("12:00 AM", 0, 0, None),
    ] {
        assert_eq!(
            one(input, &options()),
            SpeechValue::Time {
                hour,
                minute,
                second
            }
        );
    }
    for input in [
        "99:30",
        "25:00",
        "9:60",
        "12:10:99",
        "12:30:00:99",
        "9:5",
        "0:30pm",
    ] {
        assert!(
            matches!(one(input, &options()), SpeechValue::Ambiguous { .. }),
            "{input}"
        );
    }
}

#[test]
fn currency_percent_units_and_numeric_codes_keep_semantics() {
    let options = options();
    for input in ["USD 12.50", "12.50 USD", "S/ 12.50"] {
        let SpeechValue::Currency { code, value } = one(input, &options) else {
            panic!("{input}")
        };
        assert_eq!(
            code,
            if input.starts_with("S/") {
                "PEN"
            } else {
                "USD"
            }
        );
        assert_eq!(value.integer, "12");
        assert_eq!(value.fraction.as_deref(), Some("50"));
    }
    assert!(matches!(
        one("$12", &options),
        SpeechValue::Ambiguous { .. }
    ));
    assert!(matches!(
        one("¥12", &options),
        SpeechValue::Ambiguous { .. }
    ));
    assert!(matches!(
        one("12.5%", &options),
        SpeechValue::Percent { .. }
    ));
    for input in ["3 kg", "3kg", "3 min", "3 °C", "3km/h"] {
        assert!(
            matches!(one(input, &options), SpeechValue::Measure { .. }),
            "{input}"
        );
    }
    let SpeechValue::Number { value, reading } = one("00123", &options) else {
        panic!("number")
    };
    assert_eq!(value.integer, "00123");
    assert_eq!(reading, NumberReading::Digits);
    let SpeechValue::Number { value, .. } = one("−42.007", &options) else {
        panic!("signed")
    };
    assert!(value.negative);
    assert_eq!(value.fraction.as_deref(), Some("007"));
}

#[test]
fn exact_decimals_do_not_round_and_separator_policy_is_explicit() {
    let mut options = SpeechOptions::new(SpeechLocale::En);
    assert!(matches!(
        one("1,234", &options),
        SpeechValue::Ambiguous { .. }
    ));
    assert!(matches!(
        one("1.234", &options),
        SpeechValue::Ambiguous { .. }
    ));
    options.decimal_separator = Some('.');
    let SpeechValue::Number { value, .. } = one("1,234.000000000000000001", &options) else {
        panic!("decimal")
    };
    assert_eq!(value.integer, "1234");
    assert_eq!(value.fraction.as_deref(), Some("000000000000000001"));
    options.decimal_separator = Some(',');
    let SpeechValue::Number { value, .. } = one("1.234,50", &options) else {
        panic!("decimal")
    };
    assert_eq!(value.integer, "1234");
    assert_eq!(value.fraction.as_deref(), Some("50"));
    for bad in ["1,,234", "1,23,4", "12.34,5"] {
        assert!(
            matches!(one(bad, &options), SpeechValue::Ambiguous { .. }),
            "{bad}"
        );
    }
}

#[test]
fn native_decimal_digit_scripts_are_parsed_without_offset_changes() {
    for input in ["٤٢", "۴۲", "४२", "৪২", "੪੨", "௪௨", "౪౨", "４２", "𝟜𝟚"]
    {
        for locale in SpeechLocale::ALL {
            let options = SpeechOptions::new(locale);
            let SpeechValue::Number { value, .. } = one(input, &options) else {
                panic!("{input}/{locale:?}")
            };
            assert_eq!(value.integer, "42");
        }
    }
    let SpeechValue::Number { value, .. } = one("١٬٢٣٤٫٥٠", &SpeechOptions::new(SpeechLocale::Ar))
    else {
        panic!("Arabic decimal")
    };
    assert_eq!(value.integer, "1234");
    assert_eq!(value.fraction.as_deref(), Some("50"));
    let mut options = options();
    options.locale = SpeechLocale::Hi;
    let SpeechValue::Number { value, .. } = one("12,34,567.50", &options) else {
        panic!("Indian grouping")
    };
    assert_eq!(value.integer, "1234567");
}

#[test]
fn identifiers_are_not_expanded_as_dates_amounts_or_numbers() {
    for input in [
        "https://example.test/2024-01-01?x=42",
        "ada@example.com",
        "+51 987 654 321",
        "123-456-7890",
        "v1.2.3",
        "1.2.3",
        "abc2024-01-01xyz",
        "abc123",
        "1e3",
    ] {
        assert!(
            matches!(one(input, &options()), SpeechValue::Identifier { .. }),
            "{input}"
        );
    }
    assert_eq!(
        values("Email ada@example.com, then 42!", &options()).len(),
        2
    );
}

#[test]
fn limits_error_instead_of_truncating_and_bad_options_fail_early() {
    let mut options = options();
    options.max_input_chars = 2;
    assert!(parse_for_speech("你好世", &options).is_err());
    options.max_input_chars = 100;
    options.max_spans = 1;
    assert!(parse_for_speech("one 42 two", &options).is_err());
    options.max_spans = 20;
    options.decimal_separator = Some(';');
    assert!(parse_for_speech("42", &options).is_err());
    options.decimal_separator = None;
    assert!(parse_for_speech("", &options).unwrap().spans.is_empty());
}

proptest::proptest! {
    #[test]
    fn arbitrary_unicode_roundtrips_without_span_panics(input in ".{0,100}") {
        let mut options=SpeechOptions::new(SpeechLocale::Es);options.max_spans=1000;
        let parsed=parse_for_speech(&input,&options).unwrap();
        let output=parsed.spans.iter().map(|span|span.source.as_str()).collect::<String>();
        proptest::prop_assert_eq!(output.as_str(),input.as_str());
        let mut end=0;
        for span in parsed.spans {
            proptest::prop_assert_eq!(span.start,end);
            proptest::prop_assert_eq!(&input[span.start..span.end],span.source);
            end=span.end;
        }
        proptest::prop_assert_eq!(end,input.len());
    }
}

#[test]
fn space_grouped_amounts_are_complete_and_cjk_needs_no_spaces() {
    let parsed = values("PEN 1 234.50", &options());
    let SpeechValue::Currency { value, .. } = &parsed[0] else {
        panic!("{parsed:?}")
    };
    assert_eq!(value.integer, "1234");
    assert_eq!(value.fraction.as_deref(), Some("50"));
    assert_eq!(values("1.25 123", &options()).len(), 2);
    for text in ["USD12.50", "12.50USD"] {
        assert!(matches!(
            one(text, &options()),
            SpeechValue::Currency { .. }
        ));
    }
    let parsed = parse_for_speech("总数是４２。", &options()).unwrap();
    assert!(
        parsed
            .spans
            .iter()
            .any(|s| s.source == "４２" && matches!(s.value, SpeechValue::Number { .. }))
    );
    let parsed = parse_for_speech("温度は３ °Cです。", &options()).unwrap();
    assert!(
        parsed
            .spans
            .iter()
            .any(|s| s.source == "３ °C" && matches!(s.value, SpeechValue::Measure { .. }))
    );
}

#[test]
fn malformed_compounds_are_not_fragmented_into_valid_inner_numbers() {
    let options = options();
    for text in ["-1e3", "1.e3", "1,e3", "-1e-3", "1,2e3", "0xFF"] {
        assert!(
            matches!(one(text, &options), SpeechValue::Identifier { .. }),
            "{text}"
        );
    }
    for text in [
        "1-2",
        "1 – 2",
        "1 — 2",
        "1 … 2",
        "2024-01-010",
        "2024-01-01-99",
        "-2024-01-01",
        "12:30foo",
    ] {
        assert!(
            matches!(one(text, &options), SpeechValue::Ambiguous { .. }),
            "{text}"
        );
    }
    for text in ["(2024-02-29)", "[2024-02-29]"] {
        assert_eq!(
            values(text, &options),
            vec![SpeechValue::Date {
                year: 2024,
                month: 2,
                day: 29
            }]
        );
    }
    assert!(matches!(
        values("(2023-02-29)", &options).as_slice(),
        [SpeechValue::Ambiguous { .. }]
    ));
}

#[test]
fn leading_decimal_notation_keeps_the_fraction_instead_of_reading_an_integer() {
    for (text, negative, fraction) in [
        (".5", false, "5"),
        ("-.5", true, "5"),
        ("+.005", false, "005"),
    ] {
        let SpeechValue::Number { value, .. } = one(text, &options()) else {
            panic!("{text}")
        };
        assert_eq!(value.integer, "0");
        assert_eq!(value.fraction.as_deref(), Some(fraction));
        assert_eq!(value.negative, negative);
    }
}
