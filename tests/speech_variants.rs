use textintel::speech::variants::{compare_spoken, rule_catalog};
use textintel::speech::{
    NumberNotation, ReadingStyle, SpeechLocale, SpeechOptions, SpeechValue, normalize_for_speech,
    parse_for_speech,
};

fn options(locale: SpeechLocale) -> SpeechOptions {
    let mut options = SpeechOptions::new(locale);
    options.reading_style = ReadingStyle::Natural;
    options.number_notation = NumberNotation::Orthographic;
    options.decimal_separator = Some('.');
    options
}
fn check(source: &str, options: &SpeechOptions, reference: &str, rule: &str) {
    let document = parse_for_speech(source, options).unwrap();
    let actual = normalize_for_speech(source, options).unwrap();
    assert!(actual.fully_supported, "{source}: {actual:?}");
    let matched = compare_spoken(&document, options, &actual.text, reference)
        .unwrap_or_else(|| panic!("{source}: {:?} != {reference:?}", actual.text));
    assert!(matched.rules.iter().any(|r| r == rule), "{matched:?}");
    assert!(!matched.rules.is_empty());
}
fn reject(source: &str, options: &SpeechOptions, reference: &str) {
    let document = parse_for_speech(source, options).unwrap();
    let actual = normalize_for_speech(source, options).unwrap();
    assert!(
        compare_spoken(&document, options, &actual.text, reference).is_none(),
        "{source}: {:?} wrongly accepted {reference:?}",
        actual.text
    );
}

#[test]
fn exact_is_separate_and_only_canonical_unicode_equivalence_is_accepted() {
    let opts = options(SpeechLocale::En);
    reject("42", &opts, "forty two");
    check("café 42", &opts, "cafe\u{301} forty two", "unicode_nfc");
    check("42", &opts, " \tforty two\n", "outer_whitespace");
    reject("café 42", &opts, "cafe forty two");
    reject("ABC 42", &opts, "ＡＢＣ forty two");
    reject("I have 42 apples", &opts, "i have forty-two apples");
    // Whitespace is not collapsed in prose.
    reject("I have 42 apples", &opts, "I  have forty two apples");
}

#[test]
fn english_hyphen_and_hundred_and_are_local_numeric_rules() {
    let opts = options(SpeechLocale::En);
    check(
        "123",
        &opts,
        "one hundred twenty-three",
        "english_hundred_and",
    );
    check("42", &opts, "forty-two", "numeric_hyphen_en_fr");
    check(
        "I have 123 apples",
        &opts,
        "I have one hundred twenty-three apples",
        "english_hundred_and",
    );
    reject("123", &opts, "one hundred twenty-four");
    reject("-42", &opts, "forty-two");
    reject("-42", &opts, "-forty-two");
    reject("123 kg", &opts, "one hundred twenty-three grams");
    reject("123 USD", &opts, "one hundred twenty-three euros");
    reject("100 1/2", &opts, "one hundred one half");
    reject("100 1", &opts, "one hundred and one");
    reject("1001", &opts, "one thousand one"); // No unverified thousand-and rule.
}

#[test]
fn english_literal_label_separator_keeps_label_numeric_sign_and_precision_exact() {
    let mut opts = options(SpeechLocale::En);
    opts.interpret_as = textintel::speech::SpeechInterpretation::Measure;
    for (source, reference) in [
        ("volume-27.50", "volume- twenty seven point five zero"),
        ("version-8.02", "version-eight point zero two"),
        ("volume--2.50", "volume- minus two point five zero"),
        ("27-channel", "twenty seven- channel"),
        ("Channel-7", "Channel- seven"),
    ] {
        check(source, &opts, reference, "english_literal_label_separator");
    }
    reject("version-8.02", &opts, "version- eight point two");
    reject("version-8.02", &opts, "version- eight point zero three");
    reject("version-8.02", &opts, "revision- eight point zero two");
    reject("Channel-7", &opts, "channel- seven");
    reject("volume--2.50", &opts, "volume- two point five zero");
    reject("volume-2.50", &opts, "volume- minus two point five zero");
    reject(
        "volume-27.50",
        &options(SpeechLocale::En),
        "volume- twenty seven point five zero",
    );
    reject("new-age volume-27", &opts, "new age volume- twenty seven");
    reject("100 kg", &opts, "one hundred kilograms");
}

#[test]
fn spanish_exact_hour_noun_preserves_clock_fields_and_qualifiers() {
    let mut opts = options(SpeechLocale::Es);
    opts.interpret_as = textintel::speech::SpeechInterpretation::Time;
    for (source, reference) in [
        ("2 h", "dos horas"),
        ("7 h", "siete horas"),
        ("1 h", "una hora"),
        ("00:00", "cero horas"),
        ("22:00", "veintidós horas"),
        ("02:00 PM", "dos horas de la tarde"),
        ("02:00 AM", "dos horas de la mañana"),
        ("02:00:00", "dos horas y cero segundos"),
    ] {
        check(source, &opts, reference, "spanish_whole_hour_noun");
    }
    reject("2 h", &opts, "dos hora");
    reject("1 h", &opts, "una horas");
    reject("7 h", &opts, "ocho horas");
    reject("02:00 PM", &opts, "dos horas");
    reject("02:00 PM", &opts, "dos horas de la mañana");
    reject("02:00:00", &opts, "dos horas");
    reject("02:01", &opts, "dos horas y uno");
    reject("02:00:01", &opts, "dos horas y uno segundos");
    let mut duration = opts.clone();
    duration.interpret_as = textintel::speech::SpeechInterpretation::Duration;
    reject("2h", &duration, "dos");
}

