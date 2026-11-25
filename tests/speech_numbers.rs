use textintel::speech::numbers::{cardinal_max, verbalize_number, verbalize_number_feminine};
use textintel::speech::{NumberReading, NumberValue, SpeechLocale};

fn number(integer: &str) -> NumberValue {
    NumberValue {
        negative: false,
        integer: integer.into(),
        fraction: None,
    }
}
fn read(integer: &str, locale: SpeechLocale) -> String {
    verbalize_number(&number(integer), locale, NumberReading::Cardinal).unwrap()
}

#[test]
fn independent_forty_two_gold_examples_cover_all_twenty_locales() {
    use SpeechLocale::*;
    let cases = [
        (En, "forty-two"),
        (Es, "cuarenta y dos"),
        (Zh, "四十二"),
        (Hi, "बयालीस"),
        (Ar, "اثنان وأربعون"),
        (Fr, "quarante-deux"),
        (Pt, "quarenta e dois"),
        (Ru, "сорок два"),
        (Bn, "বিয়াল্লিশ"),
        (Ur, "بیالیس"),
        (Id, "empat puluh dua"),
        (De, "zweiundvierzig"),
        (Ja, "よんじゅうに"),
        (Pa, "ਬਿਆਲੀ"),
        (Mr, "बेचाळीस"),
        (Te, "నలభై రెండు"),
        (Tr, "kırk iki"),
        (Ta, "நாற்பத்து இரண்டு"),
        (Vi, "bốn mươi hai"),
        (Ko, "사십이"),
    ];
    assert_eq!(cases.len(), SpeechLocale::ALL.len());
    for (locale, gold) in cases {
        assert_eq!(read("42", locale), gold, "{}", locale.code());
    }
}

#[test]
fn native_number_systems_and_contextual_compounds_have_explicit_gold() {
    use SpeechLocale::*;
    for (locale, integer, gold) in [
        (En, "1001", "one thousand one"),
        (En, "1000000000", "one billion"),
        (Es, "101", "ciento uno"),
        (Es, "101001", "ciento un mil uno"),
        (Es, "21000001", "veintiún millones uno"),
        (Es, "1000000000", "mil millones"),
        (Fr, "200", "deux cents"),
        (Fr, "201", "deux cent un"),
        (Fr, "80000", "quatre-vingt mille"),
        (Pt, "100", "cem"),
        (Pt, "101", "cento e um"),
        (Pt, "1023", "mil e vinte e três"),
        (Pt, "1123", "mil cento e vinte e três"),
        (De, "101", "einhunderteins"),
        (De, "2026", "zweitausendsechsundzwanzig"),
        (Ru, "21000", "двадцать одна тысяча"),
        (Ru, "22000", "двадцать две тысячи"),
        (Ru, "12000", "двенадцать тысяч"),
        (Hi, "100000", "एक लाख"),
        (Hi, "10000000", "एक करोड़"),
        (Bn, "2026", "দুই হাজার ছাব্বিশ"),
        (Ur, "2026", "دو ہزار چھبیس"),
        (Pa, "2026", "ਦੋ ਹਜ਼ਾਰ ਛੱਬੀ"),
        (Mr, "101", "एकशे एक"),
        (Mr, "2026", "दोन हजार सव्वीस"),
        (Te, "101", "నూట ఒకటి"),
        (Te, "2026", "రెండు వేల ఇరవై ఆరు"),
        (Ta, "101", "நூற்று ஒன்று"),
        (Ta, "2026", "இரண்டாயிரத்து இருபத்து ஆறு"),
        (Ar, "2000", "ألفان"),
        (Ar, "2026", "ألفان وستة وعشرون"),
        (Id, "1001", "seribu satu"),
        (Tr, "1001", "bin bir"),
        (Zh, "101", "一百零一"),
        (Zh, "1010", "一千零一十"),
        (Zh, "100001", "十万零一"),
        (Ja, "300", "さんびゃく"),
        (Ja, "600", "ろっぴゃく"),
        (Ja, "8000", "はっせん"),
        (Ja, "10000", "いちまん"),
        (Ko, "101", "백일"),
        (Ko, "10000", "만"),
        (Ko, "100000001", "일억 일"),
        (Ko, "120001", "십이만 일"),
        (Vi, "101", "một trăm lẻ một"),
        (Vi, "1000001", "một triệu lẻ một"),
        (Vi, "2000021", "hai triệu hai mươi mốt"),
        (Vi, "121", "một trăm hai mươi mốt"),
        (Vi, "2026", "hai nghìn không trăm hai mươi sáu"),
    ] {
        assert_eq!(read(integer, locale), gold, "{} {integer}", locale.code());
    }
}

