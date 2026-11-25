use textintel::speech::{
    CurrencyNaming, NumberNotation, ReadingStyle, SpeechLocale, SpeechOptions, SpeechValue,
    normalize_for_speech, parse_for_speech,
};

fn options(locale: SpeechLocale) -> SpeechOptions {
    let mut options = SpeechOptions::new(locale);
    options.decimal_separator = Some('.');
    options
}

#[test]
fn arabic_euro_counts_reuse_the_same_reviewed_paradigm_in_style_and_full_naming() {
    for style in [ReadingStyle::Precise, ReadingStyle::Natural] {
        let mut opts = options(SpeechLocale::Ar);
        opts.reading_style = style;
        for (source, expected) in [
            ("EUR 1", "يورو واحد"),
            ("EUR 1.00", "يورو واحد"),
            ("EUR 2", "يوروان"),
            ("EUR 2.00", "يوروان"),
            ("EUR ٢.٠٠", "يوروان"),
            ("EUR -2.00", "سالب يوروان"),
            ("EUR 2.05", "يوروان وخمسة سنتات"),
            ("EUR -2.50", "سالب يوروان وخمسون سنتا"),
        ] {
            let result = normalize_for_speech(source, &opts).unwrap();
            assert!(result.fully_supported, "{source}: {result:?}");
            assert_eq!(result.text, expected, "{style:?} {source}");
        }
        let parsed = parse_for_speech("EUR 2.00", &opts).unwrap();
        assert!(matches!(
            &parsed.spans[0].value,
            SpeechValue::Currency { code, value }
                if code == "EUR" && value.integer == "2"
                    && value.fraction.as_deref() == Some("00") && !value.negative
        ));
        opts.currency_naming = CurrencyNaming::Full;
        let full = normalize_for_speech("EUR 2.00", &opts).unwrap();
        assert!(full.fully_supported, "{full:?}");
        assert_eq!(full.text, "يوروان");
    }
}

#[test]
fn canonical_units_recognize_attached_native_fullwidth_and_case_variants() {
    for (source, key) in [
        ("12KG", "mass-kilogram"),
        ("2ｋｇ", "mass-kilogram"),
        ("25千克", "mass-kilogram"),
        ("12кг", "mass-kilogram"),
        ("120m²", "area-square-meter"),
        ("20km²", "area-square-kilometer"),
        ("500mg", "mass-milligram"),
        ("1,000028 cm³", "volume-cubic-centimeter"),
        ("2 kwh", "energy-kilowatt-hour"),
    ] {
        let mut options = options(SpeechLocale::En);
        if source.contains(',') {
            options.decimal_separator = Some(',');
        }
        let parsed = parse_for_speech(source, &options).unwrap();
        assert_eq!(parsed.spans.len(), 1, "{source}: {parsed:?}");
        match &parsed.spans[0].value {
            SpeechValue::Measure { unit, .. } => assert_eq!(unit, key, "{source}"),
            other => panic!("{source}: {other:?}"),
        }
        assert_eq!(parsed.spans[0].source, source);
        assert_eq!(
            &parsed.text[parsed.spans[0].start..parsed.spans[0].end],
            source
        );
    }
}

#[test]
fn expanded_unit_dictionaries_are_available_in_all_twenty_languages() {
    for locale in SpeechLocale::ALL {
        let normalized =
            normalize_for_speech("12 kg, 25 mg, 7 cm, 2 m²", &options(locale)).unwrap();
        assert!(normalized.fully_supported, "{locale:?}: {normalized:?}");
        assert!(
            !normalized.text.chars().any(|c| c.is_ascii_digit()),
            "{locale:?}: {}",
            normalized.text
        );
    }
}

#[test]
fn quantities_with_omitted_integer_digits_reuse_the_shared_decimal_parser() {
    for locale in SpeechLocale::ALL {
        let options = options(locale);
        for (source, kind) in [
            ("€.50", "currency"),
            (".50 kg", "measure"),
            (".50%", "percent"),
            ("٪٫٥٠", "percent"),
            ("€٫٥٠", "currency"),
            ("٫५०kg", "measure"),
        ] {
            let parsed = parse_for_speech(source, &options).unwrap();
            assert_eq!(parsed.spans.len(), 1, "{locale:?} {source}: {parsed:?}");
            assert_eq!(parsed.spans[0].source, source);
            let value = match (&parsed.spans[0].value, kind) {
                (SpeechValue::Currency { code, value }, "currency") => {
                    assert_eq!(code, "EUR");
                    value
                }
                (SpeechValue::Measure { unit, value }, "measure") => {
                    assert_eq!(unit, "mass-kilogram");
                    value
                }
                (SpeechValue::Percent { value }, "percent") => value,
                other => panic!("{locale:?} {source}: {other:?}"),
            };
            assert_eq!(value.integer, "0", "{locale:?} {source}");
            assert_eq!(value.fraction.as_deref(), Some("50"), "{locale:?} {source}");
            assert!(!value.negative);
        }
    }
    let mut options = options(SpeechLocale::En);
    options.reading_style = ReadingStyle::Natural;
    for (source, expected) in [
        ("€.50", "fifty cents"),
        (".50 kg", "zero point five zero kilograms"),
        (".50%", "zero point five zero percent"),
    ] {
        let result = normalize_for_speech(source, &options).unwrap();
        assert_eq!(result.text, expected, "{source}: {result:?}");
        assert!(result.fully_supported, "{source}: {result:?}");
    }
}

#[test]
fn omitted_integer_digits_do_not_override_separator_or_identifier_policy() {
    let mut options = options(SpeechLocale::En);
    options.decimal_separator = Some(',');
    for source in ["€,50", ",50kg", ",50%"] {
        let parsed = parse_for_speech(source, &options).unwrap();
        assert_eq!(parsed.spans.len(), 1, "{source}: {parsed:?}");
        let value = match &parsed.spans[0].value {
            SpeechValue::Currency { value, .. }
            | SpeechValue::Measure { value, .. }
            | SpeechValue::Percent { value } => value,
            other => panic!("{source}: {other:?}"),
        };
        assert_eq!(value.integer, "0");
        assert_eq!(value.fraction.as_deref(), Some("50"));
    }
    for source in [".", ",", "€.", "€.50.2", "€,50,2", "part.50kg", "v.50"] {
        let parsed = parse_for_speech(source, &options).unwrap();
        assert!(
            !parsed.spans.iter().any(|span| matches!(
                span.value,
                SpeechValue::Currency { .. }
                    | SpeechValue::Measure { .. }
                    | SpeechValue::Percent { .. }
            )),
            "{source}: {parsed:?}"
        );
    }
    // A comma after prose can delimit a separate integer quantity; do not
    // absorb punctuation across the left word boundary as a decimal mark.
    let parsed = parse_for_speech("tag,50kg", &options).unwrap();
    assert!(parsed.spans.iter().any(|span| matches!(
        &span.value,
        SpeechValue::Measure { value, .. } if value.integer == "50" && value.fraction.is_none()
    )));
}

#[test]
fn compound_unit_parsing_keeps_the_denominator_and_reciprocal() {
    for (source, expected) in [
        ("12/kg", "twelve per kilogram"),
        ("12kg/kg", "twelve kilograms per kilogram"),
        ("3 km/h", "three kilometers per hour"),
    ] {
        let normalized = normalize_for_speech(source, &options(SpeechLocale::En)).unwrap();
        assert_eq!(normalized.text, expected);
        assert!(normalized.fully_supported);
    }
}

#[test]
fn currency_symbols_need_explicit_policy_but_explicit_codes_and_names_do_not() {
    let mut options = options(SpeechLocale::En);
    let ambiguous = normalize_for_speech("$2", &options).unwrap();
    assert_eq!(ambiguous.text, "$2");
    assert!(!ambiguous.fully_supported);
    options.default_currency = Some("USD".into());
    options.reading_style = ReadingStyle::Natural;
    assert_eq!(
        normalize_for_speech("$2", &options).unwrap().text,
        "two dollars"
    );
    for source in ["2 usd", "USD2", "2美元"] {
        let parsed = parse_for_speech(source, &options).unwrap();
        assert!(
            matches!(&parsed.spans[0].value,SpeechValue::Currency{code,..} if code=="USD"),
            "{source}: {parsed:?}"
        );
    }
}

#[test]
fn native_currency_suffixes_and_grouped_amounts_keep_exact_numeric_values() {
    for (locale, source, code, integer) in [
        (SpeechLocale::Ko, "KRW 1,230,000", "KRW", "1230000"),
        (SpeechLocale::Ja, "-12,345円", "JPY", "12345"),
        (SpeechLocale::Vi, "10 đồng", "VND", "10"),
        (SpeechLocale::Ar, "5د.ك", "KWD", "5"),
        (SpeechLocale::Pt, "R$ 12", "BRL", "12"),
    ] {
        let parsed = parse_for_speech(source, &options(locale)).unwrap();
        assert_eq!(parsed.spans.len(), 1, "{source}: {parsed:?}");
        let SpeechValue::Currency {
            code: actual,
            value,
        } = &parsed.spans[0].value
        else {
            panic!("{source}: {parsed:?}")
        };
        assert_eq!(actual, code);
        assert_eq!(value.integer, integer);
        assert_eq!(value.negative, source.starts_with('-'));
    }
}

