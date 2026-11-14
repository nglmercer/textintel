use textintel::speech::normalization::{
    self as tn, ChangeKind, NormalizationMode, NormalizeLimits, NormalizeOptions, WarningCode,
};
use textintel::speech::{self, DateOrder, ReadingStyle, SpeechLocale, SpeechOptions};

fn options(language: &str, locale: Option<&str>) -> NormalizeOptions {
    NormalizeOptions::resolve(
        NormalizationMode::Auto,
        language,
        locale,
        NormalizeLimits::default(),
        true,
    )
    .unwrap()
}

#[test]
fn common_exact_number_parser_keeps_versioned_ambiguity_and_precision_policies() {
    let en = options("en", Some("en-US"));
    let es = options("es", Some("es-ES"));
    for source in ["--3e2", "-+3e2", "−−3e2", "--3.5e2"] {
        assert_eq!(tn::normalize(source, &en).unwrap().spoken, source);
        assert_eq!(tn::normalize(source, &es).unwrap().spoken, source);
    }
    assert_eq!(
        tn::normalize("1,234.05", &en).unwrap().spoken,
        "one thousand two hundred thirty-four point zero five"
    );
    assert_eq!(
        tn::normalize("0.0001", &es).unwrap().spoken,
        "cero coma cero cero cero uno"
    );
    let ambiguous = tn::normalize("3.14", &es).unwrap();
    assert_eq!(ambiguous.spoken, "3.14");
    assert!(
        ambiguous
            .warnings
            .iter()
            .any(|w| w.code == WarningCode::AmbiguousNumber)
    );
    for source in ["1000000000000000", "1.1234567890123"] {
        let large = tn::normalize(source, &en).unwrap();
        assert_eq!(large.spoken, source);
        assert!(
            large
                .warnings
                .iter()
                .any(|w| w.code == WarningCode::NumberTooLarge)
        );
    }
    for source in [
        "v1.2.3",
        "user@example.org",
        "https://example.org/2024-12-31",
        "`3.14 2024-12-31`",
    ] {
        assert_eq!(tn::normalize(source, &en).unwrap().spoken, source);
    }
}

#[test]
fn oversized_fraction_and_duration_counts_preserve_input_without_panicking() {
    for (language, locale) in [("en", "en-US"), ("es", "es-ES")] {
        let mut opts = options(language, Some(locale));
        opts.reading_style = ReadingStyle::Natural;
        for source in [
            "18446744073709551615/2",
            "18446744073709551616/2",
            "18446744073709551615h",
            "1h18446744073709551615min",
            "1h2min18446744073709551615s",
        ] {
            let result = tn::normalize(source, &opts).unwrap();
            assert_eq!(result.spoken, source, "{language}: {source}: {result:?}");
        }
    }
}

#[test]
fn shared_relation_profiles_keep_legacy_acceptance_and_typed_value_semantics() {
    use speech::expressions::{self, ExpressionValue};
    let mut typed = SpeechOptions::new(SpeechLocale::En);
    typed.reading_style = ReadingStyle::Natural;
    let (_, value) = expressions::parse_at("−2.50–−1.25", 0, &typed).unwrap();
    assert!(
        matches!(&value,ExpressionValue::Range{from,to} if from.negative && from.integer=="2" && from.fraction.as_deref()==Some("50") && to.negative && to.fraction.as_deref()==Some("25"))
    );
    let (_, value) = expressions::parse_at("-3 1/2", 0, &typed).unwrap();
    assert!(
        matches!(&value,ExpressionValue::MixedFraction{whole,numerator,denominator} if whole.negative && whole.integer=="3" && numerator.integer=="1" && denominator.integer=="2")
    );
    assert!(expressions::parse_at("3/0.00", 0, &typed).is_none());
    for lang in ["en", "es"] {
        let mut opts = options(lang, None);
        opts.reading_style = ReadingStyle::Natural;
        for source in ["03–4", "4–3", "3–3"] {
            assert_eq!(tn::normalize(source, &opts).unwrap().spoken, source);
        }
    }
}