#[test]
fn decimal_and_identifier_reading_preserve_sign_and_every_zero() {
    let value = NumberValue {
        negative: true,
        integer: "12".into(),
        fraction: Some("0050".into()),
    };
    assert_eq!(
        verbalize_number(&value, SpeechLocale::En, NumberReading::Cardinal).unwrap(),
        "minus twelve point zero zero five zero"
    );
    assert_eq!(
        verbalize_number(&value, SpeechLocale::Es, NumberReading::Cardinal).unwrap(),
        "menos doce coma cero cero cinco cero"
    );
    let identifier = number("0012");
    assert!(verbalize_number(&identifier, SpeechLocale::En, NumberReading::Cardinal).is_none());
    assert_eq!(
        verbalize_number(&identifier, SpeechLocale::En, NumberReading::Digits).unwrap(),
        "zero zero one two"
    );
    for locale in SpeechLocale::ALL {
        let spoken = verbalize_number(&identifier, locale, NumberReading::Digits).unwrap();
        assert_eq!(spoken.split_whitespace().count(), 4, "{}", locale.code());
    }
}

#[test]
fn invalid_components_and_excessive_magnitudes_fail_without_guessing() {
    for integer in ["", "-1", "1,000", "1.0", "NaN", "inf", "１２", "1e3"] {
        assert!(
            verbalize_number(&number(integer), SpeechLocale::En, NumberReading::Cardinal).is_none(),
            "{integer}"
        );
    }
    for fraction in ["", "0.1", "-1", "1e2"] {
        let value = NumberValue {
            negative: false,
            integer: "1".into(),
            fraction: Some(fraction.into()),
        };
        assert!(verbalize_number(&value, SpeechLocale::En, NumberReading::Cardinal).is_none());
    }
    assert!(
        verbalize_number(
            &number(&"1".repeat(129)),
            SpeechLocale::En,
            NumberReading::Digits
        )
        .is_none()
    );
    for locale in SpeechLocale::ALL {
        assert!(verbalize_number(&number("9999"), locale, NumberReading::Cardinal).is_some());
        assert!(
            verbalize_number(
                &number(&(cardinal_max(locale) + 1).to_string()),
                locale,
                NumberReading::Cardinal
            )
            .is_none(),
            "{}",
            locale.code()
        );
    }
}

#[test]
fn feminine_agreement_is_explicit_and_bounded() {
    for (locale, n, gold) in [
        (SpeechLocale::Es, "221", "doscientas veintiuna"),
        (SpeechLocale::Pt, "222", "duzentas e vinte e duas"),
        (SpeechLocale::Ru, "22", "двадцать две"),
        (SpeechLocale::Ar, "1", "واحدة"),
    ] {
        assert_eq!(
            verbalize_number_feminine(&number(n), locale, NumberReading::Cardinal).as_deref(),
            Some(gold)
        );
    }
    assert_eq!(
        verbalize_number_feminine(&number("1000"), SpeechLocale::Es, NumberReading::Cardinal)
            .as_deref(),
        Some("mil")
    );
}

