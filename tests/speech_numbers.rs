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
    ] {
        assert_eq!(
            verbalize_number_feminine(&number(n), locale, NumberReading::Cardinal).as_deref(),
            Some(gold)
        );
    }
    assert!(
        verbalize_number_feminine(&number("1000"), SpeechLocale::Es, NumberReading::Cardinal)
            .is_none()
    );
    assert!(
        verbalize_number_feminine(&number("1"), SpeechLocale::Ar, NumberReading::Cardinal)
            .is_none()
    );
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