#[test]
fn russian_explicit_us_qualifier_is_spoken_once_after_the_complete_amount() {
    let mut options = options(SpeechLocale::Ru);
    options.reading_style = ReadingStyle::Natural;
    for (source, expected) in [
        ("47 долл. США", "сорок семь долларов сэ шэ а"),
        ("-23 долл. США", "минус двадцать три доллара сэ шэ а"),
        (
            "6.21 долл. США",
            "шесть долларов двадцать один цент сэ шэ а",
        ),
        ("-2.04 долл. США", "минус два доллара четыре цента сэ шэ а"),
        ("0.11 долл. США", "одиннадцать центов сэ шэ а"),
        ("2 млн долл. США", "два миллиона долларов сэ шэ а"),
        (
            "-2.5 млн долл. США",
            "минус две целых пять десятых миллиона долларов сэ шэ а",
        ),
        ("долл. США 13", "тринадцать долларов сэ шэ а"),
    ] {
        let normalized = normalize_for_speech(source, &options).unwrap();
        assert_eq!(normalized.text, expected, "{source}: {normalized:?}");
        assert!(normalized.fully_supported, "{source}: {normalized:?}");
        assert_eq!(normalized.text.matches("сэ шэ а").count(), 1);
    }
    // Precise names already include the written CLDR qualifier; pronounce it
    // rather than retaining the raw abbreviation and adding it a second time.
    options.reading_style = ReadingStyle::Precise;
    assert_eq!(
        normalize_for_speech("3 долл. США", &options).unwrap().text,
        "три доллара сэ шэ а"
    );
}

#[test]
fn russian_us_qualifier_does_not_change_unqualified_or_other_currencies() {
    let mut options = options(SpeechLocale::Ru);
    options.reading_style = ReadingStyle::Natural;
    options.default_currency = Some("USD".into());
    for (source, expected) in [
        ("$7", "семь долларов"),
        ("7 USD", "семь долларов"),
        ("7 CAD", "семь канадских долларов"),
        ("7 руб.", "семь рублей"),
    ] {
        assert_eq!(
            normalize_for_speech(source, &options).unwrap().text,
            expected,
            "{source}"
        );
    }
    for locale in SpeechLocale::ALL {
        if locale == SpeechLocale::Ru {
            continue;
        }
        let mut options = SpeechOptions::new(locale);
        options.reading_style = ReadingStyle::Natural;
        assert_eq!(
            normalize_for_speech("7 долл. США", &options).unwrap().text,
            normalize_for_speech("7 USD", &options).unwrap().text,
            "{locale:?}"
        );
    }
}

#[test]
fn full_currency_names_retain_independent_known_country_qualifiers() {
    for (locale, expected) in [
        (SpeechLocale::En, "seven US dollars"),
        (SpeechLocale::Es, "siete dólares estadounidenses"),
        (SpeechLocale::Hi, "सात यूएस डॉलर"),
        (SpeechLocale::Ar, "سبعة دولارات أمريكية"),
        (SpeechLocale::De, "sieben US-Dollar"),
        (SpeechLocale::Vi, "bảy đô la Mỹ"),
        (SpeechLocale::Ru, "семь долларов сэ шэ а"),
    ] {
        let mut options = options(locale);
        options.reading_style = ReadingStyle::Natural;
        let short = normalize_for_speech("US$7", &options).unwrap();
        options.currency_naming = CurrencyNaming::Full;
        let full = normalize_for_speech("US$7", &options).unwrap();
        assert_eq!(full.text, expected, "{locale:?}: {full:?}");
        assert!(full.fully_supported, "{locale:?}: {full:?}");
        assert_ne!(short.text, full.text, "{locale:?}");
    }
}

#[test]
fn full_currency_dictionary_path_keeps_identity_in_all_twenty_languages() {
    use textintel::speech::verbalize_document_with_options;
    let lexicons: serde_json::Value =
        serde_json::from_str(include_str!("../src/speech/data/lexicons.json")).unwrap();
    for locale in SpeechLocale::ALL {
        let mut options = options(locale);
        options.reading_style = ReadingStyle::Natural;
        for source in ["USD7", "USD-0.05"] {
            let parsed = parse_for_speech(source, &options).unwrap();
            assert_eq!(parsed.spans.len(), 1, "{locale:?} {source}: {parsed:?}");
            assert!(matches!(
                &parsed.spans[0].value,
                SpeechValue::Currency { code, .. } if code == "USD"
            ));
            let mut full_options = options.clone();
            full_options.currency_naming = CurrencyNaming::Full;
            assert_eq!(
                parse_for_speech(source, &full_options).unwrap(),
                parsed,
                "naming must not alter parsing: {locale:?} {source}"
            );
            let full = verbalize_document_with_options(&parsed, &full_options).unwrap();
            assert!(full.fully_supported, "{locale:?} {source}: {full:?}");
            if locale == SpeechLocale::Ru {
                assert_eq!(full.text.matches("сэ шэ а").count(), 1, "{full:?}");
                assert!(!full.text.contains("США"), "{full:?}");
            } else if locale == SpeechLocale::Ar {
                assert!(full.text.contains("أمريكي"), "{full:?}");
            } else {
                let names = lexicons[locale.code()]["currencies"]["USD"]
                    .as_object()
                    .unwrap();
                assert!(
                    names.iter().any(|(key, name)| {
                        key.starts_with("displayName-count-")
                            && name.as_str().is_some_and(|name| full.text.contains(name))
                    }),
                    "{locale:?} {source}: {full:?}"
                );
            }
            assert_eq!(full.readings[0].source, source);
            assert_eq!(full.readings[0].start, 0);
            assert_eq!(full.readings[0].end, source.len());
        }
    }
}

#[test]
fn full_currency_names_preserve_sign_scales_and_minor_only_identity() {
    for (locale, source, expected) in [
        (
            SpeechLocale::En,
            "USD-0.05",
            "minus zero US dollars five cents",
        ),
        (SpeechLocale::En, "CAD7", "seven Canadian dollars"),
        (
            SpeechLocale::En,
            "KWD0.125",
            "zero point one two five Kuwaiti dinars",
        ),
        (
            SpeechLocale::En,
            "USD-2 million",
            "minus two million US dollars",
        ),
        (
            SpeechLocale::Es,
            "USD-0.01",
            "menos cero dólares estadounidenses un centavo",
        ),
        (
            SpeechLocale::Ru,
            "USD-0.21",
            "минус ноль долларов двадцать один цент сэ шэ а",
        ),
        (SpeechLocale::Ru, "USD2", "два доллара сэ шэ а"),
        (
            SpeechLocale::Ru,
            "6.21 долл. США",
            "шесть долларов двадцать один цент сэ шэ а",
        ),
        (
            SpeechLocale::Ru,
            "2 млн USD",
            "два миллиона долларов сэ шэ а",
        ),
        (SpeechLocale::Ru, "RUB-7", "минус семь российских рублей"),
        (SpeechLocale::Ar, "USD1", "دولار أمريكي واحد"),
        (SpeechLocale::Ar, "USD2", "دولاران أمريكيان"),
        (SpeechLocale::Ar, "USD3", "ثلاثة دولارات أمريكية"),
        (SpeechLocale::Ar, "USD11", "أحد عشر دولارا أمريكيا"),
        (
            SpeechLocale::Ar,
            "USD-0.05",
            "سالب صفر دولار أمريكي وخمسة سنتات",
        ),
        (SpeechLocale::Ar, "CAD3", "ثلاثة دولارات كندية"),
        (SpeechLocale::Ar, "KWD2", "ديناران كويتيان"),
    ] {
        let mut options = options(locale);
        options.reading_style = ReadingStyle::Natural;
        options.currency_naming = CurrencyNaming::Full;
        let full = normalize_for_speech(source, &options).unwrap();
        assert!(full.fully_supported, "{locale:?} {source}: {full:?}");
        assert_eq!(full.text, expected, "{locale:?} {source}: {full:?}");
    }
    // Full naming also works with the established precise numeric style.
    let mut options = options(SpeechLocale::Ru);
    options.currency_naming = CurrencyNaming::Full;
    assert_eq!(
        normalize_for_speech("USD3", &options).unwrap().text,
        "три доллара сэ шэ а"
    );
}