#[test]
fn french_traditional_and_rectified_numeric_hyphenation_share_words() {
    let opts = options(SpeechLocale::Fr);
    check("71", &opts, "soixante-et-onze", "numeric_hyphen_en_fr");
    reject("71", &opts, "soixante-et-douze");
    reject("-71", &opts, "soixante-et-onze");
    reject("porte-monnaie 71", &opts, "porte monnaie soixante-et-onze");
}

#[test]
fn french_colloquial_decade_loanword_retains_an_explicit_century() {
    let mut opts = options(SpeechLocale::Fr);
    reject("1980s", &opts, "eighties"); // Auto recognizes the explicit seconds unit.
    opts.interpret_as = textintel::speech::SpeechInterpretation::Date;
    check("1980s", &opts, "eighties", "french_1980s_loanword");
    check("les 1980s", &opts, "les eighties", "french_1980s_loanword");
    reject("les 80s", &opts, "les eighties");
    reject("1880s", &opts, "eighties");
    reject("2080s", &opts, "eighties");
    reject("1980", &opts, "eighties");
    reject("1980s", &opts, "seventies");
    reject("les 1980s", &opts, "des eighties");
}

#[test]
fn german_preserves_exact_compound_letters_and_unit_case_semantics() {
    let opts = options(SpeechLocale::De);
    check("42", &opts, "zwei und vierzig", "german_compound_spacing");
    check("42", &opts, "ZWEIUNDVIERZIG", "german_numeric_case");
    let actual = normalize_for_speech("42 kg", &opts).unwrap().text;
    assert!(actual.contains("Kilogramm"), "{actual}");
    check(
        "42 kg",
        &opts,
        &actual.replace("Kilogramm", "kilogramm"),
        "german_numeric_case",
    );
    reject("42", &opts, "zwei und fünfzig");
    reject("42", &opts, "z w e i und vierzig");
    reject("42 kg", &opts, "zwei und vierzig Gramm");
    reject("1 2", &opts, "einszwei");
    reject("Berlin 42", &opts, "berlin zwei und vierzig");
    // Unknown letter/label spelling is opaque, not lowercased as a unit.
    let mut labelled = opts.clone();
    labelled
        .unit_aliases
        .insert("MB".into(), "digital-megabyte".into());
    let actual = normalize_for_speech("2 MB", &labelled).unwrap().text;
    reject("2 MB", &labelled, &actual.replace("Megabyte", "Megabit"));
}

#[test]
fn german_ordinal_and_typed_area_volume_compounds_preserve_every_component() {
    let mut opts = options(SpeechLocale::De);
    opts.decimal_separator = Some(',');
    check(
        "21. juni 2025",
        &opts,
        "ein und zwanzigster juni zwei tausend fünf und zwanzig",
        "german_compound_spacing",
    );
    check(
        "2,5 /cm³",
        &opts,
        "zwei komma fünf pro kubik zentimeter",
        "german_unit_compound_spacing",
    );
    check(
        "81 /m²",
        &opts,
        "ein und achtzig pro quadrat meter",
        "german_unit_compound_spacing",
    );
    reject("2,5 /cm³", &opts, "zwei komma fünf pro quadrat zentimeter");
    reject("2,5 /cm³", &opts, "zwei komma fünf pro kubik meter");
    reject("81 /m²", &opts, "ein und achtzig pro q u a d r a t meter");
    reject(
        "21. juni 2025",
        &opts,
        "zwei und zwanzigster juni zwei tausend fünf und zwanzig",
    );
}

#[test]
fn cjk_layout_does_not_change_units_or_split_unit_words() {
    let zh = options(SpeechLocale::Zh);
    check("42", &zh, "四 十 二", "cjk_numeric_unit_spacing");
    check("2024年", &zh, "二 零 二 四 年", "cjk_numeric_unit_spacing");
    reject("42", &zh, "四 十 三");
    reject("-42", &zh, "四 十 二");
    reject("42公里", &zh, "四十二公 里");
    reject("42公里", &zh, "四十二米");
    let ja = options(SpeechLocale::Ja);
    check("42円", &ja, "四十二 円", "cjk_numeric_unit_spacing");
    reject("42円", &ja, "四十二 元");
    reject("42", &options(SpeechLocale::Ko), "사 십 이");
}

#[test]
fn japanese_one_thousand_is_only_optional_at_group_boundaries() {
    let opts = options(SpeechLocale::Ja);
    check("1000", &opts, "一千", "japanese_initial_one_thousand");
    check(
        "12345000",
        &opts,
        "一千二百三十四万五千",
        "japanese_initial_one_thousand",
    );
    check("11000", &opts, "一万一千", "japanese_initial_one_thousand");
    reject("2000", &opts, "二一千");
    reject("1000", &opts, "一百");
    reject("-1000", &opts, "一千");
}