#[test]
fn arabic_feminine_counts_reuse_scale_data_with_explicit_nominative_context() {
    // Independent number-agreement rules and the two different contexts of8:
    // https://uomus.edu.iq/img/lectures21/MUCLecture_2024_51215125.pdf
    for (integer, gold) in [
        ("0", "صفر"),
        ("2", "اثنتان"),
        ("3", "ثلاث"),
        ("4", "أربع"),
        ("5", "خمس"),
        ("6", "ست"),
        ("7", "سبع"),
        ("8", "ثماني"),
        ("9", "تسع"),
        ("10", "عشر"),
        ("11", "إحدى عشرة"),
        ("12", "اثنتا عشرة"),
        ("13", "ثلاث عشرة"),
        ("14", "أربع عشرة"),
        ("15", "خمس عشرة"),
        ("16", "ست عشرة"),
        ("17", "سبع عشرة"),
        ("18", "ثماني عشرة"),
        ("19", "تسع عشرة"),
        ("20", "عشرون"),
        ("21", "إحدى وعشرون"),
        ("22", "اثنتان وعشرون"),
        ("23", "ثلاث وعشرون"),
        ("28", "ثمان وعشرون"),
        ("100", "مائة"),
        ("103", "مائة وثلاث"),
        ("108", "مائة وثماني"),
        ("118", "مائة وثماني عشرة"),
        ("128", "مائة وثمان وعشرون"),
        ("200", "مائتان"),
        ("1000", "ألف"),
        ("2000", "ألفان"),
        ("2003", "ألفان وثلاث"),
        ("2000003", "مليونان وثلاث"),
        ("1003", "ألف وثلاث"),
        ("3003", "ثلاثة آلاف وثلاث"),
        ("3000013", "ثلاثة ملايين وثلاث عشرة"),
        ("200000", "مائتا ألف"),
        ("200000000", "مائتا مليون"),
        ("3000000022", "ثلاثة مليارات واثنتان وعشرون"),
        ("101", "مائة وواحدة"),
        ("102", "مائة واثنتان"),
        ("1001", "ألف وواحدة"),
        ("1002", "ألف واثنتان"),
        ("101000", "مائة وواحد ألف"),
        ("102000000", "مائة واثنان مليون"),
    ] {
        assert_eq!(
            verbalize_number_feminine(&number(integer), SpeechLocale::Ar, NumberReading::Cardinal)
                .as_deref(),
            Some(gold),
            "{integer}"
        );
    }
    for integer in ["1000000000000", "1x", ""] {
        assert!(
            verbalize_number_feminine(&number(integer), SpeechLocale::Ar, NumberReading::Cardinal)
                .is_none(),
            "{integer}"
        );
    }
    let mut negative = number("21");
    negative.negative = true;
    assert_eq!(
        verbalize_number_feminine(&negative, SpeechLocale::Ar, NumberReading::Cardinal).as_deref(),
        Some("سالب إحدى وعشرون")
    );
    let mut options = textintel::speech::SpeechOptions::new(SpeechLocale::Ar);
    options.reading_style = textintel::speech::ReadingStyle::Natural;
    assert_eq!(
        textintel::speech::numbers::verbalize_with_gender(
            &number("108"),
            &options,
            Some("feminine")
        )
        .as_deref(),
        Some("مئة وثماني")
    );
    let mut decimal = number("2");
    decimal.fraction = Some("03".into());
    assert!(
        textintel::speech::numbers::verbalize_with_gender(&decimal, &options, Some("feminine"))
            .is_none()
    );
    assert_eq!(read("3", SpeechLocale::Ar), "ثلاثة");
    assert_eq!(read("3003", SpeechLocale::Ar), "ثلاثة آلاف وثلاثة");
    assert_eq!(read("200", SpeechLocale::Ar), "مائتان");
    assert_eq!(read("2003", SpeechLocale::Ar), "ألفان وثلاثة");
    assert_eq!(read("2000003", SpeechLocale::Ar), "مليونان وثلاثة");
    assert_eq!(read("200000", SpeechLocale::Ar), "مائتا ألف");
}

#[test]
fn embedded_tables_are_complete_and_have_no_control_formatting() {
    let source = include_str!("../src/speech/data/numbers.json");
    let tables: serde_json::Value = serde_json::from_str(source).unwrap();
    assert_eq!(tables.as_object().unwrap().len(), 20);
    for locale in SpeechLocale::ALL {
        for key in [
            "small",
            "hundreds",
            "hundreds_prefix",
            "hundreds_tens_prefix",
            "thousands",
            "thousands_prefix",
        ] {
            let rows = tables[locale.code()][key].as_array().unwrap();
            assert_eq!(
                rows.len(),
                if key == "small" { 100 } else { 10 },
                "{} {key}",
                locale.code()
            );
            for (i, row) in rows.iter().enumerate() {
                let word = row.as_str().unwrap();
                assert!(i == 0 || !word.is_empty());
                assert!(
                    !word
                        .chars()
                        .any(|c| c.is_ascii_digit() || c.is_control() || c == '\u{00ad}')
                );
            }
        }
    }
}