#[test]
fn full_currency_naming_keeps_unsupported_precision_and_unreviewed_grammar_visible() {
    for (locale, source) in [(SpeechLocale::Ja, "JPY0.5"), (SpeechLocale::Ko, "KRW0.5")] {
        let mut options = options(locale);
        options.reading_style = ReadingStyle::Natural;
        options.currency_naming = CurrencyNaming::Full;
        let full = normalize_for_speech(source, &options).unwrap();
        assert!(!full.fully_supported, "{locale:?}: {full:?}");
        assert_eq!(full.text, source);
    }
    let mut options = options(SpeechLocale::Ar);
    options.reading_style = ReadingStyle::Natural;
    options.currency_naming = CurrencyNaming::Full;
    let full = normalize_for_speech("KWD0.125", &options).unwrap();
    assert!(full.fully_supported, "{full:?}");
    assert!(full.text.contains("دينار كويتي"), "{full:?}");
    assert!(!full.text.contains("سنت"), "{full:?}");
    let parsed = parse_for_speech("KWD0.125", &options).unwrap();
    assert!(matches!(
        &parsed.spans[0].value,
        SpeechValue::Currency { value, .. } if value.fraction.as_deref() == Some("125")
    ));
}

#[test]
fn reviewed_arabic_ruble_and_franc_paradigms_agree_with_the_amount() {
    // Independent lexical evidence, rather than generated CLDR labels:
    // SKIU / Pyatigorsk (2024), lesson 36, lexical units: рубль روبل (روبلات).
    // WIPO's Arabic certified-document fees explicitly use the dual
    // فرنكان سويسريان; its published fee schedules attest the plural and
    // singular accusative. The numeral agreement follows the same reviewed
    // standalone masculine-quantity rule as dollars, dinars and dirhams.
    // https://www.wipo.int/ar/web/hague-system/hague-system-certified-documents
    // https://www.wipo.int/ar/web/hague-system/w/news/2024/news_0004
    for (source, expected) in [
        ("RUB1", "روبل روسي واحد"),
        ("RUB2", "روبلان روسيان"),
        ("RUB3", "ثلاثة روبلات روسية"),
        ("RUB11", "أحد عشر روبلا روسيا"),
        ("RUB٢١", "واحد وعشرون روبلا روسيا"),
        ("RUB100", "مئة روبل روسي"),
        ("RUB-3", "سالب ثلاثة روبلات روسية"),
        ("RUB3.00", "ثلاثة روبلات روسية"),
        ("CHF1", "فرنك سويسري واحد"),
        ("CHF2", "فرنكان سويسريان"),
        ("CHF3", "ثلاثة فرنكات سويسرية"),
        ("CHF11", "أحد عشر فرنكا سويسريا"),
        ("CHF21", "واحد وعشرون فرنكا سويسريا"),
        ("CHF100", "مئة فرنك سويسري"),
        ("CHF-21.00", "سالب واحد وعشرون فرنكا سويسريا"),
    ] {
        let mut options = options(SpeechLocale::Ar);
        options.reading_style = ReadingStyle::Natural;
        let short_parse = parse_for_speech(source, &options).unwrap();
        options.currency_naming = CurrencyNaming::Full;
        assert_eq!(parse_for_speech(source, &options).unwrap(), short_parse);
        let full = normalize_for_speech(source, &options).unwrap();
        assert!(full.fully_supported, "{source}: {full:?}");
        assert_eq!(full.text, expected, "{source}: {full:?}");
        assert_eq!(full.readings.len(), 1, "{source}: {full:?}");
        assert_eq!(full.readings[0].source, source);
        assert_eq!(
            (full.readings[0].start, full.readings[0].end),
            (0, source.len())
        );
    }

    // A decimal keeps the exact written fraction, never an invented centime
    // or kopek denomination. Precise style reads the decimal digits explicitly.
    for (source, expected) in [
        ("RUB0.125", "صفر فاصلة واحد اثنان خمسة روبل روسي"),
        ("CHF-2.03", "سالب اثنان فاصلة صفر ثلاثة فرنك سويسري"),
    ] {
        let mut options = options(SpeechLocale::Ar);
        options.currency_naming = CurrencyNaming::Full;
        let full = normalize_for_speech(source, &options).unwrap();
        assert!(full.fully_supported, "{source}: {full:?}");
        assert_eq!(full.text, expected, "{source}: {full:?}");
        let parsed = parse_for_speech(source, &options).unwrap();
        assert!(matches!(
            &parsed.spans[0].value,
            SpeechValue::Currency { value, .. }
                if value.fraction.as_deref() == Some(if source.starts_with("RUB") { "125" } else { "03" })
        ));
    }
}

#[test]
fn arabic_full_currency_coverage_is_bounded_by_reviewed_noun_and_qualifier_grammar() {
    let mut options = options(SpeechLocale::Ar);
    options.currency_naming = CurrencyNaming::Full;
    for code in [
        "USD", "GBP", "BRL", "KWD", "SAR", "AED", "EGP", "CAD", "AUD", "NZD", "SGD", "TWD", "RUB",
        "CHF", "INR", "PKR", "IDR", "TRY", "HKD", "PEN", "JPY", "CNY", "ZAR", "MYR", "EUR", "KRW",
        "BDT", "VND", "MXN", "ARS", "CLP", "COP", "NGN", "THB", "PHP", "ILS", "PLN",
    ] {
        let source = format!("{code}3");
        let parsed = parse_for_speech(&source, &options).unwrap();
        assert!(matches!(
            &parsed.spans[0].value,
            SpeechValue::Currency { code: parsed_code, .. } if parsed_code == code
        ));
        let full = normalize_for_speech(&source, &options).unwrap();
        assert!(full.fully_supported, "{code}: {full:?}");
        assert_ne!(full.text, source, "{code}: {full:?}");
    }
}

#[test]
fn arabic_hong_kong_dollar_uses_the_dollar_paradigm_with_an_invariable_place_qualifier() {
    for (source, expected) in [
        ("HKD1", "دولار هونغ كونغ واحد"),
        ("HKD2", "دولاران هونغ كونغ"),
        ("HKD3", "ثلاثة دولارات هونغ كونغ"),
        ("HKD11", "أحد عشر دولارا هونغ كونغ"),
        ("HKD100", "مئة دولار هونغ كونغ"),
    ] {
        let mut options = options(SpeechLocale::Ar);
        options.reading_style = ReadingStyle::Natural;
        options.currency_naming = CurrencyNaming::Full;
        let full = normalize_for_speech(source, &options).unwrap();
        assert!(full.fully_supported, "{source}: {full:?}");
        assert_eq!(full.text, expected, "{source}: {full:?}");
    }
}

#[test]
fn arabic_unterm_reviewed_consonant_final_currency_paradigms_inflect_with_qualifiers() {
    for (source, expected) in [
        ("JPY1", "ين ياباني واحد"),
        ("JPY2", "ينان يابانيان"),
        ("JPY3", "ثلاثة ينات يابانية"),
        ("JPY11", "أحد عشر ينا يابانيا"),
        ("CNY1", "يوان صيني واحد"),
        ("CNY2", "يوانان صينيان"),
        ("CNY3", "ثلاثة يوانات صينية"),
        ("PEN1", "سول بيروفي واحد"),
        ("PEN2", "سولان بيروفيان"),
        ("PEN3", "ثلاثة سولات بيروفية"),
        ("MYR1", "رينغيت ماليزي واحد"),
        ("MYR2", "رينغيتان ماليزيان"),
        ("MYR3", "ثلاثة رينغيتات ماليزية"),
        ("ZAR1", "راند جنوب أفريقيا واحد"),
        ("ZAR2", "راندان جنوب أفريقيا"),
        ("ZAR3", "ثلاثة راندات جنوب أفريقيا"),
        ("ZAR11", "أحد عشر راندا جنوب أفريقيا"),
    ] {
        let mut options = options(SpeechLocale::Ar);
        options.reading_style = ReadingStyle::Natural;
        options.currency_naming = CurrencyNaming::Full;
        let full = normalize_for_speech(source, &options).unwrap();
        assert!(full.fully_supported, "{source}: {full:?}");
        assert_eq!(full.text, expected, "{source}: {full:?}");
    }
}