#[test]
fn standalone_japanese_notation_is_generated_from_identical_numeric_ir() {
    let mut opts = SpeechOptions::new(SpeechLocale::Ja);
    opts.decimal_separator = Some('.');
    for style in [ReadingStyle::Precise, ReadingStyle::Natural] {
        opts.reading_style = style;
        for (source, reference) in [
            ("2468", "二千四百六十八"),
            ("-0.050", "マイナス零点零五零"),
            ("007", "零零七"),
            (" 2468 ", "二千四百六十八"),
        ] {
            check(
                source,
                &opts,
                reference,
                "alternative_japanese_number_notation",
            );
        }
        reject("2468", &opts, "二千四百六十九");
        reject("-0.050", &opts, "零点零五零");
        reject("-0.050", &opts, "マイナス零点零五");
        reject("007", &opts, "七");
        reject("007", &opts, "零七");
        reject("四十二", &opts, "よんじゅうに");
        reject("4人", &opts, "四人");
        reject("42kg", &opts, "四十二キログラム");
        reject("42円", &opts, "四十二円");
        reject("7時", &opts, "七時");
        reject("4月", &opts, "四月");
        reject("07:00 PM", &opts, "七時零分");
        reject("count 42", &opts, "count 四十二");
        reject("42.00", &opts, "四十二点零");
        reject("3kg.txt", &opts, "三キログラム.txt");
    }
    opts.reading_style = ReadingStyle::Digits;
    check("42", &opts, "四二", "alternative_japanese_number_notation");
    reject("42", &opts, "四十二");
    opts.reading_style = ReadingStyle::Natural;
    opts.interpret_as = textintel::speech::SpeechInterpretation::Number;
    reject("2468", &opts, "二千四百六十八");
}

#[test]
fn generated_japanese_notation_composes_only_with_proven_numeric_presentation() {
    let opts = SpeechOptions::new(SpeechLocale::Ja);
    let document = parse_for_speech("1234", &opts).unwrap();
    let actual = normalize_for_speech("1234", &opts).unwrap().text;
    let matched = compare_spoken(&document, &opts, &actual, "一 千 二 百 三 十 四").unwrap();
    for required in [
        "alternative_japanese_number_notation",
        "cjk_numeric_unit_spacing",
        "japanese_initial_one_thousand",
    ] {
        assert!(
            matched.rules.iter().any(|rule| rule == required),
            "{matched:?}"
        );
    }
    check(
        "-1000",
        &opts,
        "マイナス一千",
        "alternative_japanese_number_notation",
    );
    reject("1234", &opts, "一 千 二 百 三 十 五");
    reject("2000", &opts, "二 一 千");
    reject("-1000", &opts, "一 千");
    reject("007", &opts, "零 七");
    let mut decimal = opts;
    decimal.decimal_separator = Some('.');
    reject("1.050", &decimal, "一 点 零 五");
}

#[test]
fn korean_spacing_is_limited_to_groups_and_the_intact_typed_unit() {
    let opts = options(SpeechLocale::Ko);
    check(
        "123000012",
        &opts,
        "일억 이천삼백만 십이",
        "korean_number_group_spacing",
    );
    check(
        "12kg",
        &opts,
        "십이킬로그램",
        "korean_quantity_unit_spacing",
    );
    check("0kg", &opts, "영킬로그램", "korean_quantity_unit_spacing");
    reject("12kg", &opts, "십이 킬로 그램");
    reject("12kg", &opts, "십 이 킬로그램");
    reject("12kg", &opts, "십이 그램");
    reject("123000012", &opts, "일억 이천삼백만 십삼");
    reject("42", &opts, "사 십 이");
}

#[test]
fn arabic_only_accepts_the_documented_hundred_spelling_pair() {
    let opts = options(SpeechLocale::Ar);
    check("100", &opts, "مائة", "arabic_hundred_spelling");
    let actual = normalize_for_speech("3000", &opts).unwrap().text;
    assert!(actual.contains("آلاف"), "{actual}");
    reject("3000", &opts, &actual.replace("آلاف", "اَلاف"));
    reject("100", &opts, "مِئة");
    reject("100", &opts, "مئتان");
    reject("نص مائة 42", &opts, "نص مئة اثنان وأربعون");
}

#[test]
fn vietnamese_linker_is_not_a_global_word_or_diacritic_rewrite() {
    let opts = options(SpeechLocale::Vi);
    check(
        "101",
        &opts,
        "một trăm linh một",
        "vietnamese_zero_tens_linker",
    );
    check(
        "905",
        &opts,
        "chín trăm linh năm",
        "vietnamese_zero_tens_linker",
    );
    reject("101", &opts, "một trăm một");
    reject("101", &opts, "một trăm linh hai");
    reject("Linh 101", &opts, "Lẻ một trăm linh một");
    reject("1000001", &opts, "một triệu linh một"); // Narrow reviewed scope.
}

#[test]
fn vietnamese_si_syllable_spacing_comes_from_the_typed_unit() {
    let opts = options(SpeechLocale::Vi);
    for (source, reference) in [
        ("2kg", "hai ki lô gam"),
        ("200mg", "hai trăm mi li gam"),
        ("6km", "sáu ki lô mét"),
        ("10mm", "mười mi li mét"),
        ("7ml", "bảy mi li lít"),
    ] {
        check(
            source,
            &opts,
            reference,
            "vietnamese_unit_component_spacing",
        );
    }
    reject("2kg", &opts, "hai k i l ô g a m");
    reject("2kg", &opts, "hai ki lô mét");
    reject("6km", &opts, "sáu ki lo mét");
    reject("175cm", &opts, "một trăm bảy mươi lăm xăng ti mết");
    reject("kilôgam 2", &opts, "ki lô gam hai");
}