#[test]
fn explicit_reading_styles_preserve_default_and_offer_natural_variants() {
    use textintel::speech::numbers::{verbalize_number_with_options, verbalize_with_options};
    use textintel::speech::{NumberNotation, ReadingStyle};
    for locale in SpeechLocale::ALL {
        let options = textintel::speech::SpeechOptions::new(locale);
        assert_eq!(
            verbalize_with_options(&number("42"), &options, NumberReading::Cardinal),
            verbalize_number(&number("42"), locale, NumberReading::Cardinal)
        );
        let padded = number("0042");
        assert!(
            verbalize_number_with_options(
                &padded,
                locale,
                NumberReading::Cardinal,
                ReadingStyle::Natural,
                NumberNotation::Phonetic
            )
            .is_none()
        );
        assert!(
            verbalize_number_with_options(
                &padded,
                locale,
                NumberReading::Cardinal,
                ReadingStyle::Digits,
                NumberNotation::Phonetic
            )
            .is_some()
        );
    }
    for (locale, n, expected) in [
        (SpeechLocale::En, "123", "one hundred and twenty three"),
        (SpeechLocale::En, "1001", "one thousand and one"),
        (
            SpeechLocale::En,
            "1123",
            "one thousand one hundred and twenty three",
        ),
        (SpeechLocale::En, "1000001", "one million and one"),
        (SpeechLocale::Fr, "71", "soixante et onze"),
        (SpeechLocale::Ko, "123000012", "일억이천삼백만십이"),
        (SpeechLocale::Ar, "809", "ثمان مئة وتسعة"),
        (SpeechLocale::Ar, "3917", "ثلاثة آلاف وتسع مئة وسبعة عشر"),
    ] {
        assert_eq!(
            verbalize_number_with_options(
                &number(n),
                locale,
                NumberReading::Cardinal,
                ReadingStyle::Natural,
                NumberNotation::Phonetic
            )
            .as_deref(),
            Some(expected),
            "{} {n}",
            locale.code()
        );
    }
}

#[test]
fn japanese_written_forms_are_composed_from_values_not_kana_substitutions() {
    use textintel::speech::numbers::verbalize_number_with_options;
    use textintel::speech::{NumberNotation, ReadingStyle};
    for (n, gold) in [
        ("0", "零"),
        ("1", "一"),
        ("10", "十"),
        ("100", "百"),
        ("1000", "千"),
        ("5000", "五千"),
        ("10000", "一万"),
        ("10000000", "一千万"),
        ("12345000", "千二百三十四万五千"),
        ("100000001", "一億一"),
    ] {
        assert_eq!(
            verbalize_number_with_options(
                &number(n),
                SpeechLocale::Ja,
                NumberReading::Cardinal,
                ReadingStyle::Precise,
                NumberNotation::Orthographic
            )
            .as_deref(),
            Some(gold),
            "{n}"
        );
    }
    let decimal = NumberValue {
        negative: true,
        integer: "12".into(),
        fraction: Some("0050".into()),
    };
    assert_eq!(
        verbalize_number_with_options(
            &decimal,
            SpeechLocale::Ja,
            NumberReading::Cardinal,
            ReadingStyle::Natural,
            NumberNotation::Orthographic
        )
        .as_deref(),
        Some("マイナス十二点零零五零")
    );
    assert_eq!(
        verbalize_number_with_options(
            &number("0012"),
            SpeechLocale::Ja,
            NumberReading::Digits,
            ReadingStyle::Precise,
            NumberNotation::Orthographic
        )
        .as_deref(),
        Some("零零一二")
    );
}

#[test]
fn arabic_hundreds_use_hundred_stems_instead_of_standalone_cardinals() {
    for (n, gold) in [
        ("100", "مائة"),
        ("200", "مائتان"),
        ("300", "ثلاثمائة"),
        ("400", "أربعمائة"),
        ("500", "خمسمائة"),
        ("600", "ستمائة"),
        ("700", "سبعمائة"),
        ("800", "ثمانمائة"),
        ("900", "تسعمائة"),
        ("9837", "تسعة آلاف وثمانمائة وسبعة وثلاثون"),
    ] {
        assert_eq!(read(n, SpeechLocale::Ar), gold, "{n}");
    }
}