#[test]
fn arabic_remaining_currency_paradigms_cover_irregular_plural_gender_and_qualifier_agreement() {
    for (source, expected) in [
        ("EUR1", "يورو واحد"),
        ("EUR2", "يوروان"),
        ("EUR3", "ثلاثة يوروهات"),
        ("EUR11", "أحد عشر يورو"),
        ("KRW1", "وون كوريا الجنوبية واحد"),
        ("KRW2", "وونان كوريا الجنوبية"),
        ("KRW3", "ثلاثة وونات كوريا الجنوبية"),
        ("KRW11", "أحد عشر وونا كوريا الجنوبية"),
        ("BDT1", "تاكا بنغلاديشية واحدة"),
        ("BDT2", "تاكاتان بنغلاديشيتان"),
        ("BDT3", "ثلاث تاكات بنغلاديشية"),
        ("BDT11", "إحدى عشرة تاكا بنغلاديشية"),
        ("VND1", "دونج فيتنامي واحد"),
        ("VND2", "دونجان فيتناميان"),
        ("VND3", "ثلاثة دونجات فيتنامية"),
        ("VND11", "أحد عشر دونجا فيتناميا"),
        ("MXN1", "بيزو مكسيكي واحد"),
        ("MXN2", "بيزوان مكسيكيان"),
        ("MXN3", "ثلاثة بيزوات مكسيكية"),
        ("MXN11", "أحد عشر بيزو مكسيكيا"),
        ("ARS3", "ثلاثة بيزوات أرجنتينية"),
        ("CLP3", "ثلاثة بيزوات تشيلية"),
        ("COP3", "ثلاثة بيزوات كولومبية"),
        ("PHP3", "ثلاثة بيزوات فلبينية"),
        ("NGN1", "نيرة نيجيرية واحدة"),
        ("NGN2", "نيرتان نيجيريتان"),
        ("NGN3", "ثلاث نيرات نيجيرية"),
        ("NGN11", "إحدى عشرة نيرة نيجيرية"),
        ("THB1", "بات تايلاندي واحد"),
        ("THB2", "باتان تايلانديان"),
        ("THB3", "ثلاثة بات تايلاندية"),
        ("THB11", "أحد عشر باتا تايلانديا"),
        ("ILS1", "شيكل إسرائيلي جديد واحد"),
        ("ILS2", "شيكلان إسرائيليان جديدان"),
        ("ILS3", "ثلاثة شواكل إسرائيلية جديدة"),
        ("ILS11", "أحد عشر شيكلا إسرائيليا جديدا"),
        ("PLN1", "زلوتي بولندي واحد"),
        ("PLN2", "زلوتيان بولنديان"),
        ("PLN3", "ثلاثة زلوتيات بولندية"),
        ("PLN11", "أحد عشر زلوتيا بولنديا"),
    ] {
        let mut options = options(SpeechLocale::Ar);
        options.reading_style = ReadingStyle::Natural;
        let parsed = parse_for_speech(source, &options).unwrap();
        options.currency_naming = CurrencyNaming::Full;
        assert_eq!(parse_for_speech(source, &options).unwrap(), parsed);
        let full = normalize_for_speech(source, &options).unwrap();
        assert!(full.fully_supported, "{source}: {full:?}");
        assert_eq!(full.text, expected, "{source}: {full:?}");
        assert_eq!(full.readings.len(), 1);
        assert_eq!(full.readings[0].source, source);
        assert_eq!(full.readings[0].end, source.len());
    }
}

#[test]
fn arabic_feminine_full_currency_counts_have_independent_noun_and_qualifier_agreement() {
    for (source, gold) in [
        ("INR0", "صفر روبية هندية"),
        ("INR1", "روبية هندية واحدة"),
        ("INR2", "روبيتان هنديتان"),
        ("INR3", "ثلاث روبيات هندية"),
        ("INR4", "أربع روبيات هندية"),
        ("INR5", "خمس روبيات هندية"),
        ("INR6", "ست روبيات هندية"),
        ("INR7", "سبع روبيات هندية"),
        ("INR8", "ثماني روبيات هندية"),
        ("INR9", "تسع روبيات هندية"),
        ("INR10", "عشر روبيات هندية"),
        ("INR11", "إحدى عشرة روبية هندية"),
        ("INR12", "اثنتا عشرة روبية هندية"),
        ("INR13", "ثلاث عشرة روبية هندية"),
        ("INR14", "أربع عشرة روبية هندية"),
        ("INR15", "خمس عشرة روبية هندية"),
        ("INR16", "ست عشرة روبية هندية"),
        ("INR17", "سبع عشرة روبية هندية"),
        ("INR18", "ثماني عشرة روبية هندية"),
        ("INR19", "تسع عشرة روبية هندية"),
        ("INR21", "إحدى وعشرون روبية هندية"),
        ("INR22", "اثنتان وعشرون روبية هندية"),
        ("INR28", "ثمان وعشرون روبية هندية"),
        ("INR100", "مئة روبية هندية"),
        ("INR103", "مئة وثلاث روبيات هندية"),
        ("INR108", "مئة وثماني روبيات هندية"),
        ("INR118", "مئة وثماني عشرة روبية هندية"),
        ("INR200", "مئتا روبية هندية"),
        ("INR1000", "ألف روبية هندية"),
        ("INR2000", "ألفا روبية هندية"),
        ("INR2003", "ألفان وثلاث روبيات هندية"),
        ("INR2000003", "مليونان وثلاث روبيات هندية"),
        ("INR3003", "ثلاثة آلاف وثلاث روبيات هندية"),
        ("INR2000000", "مليونا روبية هندية"),
        ("INR200000", "مئتا ألف روبية هندية"),
        ("INR200000000", "مئتا مليون روبية هندية"),
        ("INR2000000000", "مليارا روبية هندية"),
        ("INR-0.000", "سالب صفر روبية هندية"),
        ("INR-0003.00", "سالب ثلاث روبيات هندية"),
        ("INR٢٢.٠٠", "اثنتان وعشرون روبية هندية"),
        ("PKR1", "روبية باكستانية واحدة"),
        ("PKR2", "روبيتان باكستانيتان"),
        ("PKR3", "ثلاث روبيات باكستانية"),
        ("IDR1", "روبية إندونيسية واحدة"),
        ("IDR2", "روبيتان إندونيسيتان"),
        ("IDR3", "ثلاث روبيات إندونيسية"),
        ("TRY1", "ليرة تركية واحدة"),
        ("TRY2", "ليرتان تركيتان"),
        ("TRY3", "ثلاث ليرات تركية"),
        ("TRY11", "إحدى عشرة ليرة تركية"),
        ("TRY21", "إحدى وعشرون ليرة تركية"),
    ] {
        let mut options = options(SpeechLocale::Ar);
        options.reading_style = ReadingStyle::Natural;
        let parsed = parse_for_speech(source, &options).unwrap();
        options.currency_naming = CurrencyNaming::Full;
        assert_eq!(parse_for_speech(source, &options).unwrap(), parsed);
        let full = normalize_for_speech(source, &options).unwrap();
        assert!(full.fully_supported, "{source}: {full:?}");
        assert_eq!(full.text, gold, "{source}: {full:?}");
        assert_eq!(full.readings.len(), 1);
        assert_eq!(full.readings[0].source, source);
        assert_eq!(full.readings[0].end, source.len());
    }
    // Contrasts with the still-masculine currency and masculine scale nouns.
    let mut options = options(SpeechLocale::Ar);
    options.reading_style = ReadingStyle::Natural;
    options.currency_naming = CurrencyNaming::Full;
    assert_eq!(
        normalize_for_speech("USD3", &options).unwrap().text,
        "ثلاثة دولارات أمريكية"
    );
    assert_eq!(
        normalize_for_speech("USD21", &options).unwrap().text,
        "واحد وعشرون دولارا أمريكيا"
    );
    assert_eq!(
        normalize_for_speech("USD200", &options).unwrap().text,
        "مئتا دولار أمريكي"
    );
    assert_eq!(
        normalize_for_speech("USD2000", &options).unwrap().text,
        "ألفا دولار أمريكي"
    );
}

#[test]
fn arabic_short_currency_names_reuse_the_unqualified_noun_paradigm_across_currency_aliases() {
    let mut options = options(SpeechLocale::Ar);
    options.reading_style = ReadingStyle::Natural;
    options.currency_naming = CurrencyNaming::Style;
    for (source, gold) in [
        ("CAD1", "دولار واحد"),
        ("AUD2", "دولاران"),
        ("NZD1", "دولار واحد"),
        ("SGD2", "دولاران"),
        ("TWD1", "دولار واحد"),
        ("HKD2", "دولاران"),
        ("BRL1", "ريال واحد"),
        ("EGP2", "جنيهان"),
    ] {
        let result = normalize_for_speech(source, &options).unwrap();
        assert!(result.fully_supported, "{source}: {result:?}");
        assert_eq!(result.text, gold, "{source}: {result:?}");
    }
}