#[test]
fn vietnamese_centimeter_aliases_are_typed_source_documented_terms() {
    let opts = options(SpeechLocale::Vi);
    for (n, spoken) in [(2, "hai"), (7, "bảy"), (23, "hai mươi ba")] {
        for alias in ["xăng-ti-mét", "xăng ti mét"] {
            let source = format!("{n} {alias}");
            let document = parse_for_speech(&source, &opts).unwrap();
            assert_eq!(document.spans.len(), 1, "{document:?}");
            assert!(
                matches!(&document.spans[0].value, SpeechValue::Measure { unit, .. } if unit == "length-centimeter")
            );
            check(
                &source,
                &opts,
                &format!("{spoken} {alias}"),
                "vietnamese_typed_unit_alias",
            );
            check(
                &format!("{n}cm"),
                &opts,
                &format!("{spoken} {alias}"),
                "vietnamese_typed_unit_alias",
            );
        }
    }
    reject("7cm", &opts, "bảy xăng ti mét vuông");
    reject("7m", &opts, "bảy xăng ti mét");
    reject("xentimét 7", &opts, "xăng ti mét bảy");
}

#[test]
fn alternative_digit_policy_reuses_the_same_exact_integer_for_all_twenty_locales() {
    use textintel::speech::{NumberReading, NumberValue, SpeechInterpretation};
    for locale in SpeechLocale::ALL {
        let mut opts = options(locale);
        opts.interpret_as = SpeechInterpretation::Number;
        let value = NumberValue {
            integer: "205".into(),
            negative: false,
            fraction: None,
        };
        let digits = textintel::speech::numbers::verbalize_number_with_options(
            &value,
            locale,
            NumberReading::Cardinal,
            ReadingStyle::Digits,
            opts.number_notation,
        )
        .unwrap();
        check("205", &opts, &digits, "alternative_integer_digit_policy");
    }
    let mut opts = options(SpeechLocale::En);
    opts.interpret_as = SpeechInterpretation::Number;
    check(
        "205",
        &opts,
        "two zero five",
        "alternative_integer_digit_policy",
    );
    check(
        "-205",
        &opts,
        "minus two zero five",
        "alternative_integer_digit_policy",
    );
    check(
        "100001",
        &opts,
        "one zero zero zero zero one",
        "alternative_integer_digit_policy",
    );
    reject("205", &options(SpeechLocale::En), "two zero five");
    reject("0205", &opts, "zero two zero five");
    reject("205", &opts, "two zero zero five");
    reject("-205", &opts, "two zero five");
    reject("205.0", &opts, "two zero five point zero");
    reject("205kg", &opts, "two zero five kilograms");
}

#[test]
fn russian_clock_variants_are_derived_from_fields_and_keep_precision_and_periods() {
    let opts = options(SpeechLocale::Ru);
    for (source, reference, rule) in [
        (
            "14:06",
            "четырнадцать часов шесть минут",
            "russian_clock_conjunction",
        ),
        ("02:00", "два часа ноль минут", "russian_clock_zero_minutes"),
        ("02:08", "восемь минут третьего", "russian_clock_relative"),
        ("03:08", "восемь минут четвертого", "russian_clock_relative"),
        ("04:01", "одна минута пятого", "russian_clock_relative"),
        ("03:58", "без двух минут четыре", "russian_clock_relative"),
        ("12:55", "без пяти час", "russian_clock_relative"),
        (
            "07:39",
            "без двадцати одной минуты восемь",
            "russian_clock_relative",
        ),
        ("11:45", "без четверти двенадцать", "russian_clock_relative"),
        ("06:30", "половина седьмого", "russian_clock_relative"),
        ("06:30", "полседьмого", "russian_clock_relative"),
        ("09:15", "четверть десятого", "russian_clock_relative"),
    ] {
        check(source, &opts, reference, rule);
    }
    reject("03:58", &opts, "без двух минут пять");
    reject("12:55", &opts, "без пяти один");
    reject("07:39", &opts, "без двадцати одной минут восемь");
    reject("14:06", &opts, "шесть минут третьего");
    reject("00:45", &opts, "без пятнадцати час");
    reject("05:45:00", &opts, "без пятнадцати минут шесть");
    reject("05:45 PM", &opts, "без пятнадцати минут шесть");
    reject("05:45 AM", &opts, "без пятнадцати минут шесть");
    reject("14:06", &opts, "четырнадцать часов и семь минут");
}

#[test]
fn dollar_qualification_requires_the_exact_iso_currency() {
    let opts = options(SpeechLocale::Ru);
    check(
        "25000 USD",
        &opts,
        "двадцать пять тысяч долларов сэ шэ а",
        "russian_usd_qualification",
    );
    reject("25000 CAD", &opts, "двадцать пять тысяч долларов сэ ш а");
    reject("25000 AUD", &opts, "двадцать пять тысяч долларов сэ ш а");
    reject("25000 USD", &opts, "двадцать четыре тысячи долларов сэ ш а");
    reject("25000 USD", &opts, "двадцать пять тысяч долларов канады");
    reject("25000 USD", &opts, "двадцать пять тысяч долларов сэ ш а");
    check(
        "25000 USD",
        &opts,
        "двадцать пять тысяч долларов США",
        "russian_usd_qualification",
    );
    check(
        "25000 USD",
        &opts,
        "двадцать пять тысяч долларов сэ-шэ-а",
        "russian_usd_qualification",
    );
}

#[test]
fn russian_zero_minute_equivalence_is_symmetric_and_keeps_written_seconds() {
    let mut opts = SpeechOptions::new(SpeechLocale::Ru);
    for (source, reference) in [
        ("02:00", "два часа"),
        ("12:00", "двенадцать часов"),
        ("14:00", "четырнадцать часов"),
        ("Встреча в 02:00.", "Встреча в два часа."),
    ] {
        check(source, &opts, reference, "russian_clock_zero_minutes");
    }
    for (source, reference) in [
        ("02:01", "два часа"),
        ("02:00:00", "два часа"),
        ("02:00:01", "два часа"),
        ("14:00", "два часа"),
        ("02:00 PM", "два часа"),
        ("02:00 AM", "два часа"),
        ("02:15 AM", "четверть третьего"),
        ("Встреча в 02:00.", "встреча в два часа."),
        ("`02:00`", "`два часа`"),
    ] {
        reject(source, &opts, reference);
    }
    opts.reading_style = ReadingStyle::Natural;
    check(
        "02:00",
        &opts,
        "два часа и ноль минут",
        "russian_clock_zero_minutes",
    );
}