#[test]
fn natural_decimal_fractions_retain_denominator_precision_and_agreement() {
    use textintel::speech::numbers::verbalize_number_with_options;
    use textintel::speech::{NumberNotation, ReadingStyle};
    for (locale, integer, fraction, gold) in [
        (SpeechLocale::Ru, "3", "2", "три целых две десятых"),
        (SpeechLocale::Ru, "1", "02", "одна целая две сотых"),
        (SpeechLocale::Ru, "2", "21", "две целых двадцать одна сотая"),
        (SpeechLocale::Ru, "1", "00", "одна целая ноль сотых"),
        (
            SpeechLocale::Ru,
            "11",
            "0001",
            "одиннадцать целых одна десятитысячная",
        ),
        (SpeechLocale::Ar, "6", "88", "ستة وثمانية وثمانون من مئة"),
        (
            SpeechLocale::Ar,
            "4",
            "9876",
            "أربعة وتسعة آلاف وثمان مئة وستة وسبعون من عشرة آلاف",
        ),
    ] {
        let value = NumberValue {
            negative: false,
            integer: integer.into(),
            fraction: Some(fraction.into()),
        };
        let actual = verbalize_number_with_options(
            &value,
            locale,
            NumberReading::Cardinal,
            ReadingStyle::Natural,
            NumberNotation::Phonetic,
        );
        assert_eq!(
            actual.as_deref(),
            Some(gold),
            "{} {integer}.{fraction}",
            locale.code()
        );
    }
}

#[test]
fn formerly_truncated_languages_support_prices_and_lakh_crore_scales() {
    use SpeechLocale::*;
    for (locale, n, gold) in [
        (Ar, "11", "أحد عشر"),
        (Ar, "10000", "عشرة آلاف"),
        (Ar, "100000", "مائة ألف"),
        (Ar, "1000000", "مليون"),
        (Ar, "2000000", "مليونان"),
        (Ar, "3000000", "ثلاثة ملايين"),
        (Ar, "1000000000", "مليار"),
        (Ar, "2000000001", "ملياران وواحد"),
        (Ta, "10000", "பத்து ஆயிரம்"),
        (Ta, "100001", "ஒரு லட்சத்து ஒன்று"),
        (Ta, "200000", "இரண்டு லட்சம்"),
        (Ta, "1000000", "பத்து லட்சம்"),
        (Ta, "10000001", "ஒரு கோடியே ஒன்று"),
        (Te, "10000", "పది వేల"),
        (Te, "100000", "ఒక లక్ష"),
        (Te, "125000", "ఒక లక్ష ఇరవై ఐదు వేల"),
        (Te, "200000", "రెండు లక్షల"),
        (Te, "10000000", "ఒక కోటి"),
        (Te, "20000000", "రెండు కోట్ల"),
    ] {
        assert_eq!(read(n, locale), gold, "{} {n}", locale.code());
    }
    for locale in [Ar, Ta, Te] {
        assert!(cardinal_max(locale) >= 999_999_999);
        assert!(
            verbalize_number(
                &number(&cardinal_max(locale).to_string()),
                locale,
                NumberReading::Cardinal
            )
            .is_some()
        );
    }
}

#[test]
fn zero_uses_cardinal_or_identifier_conventions_without_changing_decimal_digits() {
    use textintel::speech::numbers::verbalize_number_with_options;
    use textintel::speech::{NumberNotation, ReadingStyle};
    assert_eq!(read("0", SpeechLocale::Ko), "영");
    assert_eq!(
        verbalize_number(&number("0"), SpeechLocale::Ko, NumberReading::Digits).as_deref(),
        Some("공")
    );
    assert_eq!(
        verbalize_number_with_options(
            &number("0"),
            SpeechLocale::Ja,
            NumberReading::Cardinal,
            ReadingStyle::Natural,
            NumberNotation::Orthographic
        )
        .as_deref(),
        Some("ゼロ")
    );
    assert_eq!(
        verbalize_number_with_options(
            &number("0"),
            SpeechLocale::Ja,
            NumberReading::Cardinal,
            ReadingStyle::Precise,
            NumberNotation::Orthographic
        )
        .as_deref(),
        Some("零")
    );
    let decimal = NumberValue {
        negative: false,
        integer: "0".into(),
        fraction: Some("05".into()),
    };
    assert_eq!(
        verbalize_number_with_options(
            &decimal,
            SpeechLocale::Ja,
            NumberReading::Cardinal,
            ReadingStyle::Natural,
            NumberNotation::Orthographic
        )
        .as_deref(),
        Some("零点零五")
    );
}