#[test]
fn arabic_full_currency_agreement_matrix_covers_every_bundled_currency_and_count_regime() {
    const CODES: &[&str] = &[
        "USD", "GBP", "BRL", "KWD", "SAR", "AED", "EGP", "CAD", "AUD", "NZD", "SGD", "TWD", "RUB",
        "CHF", "INR", "PKR", "IDR", "TRY", "HKD", "PEN", "JPY", "CNY", "KRW", "ZAR", "MYR", "EUR",
        "BDT", "VND", "MXN", "ARS", "CLP", "COP", "NGN", "THB", "PHP", "ILS", "PLN",
    ];
    assert_eq!(CODES.len(), 37);
    for style in [ReadingStyle::Precise, ReadingStyle::Natural] {
        for naming in [CurrencyNaming::Style, CurrencyNaming::Full] {
            let mut options = options(SpeechLocale::Ar);
            options.reading_style = style;
            options.currency_naming = naming;
            for code in CODES {
                for amount in [
                    "0", "1", "2", "3", "10", "11", "21", "100", "101", "102", "1000", "1001",
                    "1002",
                ] {
                    let source = format!("{code}{amount}");
                    let full = normalize_for_speech(&source, &options).unwrap();
                    assert!(
                        full.fully_supported,
                        "{style:?} {naming:?} {source}: {full:?}"
                    );
                    assert_ne!(full.text, source, "{style:?} {naming:?} {source}: {full:?}");
                    assert_eq!(full.readings[0].source, source);
                }
                if !matches!(*code, "JPY" | "KRW") {
                    for amount in ["0.05", "2.03", "-1.001", "0.125"] {
                        let source = format!("{code}{amount}");
                        let full = normalize_for_speech(&source, &options).unwrap();
                        assert!(
                            full.fully_supported,
                            "{style:?} {naming:?} {source}: {full:?}"
                        );
                        assert_ne!(full.text, source, "{style:?} {naming:?} {source}: {full:?}");
                    }
                }
            }
        }
    }
}

#[test]
fn arabic_feminine_counts_compound_one_and_two_with_masculine_scale_coefficients() {
    for (source, gold) in [
        ("INR101", "مئة وواحدة روبية هندية"),
        ("INR102", "مئة واثنتان روبية هندية"),
        ("INR1001", "ألف وواحدة روبية هندية"),
        ("INR1002", "ألف واثنتان روبية هندية"),
        ("INR101000", "مئة وواحد ألف روبية هندية"),
        ("INR102000000", "مئة واثنان مليون روبية هندية"),
        ("BDT101", "مئة وواحدة تاكا بنغلاديشية"),
        ("NGN102", "مئة واثنتان نيرة نيجيرية"),
    ] {
        for style in [ReadingStyle::Precise, ReadingStyle::Natural] {
            let mut options = options(SpeechLocale::Ar);
            options.reading_style = style;
            options.currency_naming = CurrencyNaming::Full;
            let full = normalize_for_speech(source, &options).unwrap();
            assert!(full.fully_supported, "{style:?} {source}: {full:?}");
            let gold = if style == ReadingStyle::Precise {
                gold.replace("مئة", "مائة")
            } else {
                gold.to_string()
            };
            assert_eq!(full.text, gold, "{style:?} {source}: {full:?}");
        }
    }
}

#[test]
fn arabic_feminine_currency_fractions_use_independent_decimal_readings_and_reviewed_names() {
    for (source, precise, natural) in [
        (
            "INR0.05",
            "صفر فاصلة صفر خمسة روبية هندية",
            "صفر وخمسة من مئة روبية هندية",
        ),
        (
            "INR2.03",
            "اثنان فاصلة صفر ثلاثة روبية هندية",
            "اثنان وثلاثة من مئة روبية هندية",
        ),
        (
            "INR-1.001",
            "سالب واحد فاصلة صفر صفر واحد روبية هندية",
            "سالب واحد وواحد من ألف روبية هندية",
        ),
        (
            "BDT0.05",
            "صفر فاصلة صفر خمسة تاكا بنغلاديشية",
            "صفر وخمسة من مئة تاكا بنغلاديشية",
        ),
        (
            "NGN2.03",
            "اثنان فاصلة صفر ثلاثة نيرة نيجيرية",
            "اثنان وثلاثة من مئة نيرة نيجيرية",
        ),
        (
            "TRY2.03",
            "اثنان فاصلة صفر ثلاثة ليرة تركية",
            "اثنان وثلاثة من مئة ليرة تركية",
        ),
    ] {
        for (style, expected) in [
            (ReadingStyle::Precise, precise),
            (ReadingStyle::Natural, natural),
        ] {
            let mut options = options(SpeechLocale::Ar);
            options.reading_style = style;
            options.currency_naming = CurrencyNaming::Full;
            let parsed = parse_for_speech(source, &options).unwrap();
            assert!(matches!(
                &parsed.spans[0].value,
                SpeechValue::Currency { .. }
            ));
            let full = normalize_for_speech(source, &options).unwrap();
            assert!(full.fully_supported, "{style:?} {source}: {full:?}");
            assert_eq!(full.text, expected, "{style:?} {source}: {full:?}");
            assert_eq!(full.readings[0].source, source);
        }
    }
}

#[test]
fn governed_arabic_time_units_reuse_the_feminine_core_without_the_old_ninety_nine_limit() {
    for style in [ReadingStyle::Precise, ReadingStyle::Natural] {
        let mut options = options(SpeechLocale::Ar);
        options.reading_style = style;
        for (source, gold) in [
            ("8s", "ثماني ثوان"),
            ("18s", "ثماني عشرة ثانية"),
            ("28s", "ثمان وعشرون ثانية"),
            ("8min", "ثماني دقائق"),
            ("18min", "ثماني عشرة دقيقة"),
            ("28min", "ثمان وعشرون دقيقة"),
            ("100s", "مائة ثانية"),
            ("108s", "مائة وثماني ثوان"),
            ("118s", "مائة وثماني عشرة ثانية"),
            ("200s", "مائتا ثانية"),
            ("2000s", "ألفا ثانية"),
            ("2003s", "ألفان وثلاث ثوان"),
            ("3003min", "ثلاثة آلاف وثلاث دقائق"),
            ("101s", "مائة وواحدة ثانية"),
            ("102min", "مائة واثنتان دقيقة"),
            ("1001s", "ألف وواحدة ثانية"),
            ("101000min", "مائة وواحد ألف دقيقة"),
        ] {
            let reading = normalize_for_speech(source, &options).unwrap();
            assert!(reading.fully_supported, "{style:?} {source}: {reading:?}");
            assert_eq!(reading.text, gold, "{style:?} {source}: {reading:?}");
        }
    }
}

#[test]
fn full_currency_identity_is_not_bypassed_by_native_written_subunits() {
    let mut options = options(SpeechLocale::Zh);
    options.reading_style = ReadingStyle::Natural;
    options.interpret_as = textintel::speech::SpeechInterpretation::Currency;
    for (source, fraction) in [("5毛", "5"), ("5角", "5"), ("5分", "05")] {
        let parsed = parse_for_speech(source, &options).unwrap();
        options.currency_naming = CurrencyNaming::Full;
        assert_eq!(parse_for_speech(source, &options).unwrap(), parsed);
        assert!(matches!(
            &parsed.spans[0].value,
            SpeechValue::Currency { code, value }
                if code == "CNY" && value.integer == "0" && value.fraction.as_deref() == Some(fraction)
        ));
        let full = normalize_for_speech(source, &options).unwrap();
        assert!(full.fully_supported, "{source}: {full:?}");
        assert!(full.text.contains("人民币"), "{source}: {full:?}");
        assert_eq!(full.readings[0].source, source);
        options.currency_naming = CurrencyNaming::Style;
    }
}

#[test]
fn scaled_quantities_use_exact_decimal_shifts_instead_of_float() {
    for (locale, source, integer) in [
        (SpeechLocale::En, "100 million kg", "100000000"),
        (SpeechLocale::En, "USD 2.5 million", "2500000"),
        (SpeechLocale::Ko, "₩30억", "3000000000"),
    ] {
        let parsed = parse_for_speech(source, &options(locale)).unwrap();
        assert_eq!(parsed.spans.len(), 1, "{source}: {parsed:?}");
        let value = match &parsed.spans[0].value {
            SpeechValue::Currency { value, .. } | SpeechValue::Measure { value, .. } => value,
            other => panic!("{source}: {other:?}"),
        };
        assert_eq!(value.integer, integer);
        assert_eq!(value.fraction, None);
    }
}

#[test]
fn measure_hints_preserve_scaled_values_instead_of_consuming_literal_labels() {
    for (locale, source, integer) in [
        (SpeechLocale::En, "100 million kg", "100000000"),
        (SpeechLocale::Ru, "3.2 млн л", "3200000"),
    ] {
        let mut options = options(locale);
        options.interpret_as = textintel::speech::SpeechInterpretation::Measure;
        options.reading_style = ReadingStyle::Natural;
        let parsed = parse_for_speech(source, &options).unwrap();
        assert_eq!(parsed.spans.len(), 1, "{source}: {parsed:?}");
        let SpeechValue::Measure { value, .. } = &parsed.spans[0].value else {
            panic!("{source}: {parsed:?}")
        };
        assert_eq!(value.integer, integer);
        assert_eq!(value.fraction, None);
        assert!(
            normalize_for_speech(source, &options)
                .unwrap()
                .fully_supported
        );
    }
}

#[test]
fn native_units_embedded_in_cjk_prose_preserve_all_surrounding_text() {
    let mut options = options(SpeechLocale::Zh);
    options.number_notation = NumberNotation::Orthographic;
    let source = "测量机显示重量是25kg，温度是38°C。";
    let normalized = normalize_for_speech(source, &options).unwrap();
    assert!(normalized.fully_supported, "{normalized:?}");
    assert!(normalized.text.starts_with("测量机显示重量是二十五"));
    assert!(normalized.text.ends_with("。"));
    assert!(!normalized.text.contains("25"));
    assert!(!normalized.text.contains("38"));
}

