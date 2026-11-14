use textintel::speech::{
    NumberValue, SpeechInterpretation, SpeechLocale, SpeechOptions, SpeechValue,
    grammar::{self, NumberInflection},
    normalize_for_speech, parse_for_speech,
};

fn options() -> SpeechOptions {
    let mut opts = SpeechOptions::new(SpeechLocale::Ru);
    opts.interpret_as = SpeechInterpretation::Measure;
    opts
}
fn inflected(integer: &str, negative: bool) -> SpeechValue {
    SpeechValue::InflectedNumber {
        value: NumberValue {
            negative,
            integer: integer.into(),
            fraction: None,
        },
        inflection: NumberInflection::Genitive,
    }
}
#[test]
fn explicit_genitive_suffixes_are_typed_and_not_rendered_as_literal_labels() {
    for (text, integer, spoken) in [
        ("2-х", "2", "двух"),
        ("3-х", "3", "трёх"),
        ("4-х", "4", "четырёх"),
        ("24-х", "24", "двадцати четырёх"),
        ("1000024-х", "1000024", "одного миллиона двадцати четырёх"),
    ] {
        let (end, value) = grammar::parse_at(text, 0, &options()).unwrap();
        assert_eq!(end, text.len());
        assert_eq!(value, inflected(integer, false));
        assert_eq!(grammar::render(&value, &options()).as_deref(), Some(spoken));
    }
    let source = "4-х килограммовый мешок";
    let normalized = normalize_for_speech(source, &options()).unwrap();
    assert_eq!(normalized.text, "четырёх килограммовый мешок");
    assert!(normalized.fully_supported, "{normalized:?}");
}
#[test]
fn auto_identifiers_and_other_languages_do_not_infer_russian_case() {
    for hint in [
        SpeechInterpretation::Auto,
        SpeechInterpretation::Currency,
        SpeechInterpretation::Date,
    ] {
        let mut opts = options();
        opts.interpret_as = hint;
        assert!(grammar::parse_at("24-х", 0, &opts).is_none());
    }
    let mut other = options();
    other.locale = SpeechLocale::En;
    assert!(grammar::parse_at("24-х", 0, &other).is_none());
    assert!(grammar::render(&inflected("24", false), &other).is_none());
    let parsed = parse_for_speech("24-х", &SpeechOptions::new(SpeechLocale::Ru)).unwrap();
    assert!(
        !parsed
            .spans
            .iter()
            .any(|s| matches!(s.value, SpeechValue::InflectedNumber { .. }))
    );
}
#[test]
fn ambiguous_or_invalid_case_suffixes_are_preserved_whole() {
    for source in [
        "12-х",
        "13-х",
        "14-х",
        "200-х",
        "31-х",
        "24.0-х",
        "1000000000000002-х",
    ] {
        let (end, value) = grammar::parse_at(source, 0, &options()).unwrap();
        assert_eq!(end, source.len());
        assert!(
            matches!(value, SpeechValue::Ambiguous { .. }),
            "{source}: {value:?}"
        );
    }
    for source in ["4-x", "4-Х", "4-м", "24-хabc", "24-х-код"] {
        assert!(
            grammar::parse_at(source, 0, &options()).is_none(),
            "{source}"
        );
    }
}
#[test]
fn genitive_composition_handles_case_specific_hundreds_and_scale_agreement() {
    for (integer, gold) in [
        ("0", "ноля"),
        ("1", "одного"),
        ("11", "одиннадцати"),
        ("40", "сорока"),
        ("90", "девяноста"),
        ("100", "ста"),
        ("200", "двухсот"),
        ("324", "трёхсот двадцати четырёх"),
        ("999", "девятисот девяноста девяти"),
        ("1000", "одной тысячи"),
        ("2000", "двух тысяч"),
        ("21000", "двадцати одной тысячи"),
        ("111000", "ста одиннадцати тысяч"),
        ("1001001", "одного миллиона одной тысячи одного"),
        ("2002004", "двух миллионов двух тысяч четырёх"),
        ("1000000000000", "одного триллиона"),
    ] {
        assert_eq!(
            grammar::render(&inflected(integer, false), &options()).as_deref(),
            Some(gold),
            "{integer}"
        );
    }
    assert_eq!(
        grammar::render(&inflected("24", true), &options()).as_deref(),
        Some("минус двадцати четырёх")
    );
}
#[test]
fn public_genitive_values_validate_numeric_shape_and_bounds() {
    for integer in [
        "",
        "x",
        "1.2",
        "-24",
        "1000000000000000",
        "999999999999999999999999999999999999999999",
    ] {
        assert!(
            grammar::render(&inflected(integer, false), &options()).is_none(),
            "{integer}"
        );
    }
    let fractional = SpeechValue::InflectedNumber {
        value: NumberValue {
            negative: false,
            integer: "2".into(),
            fraction: Some("0".into()),
        },
        inflection: NumberInflection::Genitive,
    };
    assert!(grammar::render(&fractional, &options()).is_none());
    assert!(grammar::render(&SpeechValue::Text, &options()).is_none());
    assert!(grammar::render(&inflected("999999999999999", false), &options()).is_some());
}
#[test]
fn byte_boundaries_signs_and_native_digits_reuse_the_shared_parser() {
    let source = "📦 −２４-х.";
    let start = "📦 ".len();
    let (end, value) = grammar::parse_at(source, start, &options()).unwrap();
    assert_eq!(&source[start..end], "−２４-х");
    assert_eq!(value, inflected("24", true));
    assert!(grammar::parse_at(source, 1, &options()).is_none());
    assert!(grammar::parse_at(source, source.len() + 1, &options()).is_none());
    assert!(grammar::parse_at("a24-х", 1, &options()).is_none());
    let mut number = options();
    number.interpret_as = SpeechInterpretation::Number;
    assert!(grammar::parse_at("24-х", 0, &number).is_some());
}