#[test]
fn explicitly_interpreted_counts_strip_padding_while_automatic_identifiers_keep_it() {
    use textintel::speech::numbers::verbalize_with_options;
    use textintel::speech::{SpeechInterpretation, SpeechOptions};
    let padded = number("0003");
    let mut options = SpeechOptions::new(SpeechLocale::Ru);
    assert_eq!(
        verbalize_with_options(&padded, &options, NumberReading::Digits).as_deref(),
        Some("ноль ноль ноль три")
    );
    options.interpret_as = SpeechInterpretation::Number;
    assert_eq!(
        verbalize_with_options(&padded, &options, NumberReading::Digits).as_deref(),
        Some("три")
    );
    options.reading_style = textintel::speech::ReadingStyle::Digits;
    assert_eq!(
        verbalize_with_options(&padded, &options, NumberReading::Cardinal).as_deref(),
        Some("ноль ноль ноль три")
    );
}

#[test]
fn large_feminine_counts_keep_masculine_scale_nouns_and_inflect_smaller_counts() {
    for (locale, n, gold) in [
        (SpeechLocale::Es, "221000", "doscientas veintiuna mil"),
        (SpeechLocale::Es, "200000001", "doscientos millones una"),
        (
            SpeechLocale::Es,
            "1000221",
            "un millón doscientas veintiuna",
        ),
        (
            SpeechLocale::Es,
            "200221001",
            "doscientos millones doscientas veintiuna mil una",
        ),
        (SpeechLocale::Pt, "222000", "duzentas e vinte e duas mil"),
        (SpeechLocale::Pt, "200000002", "duzentos milhões e duas"),
        (
            SpeechLocale::Pt,
            "1000222",
            "um milhão duzentas e vinte e duas",
        ),
        (SpeechLocale::Ru, "22001", "двадцать две тысячи одна"),
        (SpeechLocale::Ru, "2000002", "два миллиона две"),
    ] {
        assert_eq!(
            verbalize_number_feminine(&number(n), locale, NumberReading::Cardinal).as_deref(),
            Some(gold),
            "{} {n}",
            locale.code()
        );
    }
    for locale in [SpeechLocale::Es, SpeechLocale::Pt, SpeechLocale::Ru] {
        assert!(
            verbalize_number_feminine(
                &number(&(cardinal_max(locale) + 1).to_string()),
                locale,
                NumberReading::Cardinal
            )
            .is_none()
        );
    }
}

#[test]
fn trillion_scale_groups_support_large_korean_prices_and_japanese_readings() {
    use textintel::speech::numbers::verbalize_number_with_options;
    use textintel::speech::{NumberNotation, ReadingStyle};
    assert_eq!(read("1000000000000", SpeechLocale::Ko), "일조");
    assert_eq!(
        verbalize_number_with_options(
            &number("30000000000000"),
            SpeechLocale::Ko,
            NumberReading::Cardinal,
            ReadingStyle::Natural,
            NumberNotation::Orthographic
        )
        .as_deref(),
        Some("삼십조")
    );
    assert_eq!(read("1000000000000", SpeechLocale::Ja), "いっちょう");
    assert_eq!(read("8000000000000", SpeechLocale::Ja), "はっちょう");
    assert_eq!(read("10000000000000", SpeechLocale::Ja), "じゅっちょう");
    assert_eq!(
        verbalize_number_with_options(
            &number("1000000000001"),
            SpeechLocale::Ja,
            NumberReading::Cardinal,
            ReadingStyle::Natural,
            NumberNotation::Orthographic
        )
        .as_deref(),
        Some("一兆一")
    );
    let decimal = NumberValue {
        negative: true,
        integer: "0".into(),
        fraction: Some("05".into()),
    };
    assert_eq!(
        verbalize_number_with_options(
            &decimal,
            SpeechLocale::Ko,
            NumberReading::Cardinal,
            ReadingStyle::Natural,
            NumberNotation::Phonetic
        )
        .as_deref(),
        Some("마이너스 영점영오")
    );
}