#[test]
fn malformed_amounts_identifiers_and_unknown_units_are_not_partially_recognized() {
    for source in ["v12kg", "12kgThing", "https://example.org/12kg", "AB123kg"] {
        let parsed = parse_for_speech(source, &options(SpeechLocale::En)).unwrap();
        assert!(
            !parsed
                .spans
                .iter()
                .any(|s| matches!(s.value, SpeechValue::Measure { .. })),
            "{source}: {parsed:?}"
        );
    }
}

#[test]
fn labelled_numbers_require_a_measure_hint_and_keep_label_order_explicit() {
    use textintel::speech::SpeechInterpretation;
    for (locale, source, expected, number_first) in [
        (SpeechLocale::Es, "a-5", "a cinco", false),
        (SpeechLocale::Es, "1.2-a", "uno coma dos a", true),
        (SpeechLocale::En, "volume-7", "volume seven", false),
    ] {
        let mut options = options(locale);
        let normal = parse_for_speech(source, &options).unwrap();
        assert!(
            !normal
                .spans
                .iter()
                .any(|s| matches!(s.value, SpeechValue::LabelledNumber { .. }))
        );
        options.interpret_as = SpeechInterpretation::Measure;
        options.reading_style = ReadingStyle::Natural;
        let parsed = parse_for_speech(source, &options).unwrap();
        assert_eq!(parsed.spans.len(), 1, "{source}: {parsed:?}");
        assert!(
            matches!(parsed.spans[0].value,SpeechValue::LabelledNumber{number_first:first,..} if first==number_first),
            "{parsed:?}"
        );
        let normalized = normalize_for_speech(source, &options).unwrap();
        assert_eq!(normalized.text, expected);
        assert!(normalized.fully_supported);
    }
}

#[test]
fn written_money_scales_keep_decimal_precision_and_named_large_scales() {
    for (locale, source, currency, expected) in [
        (
            SpeechLocale::En,
            "$0.2 million",
            "USD",
            "zero point two million dollars",
        ),
        (SpeechLocale::En, "¥30B", "JPY", "thirty billion yen"),
        (
            SpeechLocale::Es,
            "2.3 billones de euros",
            "EUR",
            "dos coma tres billones de euros",
        ),
        (SpeechLocale::Ko, "₩30조", "KRW", "삼십조원"),
    ] {
        let mut options = options(locale);
        options.default_currency = Some(currency.into());
        options.reading_style = ReadingStyle::Natural;
        let result = normalize_for_speech(source, &options).unwrap();
        assert!(result.fully_supported, "{source}: {result:?}");
        assert_eq!(result.text, expected, "{source}");
    }
}

#[test]
fn symbol_policies_resolve_each_currency_without_cross_symbol_fallback() {
    let mut options = options(SpeechLocale::En);
    options.reading_style = ReadingStyle::Natural;
    options.default_currency = Some("USD".into());
    options.currency_symbols.insert("$".into(), "CAD".into());
    let unresolved = normalize_for_speech("$2; ¥3", &options).unwrap();
    assert!(!unresolved.fully_supported);
    assert!(unresolved.text.contains("Canadian dollars"));
    assert!(unresolved.text.ends_with("¥3"));
    options.currency_symbols.insert("¥".into(), "JPY".into());
    let resolved = normalize_for_speech("$2; ¥3", &options).unwrap();
    assert!(resolved.fully_supported, "{resolved:?}");
    assert!(resolved.text.ends_with("three yen"));
}

#[test]
fn natural_currency_minor_units_preserve_values_and_agreement() {
    for (locale, source, expected) in [
        (
            SpeechLocale::Ru,
            "6 руб. 35 коп.",
            "шесть рублей тридцать пять копеек",
        ),
        (
            SpeechLocale::Ru,
            "1 руб. 21 коп.",
            "один рубль двадцать одна копейка",
        ),
        (SpeechLocale::Ar, "US$7.05", "سبعة دولارات وخمسة سنتات"),
        (SpeechLocale::Ar, "US$7.50", "سبعة دولارات وخمسون سنتا"),
        (SpeechLocale::De, "EUR0.01", "ein cent"),
        (SpeechLocale::Es, "USD1.01", "un dólar un centavo"),
        (SpeechLocale::Pt, "R$12.05", "doze reais e cinco centavos"),
    ] {
        let mut options = options(locale);
        options.reading_style = ReadingStyle::Natural;
        let result = normalize_for_speech(source, &options).unwrap();
        assert_eq!(result.text, expected, "{source}: {result:?}");
        assert!(result.fully_supported, "{source}");
    }
}

#[test]
fn arabic_currency_counts_use_standalone_agreement_instead_of_invariant_display_names() {
    for (source, expected) in [
        ("USD1", "دولار واحد"),
        ("USD2", "دولاران"),
        ("USD3", "ثلاثة دولارات"),
        ("USD11", "أحد عشر دولارا"),
    ] {
        let result = normalize_for_speech(source, &options(SpeechLocale::Ar)).unwrap();
        assert!(result.fully_supported, "{source}: {result:?}");
        assert_eq!(result.text, expected);
    }
}

#[test]
fn measured_mixed_fractions_keep_exact_components_and_noun_gender() {
    use textintel::speech::SpeechInterpretation;
    for (source, expected) in [
        ("3 3/5 m", "tres y tres quintos de metro"),
        ("1 1/2 mph", "una milla y media por hora"),
    ] {
        let mut options = options(SpeechLocale::Es);
        options.reading_style = ReadingStyle::Natural;
        options.interpret_as = SpeechInterpretation::Measure;
        let parsed = parse_for_speech(source, &options).unwrap();
        assert_eq!(parsed.spans.len(), 1, "{parsed:?}");
        assert!(matches!(
            parsed.spans[0].value,
            SpeechValue::MeasuredExpression { .. }
        ));
        let result = normalize_for_speech(source, &options).unwrap();
        assert!(result.fully_supported, "{result:?}");
        assert_eq!(result.text, expected);
    }
}

#[test]
fn spanish_fraction_nouns_and_adjectival_halves_have_distinct_agreement() {
    // FundéuRAE attests una hora y media; UDEP distinguishes adjectival
    // medio/media agreement, and ULPGC IATEXT's mathematical fraction nouns
    // are masculine. These are independent examples, not reference rewrites.
    // https://www.fundeu.es/consulta/el-punto-o-la-coma-en-la-escritura-de-las-horas-432/
    // https://www.udep.edu.pe/castellanoactual/media-loca-o-medio-loca/
    // https://tulengua.iatext.ulpgc.es/es/convertir-numeros-a-texto-letras-fraccionarios.html
    let mut options = options(SpeechLocale::Es);
    options.reading_style = ReadingStyle::Natural;
    options.interpret_as = textintel::speech::SpeechInterpretation::Measure;
    for (source, expected) in [
        ("1/2h", "media hora"),
        ("-1/2h", "menos media hora"),
        ("1/3h", "un tercio de hora"),
        ("-1/3h", "menos un tercio de hora"),
        ("3/2h", "tres medios de hora"),
        ("-3/2h", "menos tres medios de hora"),
        ("3/2kg", "tres medios de kilogramo"),
        ("1 1/2h", "una hora y media"),
        ("3 1/2h", "tres horas y media"),
        ("-3 1/2h", "menos tres horas y media"),
        ("0 1/2h", "media hora"),
        ("-0 1/2h", "menos media hora"),
        ("1 1/2kg", "un kilogramo y medio"),
        ("4 1/2kg", "cuatro kilogramos y medio"),
        ("-4 1/2kg", "menos cuatro kilogramos y medio"),
        ("2 1/2 km/h", "dos kilómetros y medio por hora"),
        ("-2 1/2 mph", "menos dos millas y media por hora"),
        ("3 3/5kg", "tres y tres quintos de kilogramo"),
        ("-3 3/5kg", "dividir menos dieciocho kilogramos entre cinco"),
    ] {
        let result = normalize_for_speech(source, &options).unwrap();
        assert_eq!(result.text, expected, "{source}: {result:?}");
        assert!(result.fully_supported, "{source}: {result:?}");
    }
}

#[test]
fn explicit_quantities_handle_integer_padding_and_zero_minor_precision() {
    for (locale, source, expected) in [
        (SpeechLocale::En, "USD2.00", "two dollars"),
        (SpeechLocale::De, "EUR2.000", "zwei euro"),
        (SpeechLocale::Ar, "09%", "تسعة في المائة"),
    ] {
        let mut options = options(locale);
        options.reading_style = ReadingStyle::Natural;
        let result = normalize_for_speech(source, &options).unwrap();
        assert!(result.fully_supported, "{result:?}");
        assert_eq!(result.text, expected);
    }
}