#[test]
fn shared_calendar_resolution_preserves_locale_precedence_and_invalid_dates() {
    let us = options("en", Some("en-US"));
    let gb = options("en", Some("en-GB"));
    assert!(
        tn::normalize("31/12/2024", &us)
            .unwrap()
            .spoken
            .contains("December")
    );
    assert!(
        tn::normalize("03/04/2024", &us)
            .unwrap()
            .spoken
            .contains("March")
    );
    assert!(
        tn::normalize("03/04/2024", &gb)
            .unwrap()
            .spoken
            .contains("April")
    );
    let implicit = tn::normalize("03/04/2024", &options("en", None)).unwrap();
    assert_eq!(implicit.spoken, "03/04/2024");
    assert!(
        implicit
            .warnings
            .iter()
            .any(|w| w.code == WarningCode::AmbiguousDate)
    );
    for source in ["29/02/2023", "2024-04-31"] {
        let invalid = tn::normalize(source, &us).unwrap();
        assert_eq!(invalid.spoken, source);
        assert!(
            invalid
                .warnings
                .iter()
                .any(|w| w.code == WarningCode::InvalidDate)
        );
    }
}

#[test]
fn japanese_and_korean_locale_metadata_follows_cldr_numeric_year_month_day() {
    // CLDR48 ca-gregorian dateFormats: ja short y/MM/dd, ko short yy. M. d.
    // Source pin4d06be52b51bb2f75688d0abe55c52a66afed790.
    for (language, locale, expected) in [
        ("ja", "ja-JP", "二千二十四年十二月三十一日"),
        ("ko", "ko-KR", "이천이십사 년 십이월 삼십일 일"),
    ] {
        assert_eq!(
            speech::locales::for_language(language).unwrap().date_order,
            DateOrder::YearMonthDay
        );
        let resolved = options(language, Some(locale));
        assert_eq!(resolved.locale, locale);
        let result = tn::normalize("2024/12/31", &resolved).unwrap();
        assert_eq!(result.spoken, expected);
        assert_eq!(result.changes[0].kind, ChangeKind::Date);
        assert_eq!((result.changes[0].start, result.changes[0].end), (0, 10));
    }
}

#[test]
fn shared_clock_validation_and_span_joining_preserve_twelve_hour_context() {
    let en = options("en", Some("en-US"));
    let valid = tn::normalize("At 1:05 PM, continue.", &en).unwrap();
    assert_eq!(valid.spoken, "At one oh five P M, continue.");
    for source in ["13:05 PM", "24:00", "12:60"] {
        let bad = tn::normalize(source, &en).unwrap();
        assert_eq!(bad.spoken, source);
        assert!(
            bad.warnings
                .iter()
                .any(|w| w.code == WarningCode::AmbiguousTime)
        );
    }
    let mut spanish = SpeechOptions::new(SpeechLocale::Es);
    spanish.reading_style = ReadingStyle::Natural;
    assert_eq!(
        speech::normalize_for_speech("21:00", &spanish)
            .unwrap()
            .text,
        "veintiuna"
    );
}

#[test]
fn versioned_expression_adapters_keep_decimal_styles_and_duration_presentation() {
    for style in [ReadingStyle::Precise, ReadingStyle::Natural] {
        let mut en = options("en", Some("en-US"));
        en.reading_style = style;
        for (source, expected) in [
            (
                "1,234e2",
                "one point two three four times ten to the power of two",
            ),
            ("003e3", "three times ten to the power of three"),
            ("1e−03", "one times ten to the power of minus three"),
        ] {
            let result = tn::normalize(source, &en).unwrap();
            assert_eq!(result.spoken, expected, "{source}");
            assert_eq!(result.changes[0].kind, ChangeKind::Decimal);
        }
        let mut es = options("es", Some("es-ES"));
        es.reading_style = style;
        assert_eq!(
            tn::normalize("2.5e-3", &es).unwrap().spoken,
            "dos coma cinco por diez elevado a menos tres"
        );
    }
    let mut en = options("en", Some("en-US"));
    en.reading_style = ReadingStyle::Natural;
    for (source, expected) in [
        ("1h30", "one hour and thirty minutes"),
        ("1h30sec", "one hour and thirty seconds"),
        ("1h0min15s", "one hour and fifteen seconds"),
        ("90min", "ninety minutes"),
    ] {
        let result = tn::normalize(source, &en).unwrap();
        assert_eq!(result.spoken, expected, "{source}");
        assert_eq!(result.changes[0].kind, ChangeKind::Unit);
    }
    for source in [
        "`1e3 1h30`",
        "https://example.org/1e3",
        "user1h30@example.org",
    ] {
        assert_eq!(tn::normalize(source, &en).unwrap().spoken, source);
    }
}