#[test]
fn explicit_quantity_gender_agrees_with_exact_fractional_digits() {
    use textintel::speech::numbers::verbalize_with_gender;
    for (locale, integer, fraction, gold) in [
        (SpeechLocale::Es, "1", "01", "una coma cero una"),
        (
            SpeechLocale::Es,
            "200001",
            "02",
            "doscientas mil una coma cero dos",
        ),
        (SpeechLocale::Pt, "2", "01", "duas vírgula zero uma"),
    ] {
        let mut options = textintel::speech::SpeechOptions::new(locale);
        options.reading_style = textintel::speech::ReadingStyle::Natural;
        let value = NumberValue {
            negative: false,
            integer: integer.into(),
            fraction: Some(fraction.into()),
        };
        assert_eq!(
            verbalize_with_gender(&value, &options, Some("feminine")).as_deref(),
            Some(gold),
            "{} {integer}.{fraction}",
            locale.code()
        );
    }
}

#[test]
fn shared_integer_core_keeps_regional_presentations_and_fifteen_digit_bounds() {
    use textintel::speech::normalization as tn;
    for (language, locale, source, gold) in [
        ("en", "en-US", "1105", "one thousand one hundred five"),
        ("en", "en-GB", "1105", "one thousand one hundred and five"),
        (
            "en",
            "en-GB",
            "1121",
            "one thousand one hundred and twenty-one",
        ),
        ("en", "en-GB", "1001", "one thousand and one"),
        ("es", "es-PE", "S/ 21", "veintiún soles"),
        (
            "es",
            "es-PE",
            "221.000 GBP",
            "doscientas veintiuna mil libras esterlinas",
        ),
    ] {
        let options = tn::NormalizeOptions::resolve(
            tn::NormalizationMode::Auto,
            language,
            Some(locale),
            tn::NormalizeLimits::default(),
            true,
        )
        .unwrap();
        assert_eq!(
            tn::normalize(source, &options).unwrap().spoken,
            gold,
            "{language} {locale} {source}"
        );
    }
    assert_eq!(read("1000000000000", SpeechLocale::En), "one trillion");
    assert_eq!(read("2000000000001", SpeechLocale::En), "two trillion one");
    assert_eq!(read("1000000000000", SpeechLocale::Es), "un billón");
    assert_eq!(read("2000000000001", SpeechLocale::Es), "dos billones uno");
}

#[test]
fn all_shared_cardinal_adapters_keep_explicit_regional_and_grammatical_choices() {
    use textintel::speech::normalization as tn;
    for (language, locale, source, gold) in [
        ("fr", "fr-FR", "21", "vingt et un"),
        ("fr", "fr-FR", "1000000000000", "un billion"),
        ("de", "de-DE", "201000 EUR", "zweihunderteintausend Euro"),
        ("ru", "ru-RU", "1000 USD", "тысяча долларов"),
        ("zh", "zh-CN", "1000000000000", "一万亿"),
        ("zh", "zh-TW", "1000000000000", "一萬億"),
        ("ja", "ja-JP", "10000000", "一千万"),
        ("ko", "ko-KR", "0", "영"),
        ("ko", "ko-KR", "03:00", "세 시"),
        ("pt", "pt-BR", "16", "dezesseis"),
        ("pt", "pt-PT", "16", "dezasseis"),
        ("pt", "pt-BR", "1000000000000", "um trilhão"),
        ("pt", "pt-PT", "1000000000000", "um bilião"),
    ] {
        let options = tn::NormalizeOptions::resolve(
            tn::NormalizationMode::Auto,
            language,
            Some(locale),
            tn::NormalizeLimits::default(),
            true,
        )
        .unwrap();
        assert_eq!(
            tn::normalize(source, &options).unwrap().spoken,
            gold,
            "{locale} {source}"
        );
    }
    assert_eq!(read("1000000000000", SpeechLocale::De), "eine Billion");
    assert_eq!(read("2000000000000", SpeechLocale::Ru), "два триллиона");
}