#[test]
fn native_written_unit_names_reuse_the_same_dictionary_as_rendering() {
    for (locale, source, key) in [
        (SpeechLocale::Hi, "१२ किलोग्राम", "mass-kilogram"),
        (SpeechLocale::Bn, "১২ কিলোগ্রাম", "mass-kilogram"),
        (SpeechLocale::Ru, "12 килограммов", "mass-kilogram"),
        (SpeechLocale::Ko, "12 킬로그램", "mass-kilogram"),
        (SpeechLocale::Fr, "12 kilogrammes", "mass-kilogram"),
        (SpeechLocale::Es, "12 kilogramos", "mass-kilogram"),
    ] {
        let parsed = parse_for_speech(source, &options(locale)).unwrap();
        assert_eq!(parsed.spans.len(), 1, "{source}: {parsed:?}");
        assert!(
            matches!(&parsed.spans[0].value,SpeechValue::Measure{unit,..} if unit==key),
            "{source}: {parsed:?}"
        );
    }
}

#[test]
fn si_prefix_and_bit_byte_case_are_semantic_not_cosmetic() {
    for (source, expected) in [
        ("1MW", "power-megawatt"),
        ("1MHz", "frequency-megahertz"),
        ("1MB", "digital-megabyte"),
        ("1Mb", "digital-megabit"),
        ("1mL", "volume-milliliter"),
        ("1ml", "volume-milliliter"),
        ("1mg", "mass-milligram"),
        ("1Mg", "mass-tonne"),
        ("1Mbps", "digital-megabit/per:duration-second"),
        ("1MBps", "digital-megabyte/per:duration-second"),
        ("1KG", "mass-kilogram"),
    ] {
        let parsed = parse_for_speech(source, &options(SpeechLocale::En)).unwrap();
        assert_eq!(parsed.spans.len(), 1, "{source}: {parsed:?}");
        assert!(
            matches!(&parsed.spans[0].value,SpeechValue::Measure{unit,..}if unit==expected),
            "{source}: {parsed:?}"
        );
    }
    for source in [
        "1mW", "1mHz", "1mb", "1ML", "1mG", "1MM", "1M/s", "1KM/H", "1M²",
    ] {
        let parsed = parse_for_speech(source, &options(SpeechLocale::En)).unwrap();
        assert!(
            !parsed
                .spans
                .iter()
                .any(|s| matches!(s.value, SpeechValue::Measure { .. })),
            "ambiguous/unsupportedcasewasreinterpreted: {source}: {parsed:?}"
        );
    }
}

#[test]
fn canonical_equivalent_unit_nouns_keep_original_utf8_ranges() {
    for source in ["12 mètres", "12 me\u{0300}tres"] {
        let parsed = parse_for_speech(source, &options(SpeechLocale::Fr)).unwrap();
        assert_eq!(parsed.spans.len(), 1, "{source}: {parsed:?}");
        assert!(
            matches!(&parsed.spans[0].value,SpeechValue::Measure{unit,..}if unit=="length-meter")
        );
        assert_eq!(parsed.spans[0].source, source);
        assert_eq!(parsed.spans[0].end, source.len());
    }
}

#[test]
fn natural_spoken_units_distinguish_word_spacing_from_digit_format_patterns() {
    for (locale, source, expected) in [
        (SpeechLocale::Ja, "73kg", "七十三キロ"),
        (SpeechLocale::Ja, "17g", "十七グラム"),
        (SpeechLocale::Ko, "73kg", "칠십삼 킬로그램"),
    ] {
        let mut options = options(locale);
        options.reading_style = ReadingStyle::Natural;
        options.number_notation = NumberNotation::Orthographic;
        let result = normalize_for_speech(source, &options).unwrap();
        assert!(result.fully_supported, "{result:?}");
        assert_eq!(result.text, expected);
    }
}

#[test]
fn fractional_feminine_quantities_reuse_shared_exact_numeric_agreement() {
    let mut options = options(SpeechLocale::Es);
    options.reading_style = ReadingStyle::Natural;
    let result = normalize_for_speech("1.01 mph", &options).unwrap();
    assert!(result.fully_supported, "{result:?}");
    assert_eq!(result.text, "una coma cero una millas por hora");
}

#[test]
fn implicit_one_and_dual_unit_patterns_never_drop_a_negative_sign() {
    let mut options = options(SpeechLocale::Ar);
    options.reading_style = ReadingStyle::Natural;
    for source in ["-1 m", "-2 m"] {
        let result = normalize_for_speech(source, &options).unwrap();
        assert!(result.fully_supported, "{result:?}");
        assert!(result.text.starts_with("سالب "), "{source}: {result:?}");
    }
}

#[test]
fn custom_unit_aliases_require_explicit_caller_policy_and_work_in_both_positions() {
    use textintel::speech::SpeechInterpretation;
    let mut options = options(SpeechLocale::Ar);
    let default = parse_for_speech("f9", &options).unwrap();
    assert!(matches!(
        default.spans[0].value,
        SpeechValue::Identifier { .. }
    ));
    options.interpret_as = SpeechInterpretation::Measure;
    options
        .unit_aliases
        .insert("f".into(), "temperature-fahrenheit".into());
    for source in ["f9", "9f"] {
        let parsed = parse_for_speech(source, &options).unwrap();
        assert_eq!(parsed.spans.len(), 1, "{source}: {parsed:?}");
        assert!(
            matches!(&parsed.spans[0].value,SpeechValue::Measure{unit,value}if unit=="temperature-fahrenheit"&&value.integer=="9")
        );
        assert!(
            normalize_for_speech(source, &options)
                .unwrap()
                .fully_supported
        );
    }
    let unchanged = parse_for_speech("F9", &options).unwrap();
    assert!(matches!(
        unchanged.spans[0].value,
        SpeechValue::Identifier { .. }
    ));
}

#[test]
fn explicit_chinese_currency_subunits_shift_the_exact_value_and_preserve_the_named_unit() {
    use textintel::speech::SpeechInterpretation;
    let mut options = options(SpeechLocale::Zh);
    options.reading_style = ReadingStyle::Natural;
    options.number_notation = NumberNotation::Orthographic;
    let auto = parse_for_speech("5毛", &options).unwrap();
    assert!(
        !auto
            .spans
            .iter()
            .any(|s| matches!(s.value, SpeechValue::Currency { .. }))
    );
    options.interpret_as = SpeechInterpretation::Currency;
    for (source, integer, fraction, expected) in [
        ("5毛", "0", "5", "五毛"),
        ("5角", "0", "5", "五角"),
        ("5分", "0", "05", "五分"),
        ("23毛", "2", "3", "二十三毛"),
        ("-5毛", "0", "5", "负五毛"),
    ] {
        let parsed = parse_for_speech(source, &options).unwrap();
        assert_eq!(parsed.spans.len(), 1, "{source}: {parsed:?}");
        let SpeechValue::Currency { code, value } = &parsed.spans[0].value else {
            panic!("{parsed:?}")
        };
        assert_eq!(code, "CNY");
        assert_eq!(value.integer, integer);
        assert_eq!(value.fraction.as_deref(), Some(fraction));
        assert_eq!(value.negative, source.starts_with('-'));
        let spoken = normalize_for_speech(source, &options).unwrap();
        assert!(spoken.fully_supported, "{spoken:?}");
        assert_eq!(spoken.text, expected);
    }
}

#[test]
fn arabic_spoken_degree_counts_inflect_the_unit_and_preserve_number_gender() {
    for (source, expected) in [
        ("1°C", "درجة مئوية واحدة"),
        ("2°C", "درجتان مئويتان"),
        ("3°C", "ثلاث درجات مئوية"),
        ("9°F", "تسع درجات فهرنهايت"),
    ] {
        let mut options = options(SpeechLocale::Ar);
        options.reading_style = ReadingStyle::Natural;
        let result = normalize_for_speech(source, &options).unwrap();
        assert!(result.fully_supported, "{result:?}");
        assert_eq!(result.text, expected);
    }
}

#[test]
fn english_fraction_units_agree_and_keep_division_outside_the_unit_operand() {
    let mut opts = options(SpeechLocale::En);
    opts.reading_style = ReadingStyle::Natural;
    opts.interpret_as = textintel::speech::SpeechInterpretation::Measure;
    for (source, gold) in [
        ("1/2kg", "one half kilogram"),
        ("3/5kg", "three fifths of a kilogram"),
        ("1/3h", "one third of an hour"),
        ("1 1/2kg", "one and one half kilograms"),
        ("-1 1/2kg", "minus one and one half kilograms"),
        ("0 1/2kg", "one half kilogram"),
        ("1/2.5kg", "one kilogram divided by two point five"),
        ("1.5/2kg", "one point five kilograms divided by two"),
    ] {
        let result = normalize_for_speech(source, &opts).unwrap();
        assert!(result.fully_supported, "{source}: {result:?}");
        assert_eq!(result.text, gold, "{source}");
    }
}