#[test]
fn currency_minor_unit_policy_changes_only_style_and_preserves_exact_source_ir() {
    for (locale, source, precise, natural) in [
        (
            SpeechLocale::De,
            "EUR 2,07",
            "zwei Komma null sieben Euro",
            "zwei euro sieben cent",
        ),
        (
            SpeechLocale::De,
            "-23,40 EUR",
            "minus dreiundzwanzig Komma vier null Euro",
            "minus dreiundzwanzig euro vierzig cent",
        ),
        (
            SpeechLocale::De,
            "EUR 0,05",
            "null Komma null fünf Euro",
            "fünf cent",
        ),
        (
            SpeechLocale::Pt,
            "BRL 23,07",
            "vinte e três vírgula zero sete Reais brasileiros",
            "vinte e três reais e sete centavos",
        ),
        (
            SpeechLocale::Pt,
            "-2,40 BRL",
            "menos dois vírgula quatro zero Reais brasileiros",
            "menos dois reais e quarenta centavos",
        ),
        (
            SpeechLocale::Pt,
            "BRL 0,08",
            "zero vírgula zero oito Real brasileiro",
            "oito centavos",
        ),
    ] {
        let mut opts = SpeechOptions::new(locale);
        opts.decimal_separator = Some(',');
        let doc = parse_for_speech(source, &opts).unwrap();
        assert_eq!(normalize_for_speech(source, &opts).unwrap().text, precise);
        check(
            source,
            &opts,
            natural,
            "alternative_currency_minor_unit_policy",
        );
        opts.reading_style = ReadingStyle::Natural;
        assert_eq!(parse_for_speech(source, &opts).unwrap(), doc);
        assert_eq!(normalize_for_speech(source, &opts).unwrap().text, natural);
        check(
            source,
            &opts,
            precise,
            "alternative_currency_minor_unit_policy",
        );
    }
    let mut de = SpeechOptions::new(SpeechLocale::De);
    de.decimal_separator = Some(',');
    for wrong in [
        "zwei euro siebzig cent",
        "drei euro sieben cent",
        "minus zwei euro sieben cent",
        "zwei dollar sieben cent",
    ] {
        reject("EUR 2,07", &de, wrong);
    }
    reject("EUR 2,070", &de, "zwei euro sieben cent");
    reject("EUR 2,00", &de, "zwei euro");
    reject("2,07", &de, "zwei euro sieben cent");
    reject("2,07 kg", &de, "zwei euro sieben cent");
    reject("EUR 2,07.txt", &de, "zwei euro sieben cent");
    reject("Preis EUR 2,07", &de, "Preis zwei euro sieben cent");
    de.interpret_as = textintel::speech::SpeechInterpretation::Number;
    reject("EUR 2,07", &de, "zwei euro sieben cent");
    de.interpret_as = textintel::speech::SpeechInterpretation::Auto;
    de.currency_naming = textintel::speech::CurrencyNaming::Full;
    let full = normalize_for_speech("EUR 2,07", &de).unwrap();
    assert!(full.fully_supported);
    reject("EUR 2,07", &de, "zwei Komma null sieben Euro");
    let mut pt = SpeechOptions::new(SpeechLocale::Pt);
    pt.decimal_separator = Some(',');
    reject("BRL 2,070", &pt, "dois reais e sete centavos");
    reject("USD 2,07", &pt, "dois reais e sete centavos");
    pt.reading_style = ReadingStyle::Digits;
    reject("BRL 23,07", &pt, "vinte e três reais e sete centavos");
    pt.reading_style = ReadingStyle::Precise;
    pt.currency_naming = textintel::speech::CurrencyNaming::Full;
    reject("BRL 23,07", &pt, "vinte e três reais e sete centavos");
}

#[test]
fn chinese_currency_spacing_uses_the_exact_iso_names_without_style_changes() {
    let opts = SpeechOptions::new(SpeechLocale::Zh);
    for (source, reference) in [
        ("23 EUR", "二十三欧元"),
        ("-7 CAD", "负七加拿大元"),
        ("7 USD", "七美元"),
        ("7 AUD", "七澳大利亚元"),
        ("23 GBP", "二十三英镑"),
    ] {
        check(source, &opts, reference, "chinese_currency_name_spacing");
    }
    reject("7 CAD", &opts, "七美元");
    reject("7 AUD", &opts, "七加拿大元");
    reject("7 USD", &opts, "七加拿大元");
    reject("-7 CAD", &opts, "七加拿大元");
    reject("23 EUR", &opts, "二十四欧元");
    reject("23 EUR", &opts, "二十三欧 元");
    reject("23.07 EUR", &opts, "二十三点七欧元");
    reject("23.07 EUR", &opts, "二十三点零八欧元");
    reject("价格23 EUR", &opts, "价格二十三欧元");
    reject("23EUR.txt", &opts, "二十三欧元");
    reject("二十三 欧元", &opts, "二十三欧元");
    let mut full = opts.clone();
    full.currency_naming = textintel::speech::CurrencyNaming::Full;
    reject("7 CAD", &full, "七美元");
}

