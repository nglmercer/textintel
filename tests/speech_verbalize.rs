use textintel::speech::{
    DateOrder, NumberReading, NumberValue, SpeechDocument, SpeechLocale, SpeechOptions, SpeechSpan,
    SpeechValue, normalize_for_speech, verbalize_document,
};

fn n(integer: &str, fraction: Option<&str>) -> NumberValue {
    NumberValue {
        negative: false,
        integer: integer.into(),
        fraction: fraction.map(str::to_string),
    }
}
fn single(
    locale: SpeechLocale,
    source: &str,
    value: SpeechValue,
) -> textintel::speech::SpeechNormalization {
    verbalize_document(&SpeechDocument {
        text: source.into(),
        locale,
        spans: vec![SpeechSpan {
            start: 0,
            end: source.len(),
            source: source.into(),
            value,
        }],
    })
    .unwrap()
}

#[test]
fn spanish_reads_dates_money_and_units_without_similarity_rewrites() {
    let mut options = SpeechOptions::new(SpeechLocale::Es);
    options.decimal_separator = Some('.');
    let result =
        normalize_for_speech("Nooo: el 2024-11-13 pagué S/ 12.50 por 1 kg.", &options).unwrap();
    assert_eq!(
        result.text,
        "Nooo: el trece de noviembre de dos mil veinticuatro pagué doce soles con cincuenta céntimos por un kilogramo."
    );
    assert!(result.fully_supported);
    for reading in result.readings {
        assert!(reading.supported);
        assert!(reading.reason.is_none());
    }
}

#[test]
fn explicit_date_order_resolves_interpretation_but_default_preserves_ambiguity() {
    let mut options = SpeechOptions::new(SpeechLocale::En);
    let ambiguous = normalize_for_speech("03/04/2024", &options).unwrap();
    assert!(!ambiguous.fully_supported);
    assert_eq!(ambiguous.text, "03/04/2024");
    options.date_order = DateOrder::MonthDayYear;
    assert_eq!(
        normalize_for_speech("03/04/2024", &options).unwrap().text,
        "March fourth, two thousand twenty-four"
    );
}

#[test]
fn gender_and_plural_agreement_is_applied_to_quantities() {
    for (locale, source, unit, integer, expected) in [
        (SpeechLocale::Es, "1 h", "h", "1", "una hora"),
        (SpeechLocale::Es, "21 h", "h", "21", "veintiuna horas"),
        (SpeechLocale::Es, "21 kg", "kg", "21", "veintiún kilogramos"),
        (SpeechLocale::Fr, "1 h", "h", "1", "une heure"),
        (SpeechLocale::Pt, "2 h", "h", "2", "duas horas"),
        (SpeechLocale::De, "1 h", "h", "1", "eine Stunde"),
        (SpeechLocale::Ru, "1 min", "min", "1", "одна минута"),
        (SpeechLocale::Ru, "2 min", "min", "2", "две минуты"),
        (SpeechLocale::Ru, "5 min", "min", "5", "пять минут"),
    ] {
        let r = single(
            locale,
            source,
            SpeechValue::Measure {
                unit: unit.into(),
                value: n(integer, None),
            },
        );
        assert!(r.fully_supported, "{locale:?} {r:?}");
        assert_eq!(r.text, expected, "{locale:?}");
    }
    // Scale nouns stay masculine while the following noun requires its connector.
    let large = single(
        SpeechLocale::Es,
        "200000000 h",
        SpeechValue::Measure {
            unit: "h".into(),
            value: n("200000000", None),
        },
    );
    assert!(large.fully_supported);
    assert_eq!(large.text, "doscientos millones de horas");
}

#[test]
fn arabic_implicit_unit_forms_keep_explicit_decimal_precision() {
    for (integer, fraction, unit, expected) in [
        ("1", "000", "s", "واحد فاصلة صفر صفر صفر ثانية"),
        ("2", "000", "s", "اثنان فاصلة صفر صفر صفر ثانية"),
        ("2", "00", "min", "اثنان فاصلة صفر صفر دقيقة"),
    ] {
        let source = format!("{integer}.{fraction} {unit}");
        let result = single(
            SpeechLocale::Ar,
            &source,
            SpeechValue::Measure {
                unit: unit.into(),
                value: n(integer, Some(fraction)),
            },
        );
        assert_eq!(result.text, expected, "{source}");
        assert!(result.fully_supported);
        assert!(result.readings[0].reason.is_none());
    }
    for (integer, expected) in [("1", "ثانية"), ("2", "ثانيتان")] {
        let result = single(
            SpeechLocale::Ar,
            &format!("{integer} s"),
            SpeechValue::Measure {
                unit: "s".into(),
                value: n(integer, None),
            },
        );
        assert_eq!(result.text, expected);
    }
    let mut negative = n("1", Some("000"));
    negative.negative = true;
    let result = single(
        SpeechLocale::Ar,
        "-1.000 s",
        SpeechValue::Measure {
            unit: "s".into(),
            value: negative,
        },
    );
    assert_eq!(result.text, "سالب واحد فاصلة صفر صفر صفر ثانية");
}