#[test]
fn parsed_english_measured_fractions_keep_their_value_when_reading_style_changes() {
    use textintel::speech::{SpeechInterpretation, verbalize_document_with_options};
    let mut options = options(SpeechLocale::En);
    options.reading_style = ReadingStyle::Natural;
    options.interpret_as = SpeechInterpretation::Measure;
    for (source, natural, precise) in [
        ("1/2kg", "one half kilogram", "one kilogram divided by two"),
        (
            "3/2kg",
            "three halves of a kilogram",
            "three kilograms divided by two",
        ),
        (
            "-1/3kg",
            "minus one third of a kilogram",
            "minus one kilogram divided by three",
        ),
        (
            "3 1/2kg",
            "three and one half kilograms",
            "seven kilograms divided by two",
        ),
        (
            "-3 1/2kg",
            "minus three and one half kilograms",
            "minus seven kilograms divided by two",
        ),
        (
            "1/2.5kg",
            "one kilogram divided by two point five",
            "one kilogram divided by two point five",
        ),
    ] {
        let parsed = parse_for_speech(source, &options).unwrap();
        assert_eq!(parsed.spans.len(), 1, "{source}: {parsed:?}");
        assert!(matches!(
            parsed.spans[0].value,
            SpeechValue::MeasuredExpression { .. }
        ));
        let original = parsed.clone();
        let natural_result = verbalize_document_with_options(&parsed, &options).unwrap();
        assert!(
            natural_result.fully_supported,
            "{source}: {natural_result:?}"
        );
        assert_eq!(natural_result.text, natural, "{source}");
        let mut precise_options = options.clone();
        precise_options.reading_style = ReadingStyle::Precise;
        let precise_result = verbalize_document_with_options(&parsed, &precise_options).unwrap();
        assert!(
            precise_result.fully_supported,
            "{source}: {precise_result:?}"
        );
        assert_eq!(precise_result.text, precise, "{source}");
        assert_eq!(
            parsed, original,
            "rendering must not mutate the typed value"
        );
        for result in [natural_result, precise_result] {
            assert_eq!(result.readings[0].source, source);
            assert_eq!(result.readings[0].start, 0);
            assert_eq!(result.readings[0].end, source.len());
        }
    }
}

#[test]
fn measured_expression_boundaries_use_native_and_caller_unit_dictionaries() {
    let mut opts = options(SpeechLocale::En);
    opts.reading_style = ReadingStyle::Natural;
    opts.interpret_as = textintel::speech::SpeechInterpretation::Measure;
    opts.unit_aliases
        .insert("widgets".into(), "mass-kilogram".into());
    let result = normalize_for_speech("3/5widgets", &opts).unwrap();
    assert_eq!(result.text, "three fifths of a kilogram");
    assert!(result.fully_supported);
    let bad = parse_for_speech("3/5Widgets", &opts).unwrap();
    assert!(
        !bad.spans
            .iter()
            .any(|s| matches!(s.value, SpeechValue::MeasuredExpression { .. }))
    );
    let mut fr = options(SpeechLocale::Fr);
    fr.reading_style = ReadingStyle::Natural;
    fr.interpret_as = textintel::speech::SpeechInterpretation::Measure;
    let result = normalize_for_speech("1/3kilogrammes", &fr).unwrap();
    assert_eq!(result.text, "diviser un kilogramme par trois");
    assert!(result.fully_supported);
}

#[test]
fn measured_mixed_number_meaning_is_retained_across_twenty_languages() {
    // These expectations spell the same exact rational quantity. Outside the
    // established en/es idioms, 1 1/2 m is rendered as 3 meters divided by 2;
    // the physical unit belongs to the numerator, never to the divisor.
    let golds = [
        (SpeechLocale::En, "1 1/2m", "one and one half meters"),
        (SpeechLocale::Es, "3 3/5m", "tres y tres quintos de metro"),
        (SpeechLocale::Zh, "1 1/2m", "三米除以二"),
        (SpeechLocale::Hi, "1 1/2m", "तीन मीटर बटा दो"),
        (SpeechLocale::Ar, "1 1/2m", "ثلاثة أمتار قسمة اثنان"),
        (SpeechLocale::Fr, "1 1/2m", "diviser trois mètres par deux"),
        (SpeechLocale::Pt, "1 1/2m", "dividir três metros por dois"),
        (SpeechLocale::Ru, "1 1/2m", "три метра разделить на два"),
        (SpeechLocale::Bn, "1 1/2m", "তিন মিটার ভাগ দুই"),
        (SpeechLocale::Ur, "1 1/2m", "تین میٹر تقسیم بر دو"),
        (SpeechLocale::Id, "1 1/2m", "tiga meter dibagi dua"),
        (SpeechLocale::De, "1 1/2m", "drei Meter geteilt durch zwei"),
        (SpeechLocale::Ja, "1 1/2m", "三メートル割る二"),
        (SpeechLocale::Pa, "1 1/2m", "ਤਿੰਨ ਮੀਟਰ ਬਟਾ ਦੋ"),
        (SpeechLocale::Mr, "1 1/2m", "तीन मीटर भागिले दोन"),
        (SpeechLocale::Te, "1 1/2m", "మూడు మీటర్లు భాగహారం రెండు"),
        (SpeechLocale::Tr, "1 1/2m", "üç metre bölü iki"),
        (SpeechLocale::Ta, "1 1/2m", "மூன்று மீட்டர்கள் வகுத்தல் இரண்டு"),
        (SpeechLocale::Vi, "1 1/2m", "ba mét chia hai"),
        (SpeechLocale::Ko, "1 1/2m", "삼 미터 나누기 이"),
    ];
    assert_eq!(golds.len(), SpeechLocale::ALL.len());
    for (locale, source, gold) in golds {
        let mut opts = options(locale);
        opts.reading_style = ReadingStyle::Natural;
        opts.number_notation = NumberNotation::Orthographic;
        opts.interpret_as = textintel::speech::SpeechInterpretation::Measure;
        let result = normalize_for_speech(source, &opts).unwrap();
        assert!(result.fully_supported, "{locale:?}: {result:?}");
        assert_eq!(result.text, gold, "{locale:?}");
    }
}

#[test]
fn scientific_measurements_attach_the_unit_to_the_mantissa_not_the_exponent() {
    for (locale, source, gold) in [
        (
            SpeechLocale::En,
            "2e3kg",
            "two kilograms times ten to the power of three",
        ),
        (
            SpeechLocale::Fr,
            "2e3kg",
            "deux kilogrammes fois dix puissance trois",
        ),
        (SpeechLocale::Zh, "2e3kg", "二千克乘以十的三次方"),
        (SpeechLocale::Ja, "2e3kg", "二キロ掛ける十の三乗"),
    ] {
        let mut opts = options(locale);
        opts.reading_style = ReadingStyle::Natural;
        opts.number_notation = NumberNotation::Orthographic;
        opts.interpret_as = textintel::speech::SpeechInterpretation::Measure;
        let result = normalize_for_speech(source, &opts).unwrap();
        assert!(result.fully_supported, "{result:?}");
        assert_eq!(result.text, gold);
    }
}

#[test]
fn explicit_literal_count_labels_are_not_guessed_as_physical_units_or_inflected() {
    use textintel::speech::SpeechInterpretation;
    for (locale, source, expected, label) in [
        (SpeechLocale::Ru, "9мешок", "девять мешок", "мешок"),
        (
            SpeechLocale::En,
            "23widgets",
            "twenty three widgets",
            "widgets",
        ),
        (SpeechLocale::Es, "7 cajas", "siete cajas", "cajas"),
    ] {
        let automatic = options(locale);
        let parsed = parse_for_speech(source, &automatic).unwrap();
        assert!(
            !parsed
                .spans
                .iter()
                .any(|s| matches!(s.value, SpeechValue::LabelledNumber { .. }))
        );
        let mut explicit = automatic;
        explicit.interpret_as = SpeechInterpretation::Measure;
        explicit.reading_style = ReadingStyle::Natural;
        let parsed = parse_for_speech(source, &explicit).unwrap();
        assert_eq!(parsed.spans.len(), 1, "{source}: {parsed:?}");
        assert!(
            matches!(&parsed.spans[0].value,SpeechValue::LabelledNumber{label:actual,..} if actual==label)
        );
        let result = normalize_for_speech(source, &explicit).unwrap();
        assert_eq!(result.text, expected);
        assert!(result.fully_supported);
    }
    let mut explicit = options(SpeechLocale::En);
    explicit.interpret_as = SpeechInterpretation::Measure;
    for source in ["12mW", "12mHz", "12ML"] {
        let parsed = parse_for_speech(source, &explicit).unwrap();
        assert!(
            !parsed
                .spans
                .iter()
                .any(|s| matches!(s.value, SpeechValue::LabelledNumber { .. })),
            "{source}: {parsed:?}"
        );
    }
}