#[test]
fn regional_arabic_currency_units_compose_reviewed_masculine_names_and_complete_amounts() {
    use textintel::speech::CurrencyNaming;
    let rule = "arabic_regional_currency_unit_singular";
    for style in [ReadingStyle::Precise, ReadingStyle::Natural] {
        let mut opts = options(SpeechLocale::Ar);
        opts.reading_style = style;
        opts.currency_symbols.insert("$".into(), "USD".into());
        for (source, canonical, regional) in [
            ("$7", "سبعة دولارات", "سبعة دولار"),
            ("5د.ك", "خمسة دنانير كويتية", "خمسة دينار كويتي"),
            ("USD-9", "سالب تسعة دولارات", "سالب تسعة دولار"),
        ] {
            assert_eq!(normalize_for_speech(source, &opts).unwrap().text, canonical);
            check(source, &opts, regional, rule);
            // The comparator never accepts a noncanonical `actual` value.
            let parsed = parse_for_speech(source, &opts).unwrap();
            assert!(compare_spoken(&parsed, &opts, regional, canonical).is_none());
        }
        // Independent lexical expectations over the productive3–10 domain,
        // rather than looking up whole fixture utterances or changing prefixes.
        for (amount, number) in [
            (3, "ثلاثة"),
            (4, "أربعة"),
            (5, "خمسة"),
            (6, "ستة"),
            (7, "سبعة"),
            (8, "ثمانية"),
            (9, "تسعة"),
            (10, "عشرة"),
        ] {
            for (code, singular) in [
                ("USD", "دولار"),
                ("GBP", "جنيه"),
                ("KWD", "دينار كويتي"),
                ("SAR", "ريال"),
                ("AED", "درهم"),
                ("RUB", "روبل"),
                ("CHF", "فرنك"),
            ] {
                check(
                    &format!("{code}{amount}"),
                    &opts,
                    &format!("{number} {singular}"),
                    rule,
                );
            }
        }
        check(" \tUSD7\n", &opts, " سبعة دولار ", rule);
        opts.currency_naming = CurrencyNaming::Full;
        for (code, qualified_name) in [
            ("USD", "دولار أمريكي"),
            ("GBP", "جنيه إسترليني"),
            ("KWD", "دينار كويتي"),
            ("SAR", "ريال سعودي"),
            ("AED", "درهم إماراتي"),
            ("RUB", "روبل روسي"),
            ("CHF", "فرنك سويسري"),
            ("CAD", "دولار كندي"),
            ("AUD", "دولار أسترالي"),
            ("NZD", "دولار نيوزيلندي"),
            ("SGD", "دولار سنغافوري"),
            ("TWD", "دولار تايواني"),
            ("BRL", "ريال برازيلي"),
            ("EGP", "جنيه مصري"),
        ] {
            check(
                &format!("{code}4"),
                &opts,
                &format!("أربعة {qualified_name}"),
                rule,
            );
            check(
                &format!("{code}-6"),
                &opts,
                &format!("سالب ستة {qualified_name}"),
                rule,
            );
        }
    }
}

#[test]
fn regional_arabic_currency_units_reject_unreviewed_register_mixes_and_precision_changes() {
    let mut opts = options(SpeechLocale::Ar);
    opts.currency_symbols.insert("$".into(), "USD".into());
    for (source, reference) in [
        ("$7.05", "سبعة دولار وخمسة سنتات"),
        ("$7.50", "سبعة دولار وخمسون سنت"),
        ("$7.99", "سبعة دولار وتسعة وتسعون سنت"),
        ("$0.99", "تسعة وتسعون سنت"),
        ("EUR0.99", "تسعة وتسعون سنت"),
        ("USD7.0", "سبعة دولار"),
        ("USD7.00", "سبعة دولار"),
        ("USD7.000", "سبعة دولار"),
        ("USD7.01", "سبعة دولار"),
        ("USD1", "واحد دولار"),
        ("USD2", "اثنان دولار"),
        ("USD01", "واحد دولار"),
        ("USD02", "اثنان دولار"),
        ("USD11", "أحد عشر دولار"),
        ("USD99", "تسعة وتسعون دولار"),
        ("USD-7", "سبعة دولار"),
        ("USD7", "سالب سبعة دولار"),
        ("USD7", "ثمانية دولار"),
        ("USD7", "سبعة دينار"),
        ("USD7", "سبعة دولار أمريكي"),
        ("السعر USD7", "السعر سبعة دولار"),
        ("USD7 USD3", "سبعة دولار ثلاثة دولار"),
        ("INR3", "ثلاث روبية"),
        ("INR3", "ثلاثة روبية"),
        ("PKR5", "خمسة روبية"),
        ("IDR6", "ستة روبية"),
        ("TRY9", "تسعة ليرة"),
        ("JPY3", "ثلاثة ين"),
        ("PLN4", "أربعة زلوتي"),
    ] {
        reject(source, &opts, reference);
    }
    opts.unit_aliases
        .insert("f".into(), "temperature-fahrenheit".into());
    reject("f9", &opts, "تسعة فهرنهايت");
    opts.currency_naming = textintel::speech::CurrencyNaming::Full;
    reject("USD7", &opts, "سبعة دولار");
    reject("USD7", &opts, "سبعة دولار كندي");
    reject("CAD7", &opts, "سبعة دولار أمريكي");
    reject("KWD5", &opts, "خمسة دينار");
    reject("INR3", &opts, "ثلاث روبية هندية");
    opts.reading_style = ReadingStyle::Digits;
    reject("USD7", &opts, "سبعة دولار أمريكي");
}