#[test]
fn dates_handle_language_specific_ordinals_and_counter_pronunciations() {
    for (locale, year, month, day, expected) in [
        (
            SpeechLocale::En,
            2024,
            11,
            13,
            "November thirteenth, two thousand twenty-four",
        ),
        (
            SpeechLocale::Fr,
            2024,
            1,
            1,
            "premier janvier deux mille vingt-quatre",
        ),
        (
            SpeechLocale::De,
            2024,
            11,
            13,
            "dreizehnter November zweitausendvierundzwanzig",
        ),
        (
            SpeechLocale::Ja,
            2024,
            4,
            24,
            "にせんにじゅうよねんしがつにじゅうよっか",
        ),
        (SpeechLocale::Ko, 2024, 6, 1, "이천이십사년 유월 일일"),
        (SpeechLocale::Ko, 2024, 10, 1, "이천이십사년 시월 일일"),
        (
            SpeechLocale::Ru,
            2024,
            11,
            13,
            "тринадцатое ноября две тысячи двадцать четвёртого года",
        ),
    ] {
        let r = single(locale, "date", SpeechValue::Date { year, month, day });
        assert!(r.fully_supported, "{locale:?} {r:?}");
        assert_eq!(r.text, expected, "{locale:?}");
    }
}

#[test]
fn times_have_validated_24_hour_fields_and_counter_readings() {
    for (locale, hour, minute, second, expected) in [
        (SpeechLocale::Es, 1, 2, None, "una hora y dos minutos"),
        (
            SpeechLocale::En,
            13,
            5,
            Some(1),
            "thirteen hours and five minutes and one second",
        ),
        (SpeechLocale::Ja, 4, 1, None, "よじいっぷん"),
        (SpeechLocale::Ja, 7, 10, None, "しちじじゅっぷん"),
        (SpeechLocale::Zh, 2, 5, None, "两点五分"),
        (SpeechLocale::Ko, 13, 30, None, "열세 시 삼십 분"),
        (
            SpeechLocale::Ar,
            13,
            5,
            None,
            "الساعة الثالثة عشرة وخمس دقائق",
        ),
    ] {
        let r = single(
            locale,
            "time",
            SpeechValue::Time {
                hour,
                minute,
                second,
            },
        );
        assert_eq!(r.text, expected, "{locale:?}");
        assert!(r.fully_supported, "{r:?}");
    }
    for (hour, minute, second) in [(24, 0, None), (9, 60, None), (9, 0, Some(60))] {
        let r = single(
            SpeechLocale::En,
            "invalid",
            SpeechValue::Time {
                hour,
                minute,
                second,
            },
        );
        assert!(!r.fully_supported);
        assert_eq!(r.text, "invalid");
    }
}

#[test]
fn money_reads_minor_units_and_preserves_unhandled_precision_or_currency() {
    for (locale, code, integer, fraction, expected) in [
        (
            SpeechLocale::Es,
            "PEN",
            "1",
            Some("5"),
            "un sol con cincuenta céntimos",
        ),
        (
            SpeechLocale::Es,
            "EUR",
            "1",
            Some("01"),
            "un euro con un céntimo",
        ),
        (SpeechLocale::Es, "GBP", "1", None, "una libra esterlina"),
        (
            SpeechLocale::En,
            "USD",
            "12",
            Some("50"),
            "twelve US dollars and fifty cents",
        ),
        (
            SpeechLocale::En,
            "GBP",
            "1",
            Some("01"),
            "one British pound and one penny",
        ),
        (SpeechLocale::Ar, "USD", "1", None, "دولار واحد"),
    ] {
        let r = single(
            locale,
            "money",
            SpeechValue::Currency {
                code: code.into(),
                value: n(integer, fraction),
            },
        );
        assert_eq!(r.text, expected, "{locale:?} {code}");
        assert!(r.fully_supported, "{r:?}");
    }
    for (locale, code, fraction) in [
        (SpeechLocale::Es, "PEN", Some("123")),
        (SpeechLocale::En, "ZZZ", None),
        (SpeechLocale::En, "JPY", Some("01")),
    ] {
        let r = single(
            locale,
            "original",
            SpeechValue::Currency {
                code: code.into(),
                value: n("1", fraction),
            },
        );
        assert!(!r.fully_supported);
        assert_eq!(r.text, "original");
        assert!(r.readings[0].reason.is_some());
    }
}

#[test]
fn percent_and_basic_measure_have_real_readings_in_every_locale() {
    for (locale, expected) in [
        (SpeechLocale::En, "three percent"),
        (SpeechLocale::Es, "tres por ciento"),
        (SpeechLocale::Zh, "百分之三"),
        (SpeechLocale::Hi, "तीन प्रतिशत"),
        (SpeechLocale::Ar, "ثلاثة بالمائة"),
        (SpeechLocale::Fr, "trois pour cent"),
        (SpeechLocale::Pt, "três por cento"),
        (SpeechLocale::Ru, "три процента"),
        (SpeechLocale::Bn, "তিন শতাংশ"),
        (SpeechLocale::Ur, "تین فیصد"),
        (SpeechLocale::Id, "tiga persen"),
        (SpeechLocale::De, "drei Prozent"),
        (SpeechLocale::Ja, "さん パーセント"),
        (SpeechLocale::Pa, "ਤਿੰਨ ਪ੍ਰਤਿਸ਼ਤ"),
        (SpeechLocale::Mr, "तीन टक्के"),
        (SpeechLocale::Te, "మూడు శాతం"),
        (SpeechLocale::Tr, "yüzde üç"),
        (SpeechLocale::Ta, "மூன்று சதவீதம்"),
        (SpeechLocale::Vi, "ba phần trăm"),
        (SpeechLocale::Ko, "삼 퍼센트"),
    ] {
        let percent = single(
            locale,
            "3%",
            SpeechValue::Percent {
                value: n("3", None),
            },
        );
        assert!(percent.fully_supported, "{locale:?}: {percent:?}");
        assert_eq!(percent.text, expected, "{locale:?}");
        assert!(!percent.text.contains(['%', '٪']));
        let measure = single(
            locale,
            "3 kg",
            SpeechValue::Measure {
                unit: "kg".into(),
                value: n("3", None),
            },
        );
        assert!(measure.fully_supported, "{locale:?}: {measure:?}");
        assert_ne!(measure.text, "3 kg");
        assert!(!measure.text.contains("{0}"));
    }
    assert_eq!(
        single(
            SpeechLocale::Es,
            "12%",
            SpeechValue::Percent {
                value: n("12", None)
            }
        )
        .text,
        "doce por ciento"
    );
    assert_eq!(
        single(
            SpeechLocale::En,
            "12%",
            SpeechValue::Percent {
                value: n("12", None)
            }
        )
        .text,
        "twelve percent"
    );
}

#[test]
fn unsupported_tokens_preserve_text_offsets_and_explain_limits() {
    let text = "Hi 007!";
    let document = SpeechDocument {
        text: text.into(),
        locale: SpeechLocale::En,
        spans: vec![
            SpeechSpan {
                start: 0,
                end: 3,
                source: "Hi ".into(),
                value: SpeechValue::Text,
            },
            SpeechSpan {
                start: 3,
                end: 6,
                source: "007".into(),
                value: SpeechValue::Identifier {
                    kind: "code".into(),
                },
            },
            SpeechSpan {
                start: 6,
                end: 7,
                source: "!".into(),
                value: SpeechValue::Text,
            },
        ],
    };
    let r = verbalize_document(&document).unwrap();
    assert_eq!(r.text, text);
    assert!(!r.fully_supported);
    assert!(r.readings[0].supported);
    assert!(!r.readings[1].supported);
    assert_eq!((r.readings[1].start, r.readings[1].end), (3, 6));
    assert!(r.readings[1].reason.as_ref().unwrap().contains("policy"));
    let digits = single(
        SpeechLocale::Es,
        "007",
        SpeechValue::Number {
            value: n("007", None),
            reading: NumberReading::Digits,
        },
    );
    assert_eq!(digits.text, "cero cero siete");
    assert!(digits.fully_supported);
}

#[test]
fn public_documents_are_validated_before_any_slicing_or_replacement() {
    for (text, start, end, source) in [
        ("é", 0, 1, "x"),
        ("abc", 1, 3, "bc"),
        ("abc", 0, 5, "abc"),
        ("abc", 0, 3, "wrong"),
        ("abc", 0, 0, ""),
    ] {
        let d = SpeechDocument {
            text: text.into(),
            locale: SpeechLocale::En,
            spans: vec![SpeechSpan {
                start,
                end,
                source: source.into(),
                value: SpeechValue::Text,
            }],
        };
        assert!(verbalize_document(&d).is_err(), "{d:?}");
    }
    let empty = SpeechDocument {
        text: String::new(),
        locale: SpeechLocale::En,
        spans: vec![],
    };
    assert!(verbalize_document(&empty).unwrap().fully_supported);
    let invalid = single(
        SpeechLocale::En,
        "2024-02-30",
        SpeechValue::Date {
            year: 2024,
            month: 2,
            day: 30,
        },
    );
    assert!(!invalid.fully_supported);
    assert_eq!(invalid.text, "2024-02-30");
}