#[test]
fn regional_arabic_currency_units_require_the_exact_canonical_locale_and_source_ir() {
    let opts = options(SpeechLocale::Ar);
    let document = parse_for_speech("USD7", &opts).unwrap();
    let canonical = normalize_for_speech("USD7", &opts).unwrap().text;
    let mut forged = document.clone();
    if let SpeechValue::Currency { value, .. } = &mut forged.spans[0].value {
        value.integer = "8".into();
    } else {
        panic!("expected Currency");
    }
    assert!(compare_spoken(&forged, &opts, "ثمانية دولارات", "ثمانية دولار").is_none());
    let mut forged = document.clone();
    if let SpeechValue::Currency { code, .. } = &mut forged.spans[0].value {
        *code = "SAR".into();
    }
    assert!(compare_spoken(&forged, &opts, "سبعة ريالات", "سبعة ريال").is_none());
    let mut forged = document.clone();
    forged.locale = SpeechLocale::En;
    assert!(compare_spoken(&forged, &opts, &canonical, "سبعة دولار").is_none());
    let english = options(SpeechLocale::En);
    let foreign = parse_for_speech("USD7", &english).unwrap();
    let actual = normalize_for_speech("USD7", &english).unwrap();
    assert!(compare_spoken(&foreign, &english, &actual.text, "سبعة دولار").is_none());
    assert!(compare_spoken(&foreign, &opts, &canonical, "سبعة دولار").is_none());
}

#[test]
fn generated_currency_policies_never_accept_changed_assets_or_values_in_twenty_locales() {
    for locale in SpeechLocale::ALL {
        let mut opts = SpeechOptions::new(locale);
        opts.decimal_separator = Some('.');
        let mut natural = opts.clone();
        natural.reading_style = ReadingStyle::Natural;
        for wrong_source in ["EUR 3.07", "EUR -2.07", "USD 2.07"] {
            let wrong = normalize_for_speech(wrong_source, &natural).unwrap();
            reject("EUR 2.07", &opts, &wrong.text);
        }
        reject("EUR 2.07.txt", &opts, "two euros seven cents");
        opts.invalid_date_policy = textintel::speech::InvalidDatePolicy::ReadComponents;
        opts.date_order = textintel::speech::DateOrder::DayMonthYear;
        let invalid = normalize_for_speech("30/02/2023", &opts).unwrap();
        assert!(invalid.fully_supported, "{locale:?}");
        assert!(parse_for_speech("30/02/2023", &opts).unwrap().spans.iter().any(|span| matches!(span.value,
            SpeechValue::Temporal { value: textintel::speech::temporal::TemporalValue::InvalidCalendarDate { .. } }
        )));
        reject("30/02/2023", &opts, &format!(" {} ", invalid.text));
    }
}

#[test]
fn russian_yo_variants_are_closed_numeric_words_and_leave_prose_exact() {
    let mut opts = options(SpeechLocale::Ru);
    opts.interpret_as = textintel::speech::SpeechInterpretation::Measure;
    check(
        "4-х килограммовый мешок",
        &opts,
        "четырех килограммовый мешок",
        "russian_numeric_yo",
    );
    check(
        "324-х",
        &opts,
        "трехсот двадцати четырех",
        "russian_numeric_yo",
    );
    reject("всё 4-х", &opts, "все четырех");
    reject("4-х", &opts, "трех");
    reject("4-х", &options(SpeechLocale::Ru), "четырех");
}

#[test]
fn unsupported_invalid_or_forged_sources_cannot_be_variants() {
    let opts = options(SpeechLocale::En);
    for source in ["$2", "https://example.org/42", "2024-02-30", "1/0"] {
        let actual = normalize_for_speech(source, &opts).unwrap().text;
        reject(source, &opts, &format!(" {actual} "));
    }
    let mut document = parse_for_speech("42", &opts).unwrap();
    let actual = normalize_for_speech("42", &opts).unwrap().text;
    assert!(compare_spoken(&document, &options(SpeechLocale::Fr), &actual, "forty-two").is_none());
    assert!(compare_spoken(&document, &opts, "forty three", "forty-three").is_none());
    document.spans[0].value = SpeechValue::Text;
    assert!(compare_spoken(&document, &opts, "42", " 42 ").is_none());
}

#[test]
fn catalog_and_limits_are_auditable() {
    let mut ids = std::collections::BTreeSet::new();
    for rule in rule_catalog() {
        assert!(ids.insert(rule.id), "duplicate {}", rule.id);
        assert!(!rule.rationale.is_empty());
        assert!(!rule.sources.is_empty());
        assert!(rule.sources.iter().all(|url| url.starts_with("https://")));
    }
    let opts = options(SpeechLocale::En);
    let document = parse_for_speech("42", &opts).unwrap();
    assert!(compare_spoken(&document, &opts, "forty two", &" ".repeat(65537)).is_none());
    let mut large = opts;
    large.max_input_chars = 9000;
    let source = "a".repeat(8193);
    let document = parse_for_speech(&source, &large).unwrap();
    assert!(compare_spoken(&document, &large, &source, &format!(" {source}")).is_none());
}

#[test]
fn german_zero_minute_clock_policy_is_symmetric_and_proves_compact_ir_equivalence() {
    let rule = "alternative_german_clock_zero_minute_policy";
    for notation in [NumberNotation::Phonetic, NumberNotation::Orthographic] {
        let mut precise = SpeechOptions::new(SpeechLocale::De);
        precise.number_notation = notation;
        let mut natural = precise.clone();
        natural.reading_style = ReadingStyle::Natural;
        for (source, short, explicit) in [
            ("08:00 Uhr", "acht uhr", "acht uhr null minuten"),
            ("00:00 Uhr", "null uhr", "null uhr null minuten"),
            ("01:00 Uhr", "ein uhr", "ein uhr null minuten"),
            ("20:00 Uhr", "zwanzig uhr", "zwanzig uhr null minuten"),
        ] {
            check(source, &precise, short, rule);
            check(source, &natural, explicit, rule);
        }
        check(" \t08:00 Uhr\n", &precise, "acht uhr", rule);
        check("20:00 Uhr", &precise, "Zwanzig uhr", rule);
    }
    let precise = SpeechOptions::new(SpeechLocale::De);
    let mut natural = precise.clone();
    natural.reading_style = ReadingStyle::Natural;
    let a = parse_for_speech("08:00 Uhr", &precise).unwrap();
    let b = parse_for_speech("08:00 Uhr", &natural).unwrap();
    assert!(matches!(
        a.spans[0].value,
        SpeechValue::Time {
            hour: 8,
            minute: 0,
            second: None
        }
    ));
    assert!(matches!(
        b.spans[0].value,
        SpeechValue::Temporal {
            value: textintel::speech::temporal::TemporalValue::Clock {
                hour: 8,
                minute: Some(0),
                second: None,
                ..
            }
        }
    ));
    assert_ne!(a, b);
}

#[test]
fn german_twenty_four_hour_endpoint_requires_identical_ir_and_never_becomes_midnight_zero() {
    use textintel::speech::temporal::TemporalValue;
    let precise = SpeechOptions::new(SpeechLocale::De);
    let mut natural = precise.clone();
    natural.reading_style = ReadingStyle::Natural;
    let a = parse_for_speech("24:00 Uhr", &precise).unwrap();
    let b = parse_for_speech("24:00 Uhr", &natural).unwrap();
    assert_eq!(a, b);
    assert!(matches!(
        a.spans[0].value,
        SpeechValue::Temporal {
            value: TemporalValue::Clock {
                hour: 24,
                minute: Some(0),
                second: None,
                ..
            }
        }
    ));
    check(
        "24:00 Uhr",
        &precise,
        "vierundzwanzig uhr",
        "alternative_german_clock_zero_minute_policy",
    );
    check(
        "24:00 Uhr",
        &precise,
        "vier und zwanzig uhr",
        "alternative_german_clock_zero_minute_policy",
    );
    check(
        "24:00 Uhr",
        &natural,
        "vierundzwanzig uhr null minuten",
        "alternative_german_clock_zero_minute_policy",
    );
    reject("24:00 Uhr", &precise, "null uhr");
    reject("24:00 Uhr", &natural, "null uhr null minuten");
    reject("24:00", &natural, "vierundzwanzig uhr null minuten");
    let mut forged = a.clone();
    if let SpeechValue::Temporal {
        value: TemporalValue::Clock { hour, .. },
    } = &mut forged.spans[0].value
    {
        *hour = 0;
    }
    let actual = normalize_for_speech("24:00 Uhr", &precise).unwrap();
    assert!(compare_spoken(&forged, &precise, &actual.text, "null uhr").is_none());
}

#[test]
fn german_zero_minute_policy_excludes_changed_values_precision_qualifiers_and_prose() {
    let opts = SpeechOptions::new(SpeechLocale::De);
    for (source, reference) in [
        ("08:00 Uhr", "neun uhr"),
        ("08:00 Uhr", "acht uhr eine minute"),
        ("08:01 Uhr", "acht uhr"),
        ("08:00:00 Uhr", "acht uhr null sekunden"),
        ("08:00:01 Uhr", "acht uhr eine sekunde"),
        ("08:00:00.0 Uhr", "acht uhr"),
        ("08:00 Uhr PM", "zwanzig uhr"),
        ("08:00am", "acht uhr"),
        ("08:00 Uhr UTC", "acht uhr UTC"),
        ("Termin 08:00 Uhr", "Termin acht uhr"),
        ("08:00 Uhr.", "acht uhr."),
        ("8 h", "acht uhr"),
        ("8h00min", "acht uhr"),
        ("8", "acht uhr"),
        ("2024-08-01", "acht uhr"),
    ] {
        reject(source, &opts, reference);
    }
    let mut natural = opts.clone();
    natural.reading_style = ReadingStyle::Natural;
    reject("8h00min", &natural, "acht uhr null minuten");
    reject("08:00 Uhr PM", &natural, "zwanzig uhr null minuten");
    let mut digits = opts.clone();
    digits.reading_style = ReadingStyle::Digits;
    reject("08:00 Uhr", &digits, "acht uhr null minuten");
    let mut number = opts.clone();
    number.interpret_as = textintel::speech::SpeechInterpretation::Number;
    reject("08:00 Uhr", &number, "acht uhr");
    let mut document = parse_for_speech("08:00 Uhr", &opts).unwrap();
    let actual = normalize_for_speech("08:00 Uhr", &opts).unwrap();
    document.spans[0].value = SpeechValue::Time {
        hour: 8,
        minute: 0,
        second: Some(0),
    };
    assert!(compare_spoken(&document, &opts, &actual.text, "acht uhr").is_none());
    let mut document = parse_for_speech("08:00 Uhr", &opts).unwrap();
    document.spans[0].source = "09:00 Uhr".into();
    assert!(compare_spoken(&document, &opts, &actual.text, "acht uhr").is_none());
}
