use textintel::speech::temporal::{self, TemporalValue};
use textintel::speech::{
    DateOrder, NumberNotation, ReadingStyle, SpeechInterpretation, SpeechLocale, SpeechOptions,
};

fn options(locale: SpeechLocale) -> SpeechOptions {
    let mut opts = SpeechOptions::new(locale);
    opts.reading_style = ReadingStyle::Natural;
    opts.date_order = DateOrder::DayMonthYear;
    opts.interpret_as = SpeechInterpretation::Date;
    opts.number_notation = NumberNotation::Orthographic;
    opts
}

fn render(source: &str, opts: &SpeechOptions) -> String {
    let (end, value) =
        temporal::parse_at(source, 0, opts).unwrap_or_else(|| panic!("unparsed {source:?}"));
    assert_eq!(
        end,
        source.len(),
        "partial temporal expression {source:?}: {value:?}"
    );
    temporal::render(&value, opts.locale, opts)
        .unwrap_or_else(|| panic!("unsupported {source:?}: {value:?}"))
}

#[test]
fn named_and_partial_dates_are_real_calendar_values() {
    for (locale, source, expected) in [
        (SpeechLocale::Es, "1 enero", "primero de enero"),
        (
            SpeechLocale::Es,
            "30 julio del 2020",
            "treinta de julio del dos mil veinte",
        ),
        (SpeechLocale::Es, "abril 30", "treinta de abril"),
        (
            SpeechLocale::En,
            "jul 25 2012",
            "july twenty fifth twenty twelve",
        ),
        (
            SpeechLocale::En,
            "22nd july 2012",
            "the twenty second of july twenty twelve",
        ),
        (SpeechLocale::Fr, "17 janvier", "dix-sept janvier"),
        (SpeechLocale::Fr, "17.06", "dix-sept juin"),
        (
            SpeechLocale::Pt,
            "01/01/2000",
            "primeiro de janeiro de dois mil",
        ),
        (SpeechLocale::Hi, "१२-०७", "बारह जुलाई"),
    ] {
        assert_eq!(render(source, &options(locale)), expected, "{source}");
    }
}

#[test]
fn month_year_partial_dates_render_in_all_twenty_languages() {
    let cases = [
        (SpeechLocale::En, "May 2024", "may twenty twenty four"),
        (
            SpeechLocale::Es,
            "mayo de 2024",
            "mayo de dos mil veinticuatro",
        ),
        (SpeechLocale::Zh, "2024年5月", "二零二四年五月"),
        (SpeechLocale::Hi, "मई 2024", "मई दो हज़ार चौबीस"),
        (SpeechLocale::Ar, "مايو 2024", "مايو ألفان وأربعة وعشرون"),
        (SpeechLocale::Fr, "mai 2024", "mai deux mille vingt-quatre"),
        (
            SpeechLocale::Pt,
            "maio de 2024",
            "maio de dois mil e vinte e quatro",
        ),
        (
            SpeechLocale::Ru,
            "май 2024",
            "май две тысячи двадцать четвёртого года",
        ),
        (SpeechLocale::Bn, "মে 2024", "মে দুই হাজার চব্বিশ"),
        (SpeechLocale::Ur, "مئی 2024", "مئی دو ہزار چوبیس"),
        (SpeechLocale::Id, "Mei 2024", "Mei dua ribu dua puluh empat"),
        (
            SpeechLocale::De,
            "Mai 2024",
            "Mai zweitausendvierundzwanzig",
        ),
        (SpeechLocale::Ja, "2024年5月", "二千二十四年五月"),
        (SpeechLocale::Pa, "ਮਈ 2024", "ਮਈ ਦੋ ਹਜ਼ਾਰ ਚੌਵੀ"),
        (SpeechLocale::Mr, "मे 2024", "मे दोन हजार चोवीस"),
        (SpeechLocale::Te, "మే 2024", "మే రెండు వేల ఇరవై నాలుగు"),
        (SpeechLocale::Tr, "Mayıs 2024", "Mayıs iki bin yirmi dört"),
        (SpeechLocale::Ta, "மே 2024", "மே இரண்டாயிரத்து இருபத்து நான்கு"),
        (
            SpeechLocale::Vi,
            "tháng 5 2024",
            "tháng năm năm hai nghìn hai mươi tư",
        ),
        (SpeechLocale::Ko, "2024년 5월", "이천이십사년 오월"),
    ];
    assert_eq!(cases.len(), SpeechLocale::ALL.len());
    for (index, (locale, ..)) in cases.iter().enumerate() {
        assert_eq!(*locale, SpeechLocale::ALL[index]);
    }
    for (locale, source, expected) in cases {
        assert_eq!(render(source, &options(locale)), expected, "{source}");
    }
}

#[test]
fn public_month_year_values_render_for_every_language_style_and_notation() {
    for locale in SpeechLocale::ALL {
        for reading_style in [
            ReadingStyle::Precise,
            ReadingStyle::Natural,
            ReadingStyle::Digits,
        ] {
            for number_notation in [NumberNotation::Phonetic, NumberNotation::Orthographic] {
                let mut opts = SpeechOptions::new(locale);
                opts.reading_style = reading_style;
                opts.number_notation = number_notation;
                for month in 1..=12 {
                    let value = TemporalValue::Date {
                        year: Some("2024".into()),
                        month: Some(month),
                        day: None,
                        presentation: temporal::DatePresentation::MonthFirst,
                        connector: None,
                        year_label: false,
                    };
                    assert!(
                        temporal::render(&value, locale, &opts)
                            .is_some_and(|spoken| !spoken.is_empty()),
                        "{locale:?} {reading_style:?} {number_notation:?} month {month}"
                    );
                }
            }
        }
    }
}

#[test]
fn month_year_dates_have_no_day_and_shorter_numbers_remain_days() {
    for (locale, source) in [
        (SpeechLocale::En, "May 2024"),
        (SpeechLocale::Es, "mayo de 2024"),
        (SpeechLocale::Ru, "май 2024"),
        (SpeechLocale::Ja, "2024年5月"),
    ] {
        let (end, value) = temporal::parse_at(source, 0, &options(locale)).unwrap();
        assert_eq!(end, source.len(), "{source}");
        assert!(
            matches!(
                value,
                TemporalValue::Date {
                    year: Some(ref year),
                    month: Some(5),
                    day: None,
                    ..
                } if year == "2024"
            ),
            "{source}: {value:?}"
        );
    }
    for (locale, source) in [(SpeechLocale::En, "May 24"), (SpeechLocale::Ar, "مايو ٢٤")] {
        let (_, value) = temporal::parse_at(source, 0, &options(locale)).unwrap();
        assert!(
            matches!(
                value,
                TemporalValue::Date {
                    year: None,
                    month: Some(5),
                    day: Some(24),
                    ..
                }
            ),
            "{source}: {value:?}"
        );
    }
}

#[test]
fn trailing_numeric_month_year_uses_width_not_date_order_guesses() {
    for (locale, source, expected) in [
        (SpeechLocale::En, "5/2024", "may twenty twenty four"),
        (SpeechLocale::De, "05.2024", "Mai zweitausendvierundzwanzig"),
        (
            SpeechLocale::Ru,
            "05-2024",
            "май две тысячи двадцать четвёртого года",
        ),
        (SpeechLocale::Ar, "٠٥/٢٠٢٤", "مايو ألفان وأربعة وعشرون"),
    ] {
        for date_order in [DateOrder::DayMonthYear, DateOrder::YearMonthDay] {
            let mut opts = options(locale);
            opts.date_order = date_order;
            assert_eq!(render(source, &opts), expected, "{source} {date_order:?}");
            let (_, value) = temporal::parse_at(source, 0, &opts).unwrap();
            assert!(
                matches!(
                    value,
                    TemporalValue::Date {
                        year: Some(ref year),
                        month: Some(5),
                        day: None,
                        ..
                    } if year == "2024"
                ),
                "{source}: {value:?}"
            );
        }
    }
    let mut short_opts = options(SpeechLocale::En);
    short_opts.date_order = DateOrder::MonthDayYear;
    let (_, short) = temporal::parse_at("05/24", 0, &short_opts).unwrap();
    assert!(
        matches!(
            short,
            TemporalValue::Date {
                year: None,
                month: Some(5),
                day: Some(24),
                ..
            }
        ),
        "{short:?}"
    );
    let (_, invalid) = temporal::parse_at("13/2024", 0, &options(SpeechLocale::En)).unwrap();
    assert!(
        matches!(invalid, TemporalValue::Invalid { .. }),
        "{invalid:?}"
    );
}

#[test]
fn grammatical_month_year_connectors_and_full_year_labels_are_atomic() {
    let en = options(SpeechLocale::En);
    assert_eq!(render("May of 2024", &en), "may of twenty twenty four");
    assert_eq!(render("May, 2024", &en), "may twenty twenty four");
    assert_eq!(render("MAY OF 2024", &en), "may of twenty twenty four");
    let (_, value) = temporal::parse_at("May of 2024", 0, &en).unwrap();
    assert!(
        matches!(
            value,
            TemporalValue::Date {
                connector: Some(ref connector),
                day: None,
                ..
            } if connector == "of"
        ),
        "{value:?}"
    );

    let ru = options(SpeechLocale::Ru);
    assert_eq!(
        render("май 2024 года", &ru),
        "май две тысячи двадцать четвёртого года"
    );
    assert_eq!(
        render("МАЙ 2024 ГОДА", &ru),
        "май две тысячи двадцать четвёртого года"
    );
    assert_eq!(
        render("в мае 2024 года", &ru),
        "в мае две тысячи двадцать четвёртого года"
    );
    assert_eq!(
        render("В МАЕ 2024 ГОДА", &ru),
        "в мае две тысячи двадцать четвёртого года"
    );
    let (end, prepositional) = temporal::parse_at("в мае 2024 года", 0, &ru).unwrap();
    assert_eq!(end, "в мае 2024 года".len());
    assert!(
        matches!(
            prepositional,
            TemporalValue::Date {
                connector: Some(ref connector),
                year_label: true,
                month: Some(5),
                day: None,
                ..
            } if connector == "в"
        ),
        "{prepositional:?}"
    );
    assert_eq!(
        render("MAYO DE 2024", &options(SpeechLocale::Es)),
        "mayo de dos mil veinticuatro"
    );
    let (end, value) = temporal::parse_at("май 2024 года", 0, &ru).unwrap();
    assert_eq!(end, "май 2024 года".len());
    assert!(
        matches!(
            value,
            TemporalValue::Date {
                year_label: true,
                day: None,
                ..
            }
        ),
        "{value:?}"
    );
}

#[test]
fn russian_month_year_uses_every_nominative_month_and_cjk_irregulars() {
    let ru = options(SpeechLocale::Ru);
    for (month, (name, prepositional)) in [
        ("январь", "январе"),
        ("февраль", "феврале"),
        ("март", "марте"),
        ("апрель", "апреле"),
        ("май", "мае"),
        ("июнь", "июне"),
        ("июль", "июле"),
        ("август", "августе"),
        ("сентябрь", "сентябре"),
        ("октябрь", "октябре"),
        ("ноябрь", "ноябре"),
        ("декабрь", "декабре"),
    ]
    .into_iter()
    .enumerate()
    {
        let value = TemporalValue::Date {
            year: Some("2024".into()),
            month: Some(month as u32 + 1),
            day: None,
            presentation: temporal::DatePresentation::MonthFirst,
            connector: None,
            year_label: false,
        };
        assert_eq!(
            temporal::render(&value, SpeechLocale::Ru, &ru).unwrap(),
            format!("{name} две тысячи двадцать четвёртого года")
        );
        let value = TemporalValue::Date {
            year: Some("2024".into()),
            month: Some(month as u32 + 1),
            day: None,
            presentation: temporal::DatePresentation::MonthFirst,
            connector: Some("в".into()),
            year_label: false,
        };
        assert_eq!(
            temporal::render(&value, SpeechLocale::Ru, &ru).unwrap(),
            format!("в {prepositional} две тысячи двадцать четвёртого года")
        );
    }
    let ko = options(SpeechLocale::Ko);
    assert_eq!(render("2024년 6월", &ko), "이천이십사년 유월");
    assert_eq!(render("2024년 10월", &ko), "이천이십사년 시월");
    let mut ja = options(SpeechLocale::Ja);
    ja.number_notation = NumberNotation::Phonetic;
    assert_eq!(render("2024年6月", &ja), "にせんにじゅうよねんろくがつ");
}

#[test]
fn month_year_year_width_counts_unicode_digits_not_utf8_bytes() {
    for (locale, source, expected) in [
        (SpeechLocale::Ar, "مايو ٢٠٢٤", "مايو ألفان وأربعة وعشرون"),
        (SpeechLocale::Hi, "मई २०२४", "मई दो हज़ार चौबीस"),
        (SpeechLocale::Bn, "মে ২০২৪", "মে দুই হাজার চব্বিশ"),
        (SpeechLocale::En, "May ２０２４", "may twenty twenty four"),
        (SpeechLocale::Ur, "مئی ۲۰۲۴", "مئی دو ہزار چوبیس"),
        (SpeechLocale::Pa, "ਮਈ ੨੦੨੪", "ਮਈ ਦੋ ਹਜ਼ਾਰ ਚੌਵੀ"),
        (SpeechLocale::Te, "మే ౨౦౨౪", "మే రెండు వేల ఇరవై నాలుగు"),
        (SpeechLocale::Ta, "மே ௨௦௨௪", "மே இரண்டாயிரத்து இருபத்து நான்கு"),
    ] {
        assert_eq!(render(source, &options(locale)), expected, "{source}");
    }
}

#[test]
fn calendar_components_preserve_written_year_width_and_native_script() {
    for (locale, source, expected) in [
        (SpeechLocale::Zh, "05年", "零五年"),
        (SpeechLocale::Zh, "2015年08月08号", "二零一五年八月八日"),
        (SpeechLocale::Ja, "2024年", "二千二十四年"),
        (SpeechLocale::Ja, "1月22日", "一月二十二日"),
        (SpeechLocale::Ja, "R.4", "令和四年"),
        (SpeechLocale::Ko, "2024년", "이천이십사년"),
        (SpeechLocale::Ko, "3월", "삼월"),
        (SpeechLocale::Ko, "기원전128년", "기원전 백이십팔년"),
    ] {
        assert_eq!(render(source, &options(locale)), expected, "{source}");
    }
}

#[test]
fn calendar_ranges_and_weekdays_use_compositional_grammar() {
    let opts = options(SpeechLocale::Ja);
    assert_eq!(
        render("1月1日(月)〜3日(水)", &opts),
        "一月一日月曜日から三日水曜日"
    );
    assert_eq!(render("2月〜11月", &opts), "二月から十一月");
    assert_eq!(
        render("2024年01月01日(月)", &opts),
        "二千二十四年一月一日月曜日"
    );
    let (_, wrong) = temporal::parse_at("2024年01月01日(火)", 0, &opts).unwrap();
    assert!(matches!(wrong, TemporalValue::Invalid { .. }));
}

#[test]
fn all_twenty_languages_share_the_same_complete_temporal_contract() {
    for locale in SpeechLocale::ALL {
        let defaults = SpeechOptions::new(locale);
        assert!(
            temporal::render_date(2024, 12, 31, locale, &defaults).is_some(),
            "default date {locale:?}"
        );
        assert!(
            temporal::render_time(15, 30, None, locale, &defaults).is_some(),
            "default clock {locale:?}"
        );
        let opts = options(locale);
        let (end, value) = temporal::parse_at("31-12-2024", 0, &opts).unwrap();
        assert_eq!(end, 10, "{locale:?}");
        assert_eq!(temporal::kind(&value), "date");
        assert!(
            !temporal::render(&value, locale, &opts).unwrap().is_empty(),
            "{locale:?}"
        );
        let mut clock = opts;
        clock.interpret_as = SpeechInterpretation::Time;
        let source = if locale == SpeechLocale::De {
            "13:42 Uhr"
        } else {
            "13:42"
        };
        let (_, value) = temporal::parse_at(source, 0, &clock).unwrap();
        assert_eq!(temporal::kind(&value), "time");
        assert!(
            !temporal::render(&value, locale, &clock).unwrap().is_empty(),
            "{locale:?}"
        );
    }
}

#[test]
fn ambiguous_shapes_require_explicit_semantic_intent() {
    let mut auto = options(SpeechLocale::Es);
    auto.interpret_as = SpeechInterpretation::Auto;
    assert!(temporal::parse_at("1.00", 0, &auto).is_none());
    assert!(temporal::parse_at("1h", 0, &auto).is_none());
    let mut number = auto;
    number.interpret_as = SpeechInterpretation::Number;
    assert!(temporal::parse_at("1.00", 0, &number).is_none());
}

#[test]
fn identifier_continuations_do_not_yield_temporal_prefixes() {
    let opts = options(SpeechLocale::En);
    for source in [
        "1.2.3-beta.4",
        "2024-01-31-code",
        "17/06/2024-ID",
        "12:30:45:67",
        "2024/12/31/5",
    ] {
        assert!(temporal::parse_at(source, 0, &opts).is_none(), "{source}");
    }
}

#[test]
fn localized_clocks_retain_written_period_and_omit_unwritten_parts() {
    for (locale, source, expected) in [
        (SpeechLocale::En, "1.59 p.m.", "one fifty nine PM"),
        (SpeechLocale::En, "1 a.m.", "one AM"),
        (SpeechLocale::En, "23:00", "twenty three hundred"),
        (SpeechLocale::Es, "1.00 a.m.", "una de la mañana"),
        (SpeechLocale::Es, "21:00", "veintiuna"),
        (SpeechLocale::Es, "22:05", "veintidós y cinco"),
        (SpeechLocale::Es, "01.05 h", "una y cinco"),
        (SpeechLocale::De, "24:00 Uhr", "vierundzwanzig uhr"),
        (SpeechLocale::Pt, "14h30", "catorze horas e trinta"),
        (SpeechLocale::Hi, "५:१२:०१", "पाँच बजकर बारह मिनट एक सेकंड"),
        (SpeechLocale::Ja, "3時18分", "三時十八分"),
        (SpeechLocale::Ko, "오전10시30분", "오전 열시 삼십분"),
        (SpeechLocale::Ko, "오후 3시 반", "오후 세시 반"),
        (SpeechLocale::Zh, "03:04:05am", "三点四分五秒am"),
        (SpeechLocale::Vi, "9h", "chín giờ"),
    ] {
        let mut opts = options(locale);
        opts.interpret_as = SpeechInterpretation::Time;
        assert_eq!(render(source, &opts), expected, "{source}");
    }
}

#[test]
fn invalid_temporal_constructs_are_preserved_whole() {
    for (locale, source) in [
        (SpeechLocale::Es, "30 febrero 2023"),
        (SpeechLocale::Hi, "३१-०६"),
        (SpeechLocale::Fr, "31.04.2024"),
        (SpeechLocale::Zh, "2023年02月29日"),
        (SpeechLocale::Ja, "13月"),
        (SpeechLocale::En, "25:01"),
        (SpeechLocale::De, "24:01 Uhr"),
    ] {
        let mut opts = options(locale);
        if source.contains(':') {
            opts.interpret_as = SpeechInterpretation::Time;
        }
        let (end, value) = temporal::parse_at(source, 0, &opts).unwrap();
        assert_eq!(end, source.len(), "{source}");
        assert!(
            matches!(value, TemporalValue::Invalid { .. }),
            "{source}: {value:?}"
        );
        assert!(temporal::render(&value, locale, &opts).is_none());
    }
}

#[test]
fn legacy_typed_wrappers_reject_invalid_values_before_style_fallback() {
    for locale in SpeechLocale::ALL {
        for style in [
            ReadingStyle::Precise,
            ReadingStyle::Natural,
            ReadingStyle::Digits,
        ] {
            let mut opts = options(locale);
            opts.reading_style = style;
            for (year, m, d) in [
                (0, 1, 1),
                (2023, 2, 29),
                (2024, 4, 31),
                (10000, 1, 1),
                (2024, 13, 1),
                (2024, 1, 0),
            ] {
                assert!(
                    temporal::render_date(year, m, d, locale, &opts).is_none(),
                    "{locale:?} {style:?} {year}-{m}-{d}"
                );
            }
            for (hour, minute, second) in [(24, 0, None), (9, 60, None), (9, 15, Some(60))] {
                assert!(
                    temporal::render_time(hour, minute, second, locale, &opts).is_none(),
                    "{locale:?} {style:?} {hour}:{minute}:{second:?}"
                );
            }
        }
    }
}

#[test]
fn day_lists_and_calendar_year_context_compose_beyond_reference_examples() {
    assert_eq!(
        render("80s", &options(SpeechLocale::Fr)),
        "années quatre-vingt"
    );
    assert_eq!(render("90s", &options(SpeechLocale::En)), "nineties");
    assert_eq!(
        render("18/19 septembre", &options(SpeechLocale::Fr)),
        "dix-huit dix-neuf septembre"
    );
    assert_eq!(
        render("6/3/1006г.", &options(SpeechLocale::Ru)),
        "шестое марта тысяча шестого года"
    );
    assert_eq!(
        render("ngày 21/04/2031", &options(SpeechLocale::Vi)),
        "ngày hai mươi mốt tháng bốn năm hai nghìn ba mươi mốt"
    );
    let (_, bad) = temporal::parse_at("30/31 février", 0, &options(SpeechLocale::Fr)).unwrap();
    assert!(matches!(bad, TemporalValue::Invalid { .. }));
}

#[test]
fn unknown_order_and_incorrect_english_suffixes_are_not_guessed() {
    let mut opts = options(SpeechLocale::Fr);
    opts.date_order = DateOrder::RejectAmbiguous;
    let (_, value) = temporal::parse_at("02.03.2003", 0, &opts).unwrap();
    assert!(matches!(value, TemporalValue::Invalid { .. }));
    assert!(temporal::parse_at("25st july 2012", 0, &options(SpeechLocale::En)).is_none());
}

#[test]
fn precise_existing_numeric_forms_keep_the_original_parser_contract() {
    let opts = SpeechOptions::new(SpeechLocale::En);
    assert!(temporal::parse_at("2024-01-31", 0, &opts).is_none());
    assert!(temporal::parse_at("13:30", 0, &opts).is_none());
}

#[test]
fn date_changes_inside_unicode_text_keep_original_byte_ranges() {
    let text = "📅 rendez-vous 17 janvier après-midi";
    let start = text.find("17 janvier").unwrap();
    let (end, value) = temporal::parse_at(text, start, &options(SpeechLocale::Fr)).unwrap();
    assert_eq!(&text[start..end], "17 janvier");
    assert_eq!(
        temporal::render(&value, SpeechLocale::Fr, &options(SpeechLocale::Fr)).unwrap(),
        "dix-sept janvier"
    );
}

#[test]
fn meridiem_clocks_validate_twelve_hour_semantics_in_every_locale() {
    // Explicit AM/PM has the same 1..12 semantics in every locale. Bare 24h
    // clocks stay valid; no reference example enables a locale exception.
    for locale in SpeechLocale::ALL {
        for style in [ReadingStyle::Precise, ReadingStyle::Natural] {
            let mut opts = SpeechOptions::new(locale);
            opts.reading_style = style;
            opts.interpret_as = SpeechInterpretation::Time;
            // Precise phonetic clocks intentionally dispatch through the old
            // Time IR; orthographic enables the structural hook directly.
            let mut structural = opts.clone();
            structural.number_notation = NumberNotation::Orthographic;
            for source in ["23:30 AM", "00:30 PM", "13:10pm", "24:00 AM"] {
                let wrapped = format!("📚 {source}!");
                let start = "📚 ".len();
                let (end, value) = temporal::parse_at(&wrapped, start, &structural).unwrap();
                assert_eq!(&wrapped[start..end], source, "{locale:?} {style:?}");
                assert!(
                    matches!(value, TemporalValue::Invalid { .. }),
                    "{locale:?}: {value:?}"
                );
                assert!(temporal::render(&value, locale, &opts).is_none());
                let result = textintel::speech::normalize_for_speech(&wrapped, &opts).unwrap();
                assert_eq!(result.text, wrapped, "{locale:?} {style:?}");
                assert!(!result.fully_supported, "{locale:?} {style:?} {source}");
            }
            for source in ["12:30 AM", "12:30 PM", "01:30am", "11:30 p.m."] {
                let (end, value) = temporal::parse_at(source, 0, &structural).unwrap();
                assert_eq!(end, source.len());
                assert!(
                    matches!(value, TemporalValue::Clock { .. }),
                    "{locale:?}: {value:?}"
                );
                assert!(
                    temporal::render(&value, locale, &opts).is_some(),
                    "{locale:?} {source}"
                );
            }
            // Publicly constructed semantic values cannot bypass the parser.
            for (hour, marker) in [(23, "am"), (0, "pm"), (13, "pm")] {
                let value = TemporalValue::Clock {
                    hour,
                    minute: Some(30),
                    second: None,
                    period: None,
                    meridiem: Some(marker.into()),
                    half: false,
                };
                assert!(temporal::render(&value, locale, &opts).is_none());
            }
            assert!(temporal::render_time(23, 30, None, locale, &opts).is_some());
            assert!(temporal::render_time(0, 30, None, locale, &opts).is_some());
        }
    }
}

#[test]
fn era_years_are_positive_ordinals_in_both_source_and_public_values() {
    for (locale, era, suffix) in [
        (SpeechLocale::Zh, "公元", "年"),
        (SpeechLocale::Zh, "公元前", "年"),
        (SpeechLocale::Ko, "서기", "년"),
        (SpeechLocale::Ko, "기원전", "년"),
        (SpeechLocale::Ja, "令和", "年"),
    ] {
        let opts = options(locale);
        for year in ["0", "00", "٠", "००", "００"] {
            let source = format!("{era}{year}{suffix}");
            let wrapped = format!("📚 {source}!");
            let start = "📚 ".len();
            let (end, value) = temporal::parse_at(&wrapped, start, &opts)
                .unwrap_or_else(|| panic!("unparsed {source}"));
            assert_eq!(&wrapped[start..end], source);
            assert!(
                matches!(value, TemporalValue::Invalid { .. }),
                "{source}: {value:?}"
            );
            assert!(temporal::render(&value, locale, &opts).is_none());
            let forged = TemporalValue::EraYear {
                era: era.into(),
                year: year.into(),
            };
            assert!(
                temporal::render(&forged, locale, &opts).is_none(),
                "{source}"
            );
            let result = textintel::speech::normalize_for_speech(&wrapped, &opts).unwrap();
            assert_eq!(result.text, wrapped);
            assert!(!result.fully_supported);
        }
        for year in ["1", "01", "١", "१", "１"] {
            let source = format!("{era}{year}{suffix}");
            let (end, value) = temporal::parse_at(&source, 0, &opts).unwrap();
            assert_eq!(end, source.len());
            assert!(matches!(value, TemporalValue::EraYear { .. }));
            assert!(
                temporal::render(&value, locale, &opts).is_some(),
                "{source}"
            );
            let public = TemporalValue::EraYear {
                era: era.into(),
                year: year.into(),
            };
            assert!(
                temporal::render(&public, locale, &opts).is_some(),
                "{source}"
            );
        }
    }
}

#[test]
fn redundant_clock_period_is_opt_in_and_decodes_to_canonical_values_in_all_twenty_locales() {
    use textintel::speech::ClockPolicy;
    for locale in SpeechLocale::ALL {
        let mut strict = SpeechOptions::new(locale);
        strict.reading_style = ReadingStyle::Natural;
        strict.interpret_as = SpeechInterpretation::Time;
        assert_eq!(strict.clock_policy, ClockPolicy::Strict);
        let mut tolerant = strict.clone();
        tolerant.clock_policy = ClockPolicy::NormalizeRedundantPeriod;
        for (source, canonical, hour, minute, second, meridiem) in [
            ("13:10pm", "1:10pm", 1, 10, None, "pm"),
            ("23:59:58 PM", "11:59:58 PM", 11, 59, Some(58), "pm"),
            ("00:30am", "12:30am", 12, 30, None, "am"),
            ("0:00:00 AM", "12:00:00 AM", 12, 0, Some(0), "am"),
            ("१३:१० PM", "1:10pm", 1, 10, None, "pm"),
        ] {
            let wrapped = format!("📚 {source}!");
            let start = "📚 ".len();
            let (end, value) = temporal::parse_at(&wrapped, start, &tolerant).unwrap();
            assert_eq!(&wrapped[start..end], source, "{locale:?}");
            assert_eq!(
                value,
                TemporalValue::Clock {
                    hour,
                    minute: Some(minute),
                    second,
                    period: None,
                    meridiem: Some(meridiem.into()),
                    half: false,
                },
                "{locale:?} {source}"
            );
            let (_, canonical_value) = temporal::parse_at(canonical, 0, &strict).unwrap();
            assert_eq!(value, canonical_value);
            assert_eq!(
                temporal::render(&value, locale, &tolerant),
                temporal::render(&canonical_value, locale, &strict)
            );
            let (_, invalid) = temporal::parse_at(source, 0, &strict).unwrap();
            assert!(
                matches!(invalid, TemporalValue::Invalid { .. }),
                "{locale:?} {source}"
            );
            let document = textintel::speech::parse_for_speech(&wrapped, &tolerant).unwrap();
            let clock = document
                .spans
                .iter()
                .find(|span| span.source == source)
                .unwrap();
            assert_eq!((clock.start, clock.end), (start, end));
            for style in [ReadingStyle::Precise, ReadingStyle::Natural] {
                let mut options = tolerant.clone();
                options.reading_style = style;
                let actual = textintel::speech::normalize_for_speech(source, &options).unwrap();
                let expected =
                    textintel::speech::normalize_for_speech(canonical, &options).unwrap();
                assert!(actual.fully_supported, "{locale:?} {style:?} {source}");
                assert_eq!(actual.text, expected.text, "{locale:?} {style:?}");
            }
        }
        for source in ["1:00 AM", "12:00 AM", "1:00 PM", "12:00 PM"] {
            assert_eq!(
                temporal::parse_at(source, 0, &strict),
                temporal::parse_at(source, 0, &tolerant)
            );
        }
        for source in [
            "13:10am",
            "00:30pm",
            "24:00pm",
            "24:00am",
            "25:00pm",
            "13:60pm",
            "13:10:60pm",
        ] {
            let (end, value) = temporal::parse_at(source, 0, &tolerant).unwrap();
            assert_eq!(end, source.len());
            assert!(
                matches!(value, TemporalValue::Invalid { .. }),
                "{locale:?}: {value:?}"
            );
            let result = textintel::speech::normalize_for_speech(source, &tolerant).unwrap();
            assert_eq!(result.text, source);
            assert!(!result.fully_supported);
        }
        for (hour, marker) in [
            (13, "pm"),
            (23, "pm"),
            (0, "am"),
            (13, "am"),
            (0, "pm"),
            (24, "pm"),
        ] {
            let forged = TemporalValue::Clock {
                hour,
                minute: Some(10),
                second: None,
                period: None,
                meridiem: Some(marker.into()),
                half: false,
            };
            assert!(
                temporal::render(&forged, locale, &tolerant).is_none(),
                "{locale:?} {forged:?}"
            );
        }
    }
}

#[test]
fn tolerant_clock_readings_use_existing_language_rendering_and_reject_contradictory_labels() {
    use textintel::speech::ClockPolicy;
    for (locale, expected) in [
        (SpeechLocale::En, "one ten PM"),
        (SpeechLocale::Es, "una y diez de la tarde"),
        (SpeechLocale::Zh, "一点十分pm"),
    ] {
        let mut opts = SpeechOptions::new(locale);
        opts.reading_style = ReadingStyle::Natural;
        opts.clock_policy = ClockPolicy::NormalizeRedundantPeriod;
        opts.interpret_as = SpeechInterpretation::Time;
        assert_eq!(render("13:10pm", &opts), expected);
    }
    let mut opts = options(SpeechLocale::Ko);
    opts.interpret_as = SpeechInterpretation::Time;
    opts.clock_policy = ClockPolicy::NormalizeRedundantPeriod;
    for source in ["오전13시pm", "오후0시am", "오전1시pm", "오후1시am"] {
        let (end, value) = temporal::parse_at(source, 0, &opts).unwrap();
        assert_eq!(end, source.len());
        assert!(matches!(value, TemporalValue::Invalid { .. }), "{source}");
    }
    let (_, value) = temporal::parse_at("오후13시pm", 0, &opts).unwrap();
    assert!(matches!(value, TemporalValue::Clock { hour: 1, .. }));
    assert!(temporal::render(&value, opts.locale, &opts).is_some());
    let forged = TemporalValue::Clock {
        hour: 1,
        minute: None,
        second: None,
        period: Some("오전".into()),
        meridiem: Some("pm".into()),
        half: false,
    };
    assert!(temporal::render(&forged, opts.locale, &opts).is_none());
}

#[test]
fn invalid_calendar_components_are_explicit_and_diagnostic_in_all_twenty_languages() {
    use textintel::speech::{
        InvalidDatePolicy, SpeechValue, normalize_for_speech, parse_for_speech,
    };
    for locale in SpeechLocale::ALL {
        let strict = options(locale);
        assert_eq!(strict.invalid_date_policy, InvalidDatePolicy::Preserve);
        let mut literal = strict.clone();
        literal.invalid_date_policy = InvalidDatePolicy::ReadComponents;
        for (source, year, month, day) in [
            ("30/02/2023", "2023", 2, 30),
            ("2023-02-30", "2023", 2, 30),
            ("31/06/2024", "2024", 6, 31),
            ("29/02/1900", "1900", 2, 29),
        ] {
            let wrapped = format!("📚 {source}!");
            let start = "📚 ".len();
            let (end, value) = temporal::parse_at(&wrapped, start, &literal).unwrap();
            assert_eq!(&wrapped[start..end], source, "{locale:?}");
            assert_eq!(
                value,
                TemporalValue::InvalidCalendarDate {
                    year: Some(year.into()),
                    month,
                    day,
                    presentation: temporal::DatePresentation::Numeric,
                    connector: None,
                    year_label: false,
                }
            );
            assert_eq!(temporal::kind(&value), "invalid_date");
            assert_eq!(temporal::diagnostic(&value), Some("invalid calendar date"));
            assert!(temporal::render(&value, locale, &strict).is_none());
            assert!(
                !temporal::render(&value, locale, &literal)
                    .unwrap()
                    .is_empty()
            );
            let (_, preserved) = temporal::parse_at(source, 0, &strict).unwrap();
            assert!(matches!(preserved, TemporalValue::Invalid { .. }));
            for (style, notation) in [
                (ReadingStyle::Precise, NumberNotation::Phonetic),
                (ReadingStyle::Precise, NumberNotation::Orthographic),
                (ReadingStyle::Natural, NumberNotation::Phonetic),
                (ReadingStyle::Natural, NumberNotation::Orthographic),
            ] {
                let mut opts = literal.clone();
                opts.reading_style = style;
                opts.number_notation = notation;
                let document = parse_for_speech(&wrapped, &opts).unwrap();
                let span = document
                    .spans
                    .iter()
                    .find(|span| span.source == source)
                    .unwrap();
                assert_eq!((span.start, span.end), (start, end));
                assert!(matches!(
                    &span.value,
                    SpeechValue::Temporal {
                        value: TemporalValue::InvalidCalendarDate { .. }
                    }
                ));
                let result = normalize_for_speech(&wrapped, &opts).unwrap();
                let reading = result.readings.iter().find(|r| r.source == source).unwrap();
                assert!(reading.supported, "{locale:?}: {reading:?}");
                assert_eq!(reading.reason.as_deref(), Some("invalid calendar date"));
            }
        }
        for source in ["29/02/2000", "2024-02-29"] {
            let a = temporal::parse_at(source, 0, &strict).unwrap();
            let b = temporal::parse_at(source, 0, &literal).unwrap();
            assert_eq!(a, b);
            assert_eq!(temporal::diagnostic(&a.1), None);
            assert_eq!(
                temporal::render(&a.1, locale, &strict),
                temporal::render(&b.1, locale, &literal)
            );
        }
        let forged_date = TemporalValue::Date {
            year: Some("2023".into()),
            month: Some(2),
            day: Some(30),
            presentation: temporal::DatePresentation::Numeric,
            connector: None,
            year_label: false,
        };
        assert!(temporal::render(&forged_date, locale, &literal).is_none());
        assert!(temporal::render_date(2023, 2, 30, locale, &literal).is_none());
        for (year, month, day) in [
            (Some("2024"), 2, 29),
            (None, 2, 29),
            (Some("0000"), 2, 30),
            (Some("00"), 2, 30),
            (Some("10000"), 2, 30),
            (Some("2023"), 13, 30),
            (Some("2023"), 2, 0),
        ] {
            let forged = TemporalValue::InvalidCalendarDate {
                year: year.map(str::to_owned),
                month,
                day,
                presentation: temporal::DatePresentation::Numeric,
                connector: None,
                year_label: false,
            };
            assert!(
                temporal::render(&forged, locale, &literal).is_none(),
                "{locale:?}: {forged:?}"
            );
        }
    }
}

#[test]
fn literal_invalid_dates_reuse_named_native_and_partial_grammar_without_century_guessing() {
    use textintel::speech::InvalidDatePolicy;
    for (locale, source, expected) in [
        (
            SpeechLocale::Es,
            "30/02/1990",
            "treinta de febrero de mil novecientos noventa",
        ),
        (
            SpeechLocale::Es,
            "30 febrero del 23",
            "treinta de febrero del veintitrés",
        ),
        (SpeechLocale::Hi, "३१-०६", "इकतीस जून"),
        (SpeechLocale::Fr, "30 février", "trente février"),
        (SpeechLocale::Zh, "2023年02月30日", "二零二三年二月三十日"),
        (SpeechLocale::Ja, "2023年2月30日", "二千二十三年二月三十日"),
    ] {
        let mut opts = options(locale);
        opts.invalid_date_policy = InvalidDatePolicy::ReadComponents;
        let (_, value) = temporal::parse_at(source, 0, &opts).unwrap();
        assert!(
            matches!(value, TemporalValue::InvalidCalendarDate { .. }),
            "{source}: {value:?}"
        );
        assert_eq!(render(source, &opts), expected);
    }
    let mut opts = options(SpeechLocale::En);
    opts.invalid_date_policy = InvalidDatePolicy::ReadComponents;
    opts.date_order = DateOrder::RejectAmbiguous;
    for source in ["30/02/2023", "02/30/2023"] {
        let (_, value) = temporal::parse_at(source, 0, &opts).unwrap();
        assert!(matches!(
            value,
            TemporalValue::InvalidCalendarDate {
                month: 2,
                day: 30,
                ..
            }
        ));
    }
    for source in [
        "02/03/2023",
        "30/13/2023",
        "00/02/2023",
        "30/02/0000",
        "30/02/00",
        "30/02/10000",
    ] {
        let (_, value) = temporal::parse_at(source, 0, &opts).unwrap();
        assert!(
            matches!(value, TemporalValue::Invalid { .. }),
            "{source}: {value:?}"
        );
        assert!(temporal::render(&value, opts.locale, &opts).is_none());
    }
    opts.interpret_as = SpeechInterpretation::Time;
    assert!(temporal::parse_at("30/02/2023", 0, &opts).is_none());
}

#[test]
fn literal_invalid_dates_remain_atomic_and_sequence_diagnostics_are_bounded() {
    use textintel::speech::{InvalidDatePolicy, normalize_for_speech};
    for (locale, source) in [
        (SpeechLocale::Fr, "30/31 février"),
        (SpeechLocale::Ja, "2023年02月30日〜3日"),
        (SpeechLocale::Ja, "2023年02月30日(木)"),
        (SpeechLocale::Ja, "2024年01月01日(火)"),
    ] {
        let mut opts = options(locale);
        opts.invalid_date_policy = InvalidDatePolicy::ReadComponents;
        let (end, value) = temporal::parse_at(source, 0, &opts).unwrap();
        assert_eq!(end, source.len());
        assert!(
            matches!(value, TemporalValue::Invalid { .. }),
            "{source}: {value:?}"
        );
        assert_eq!(normalize_for_speech(source, &opts).unwrap().text, source);
    }
    let mut value = TemporalValue::InvalidCalendarDate {
        year: Some("2023".into()),
        month: 2,
        day: 30,
        presentation: temporal::DatePresentation::Numeric,
        connector: None,
        year_label: false,
    };
    value = TemporalValue::CalendarSequence {
        items: vec![temporal::CalendarItem::Date {
            value: Box::new(value),
        }],
    };
    assert_eq!(temporal::diagnostic(&value), Some("invalid calendar date"));
    assert!(temporal::render(&value, SpeechLocale::Ja, &options(SpeechLocale::Ja)).is_none());
    for _ in 0..300 {
        value = TemporalValue::CalendarSequence {
            items: vec![temporal::CalendarItem::Date {
                value: Box::new(value),
            }],
        };
    }
    assert_eq!(
        temporal::diagnostic(&value),
        Some("calendar sequence exceeds diagnostic limit")
    );
}

#[test]
fn japanese_calendar_counters_share_precise_natural_and_literal_paths() {
    use textintel::speech::{InvalidDatePolicy, normalize_for_speech};
    // Year/hour/minute gold already exercised in speech_verbalize.rs:
    // 2024-04-24 => にせんにじゅうよねんしがつにじゅうよっか.
    // Date17/19/27/29 readings: Japan Foundation Marugoto Starter wordbook,
    // https://www.marugoto-online.jp/a1/support/pdf/MarugotoStarterWordbook_jpn.pdf
    for style in [ReadingStyle::Precise, ReadingStyle::Natural] {
        let mut opts = SpeechOptions::new(SpeechLocale::Ja);
        opts.reading_style = style;
        opts.interpret_as = SpeechInterpretation::Date;
        for policy in [
            InvalidDatePolicy::Preserve,
            InvalidDatePolicy::ReadComponents,
        ] {
            opts.invalid_date_policy = policy;
            for (source, expected) in [
                ("2024-04-24", "にせんにじゅうよねんしがつにじゅうよっか"),
                ("2024-02-29", "にせんにじゅうよねんにがつにじゅうくにち"),
                ("2024-01-17", "にせんにじゅうよねんいちがつじゅうしちにち"),
                ("2024-01-19", "にせんにじゅうよねんいちがつじゅうくにち"),
                ("2024-01-27", "にせんにじゅうよねんいちがつにじゅうしちにち"),
            ] {
                let result = normalize_for_speech(source, &opts).unwrap();
                assert_eq!(result.text, expected, "{style:?} {policy:?} {source}");
                assert!(result.fully_supported);
                assert!(result.readings.iter().all(|r| r.reason.is_none()));
            }
        }
        opts.invalid_date_policy = InvalidDatePolicy::ReadComponents;
        let literal = normalize_for_speech("2024-02-30", &opts).unwrap();
        assert_eq!(literal.text, "にせんにじゅうよねんにがつさんじゅうにち");
        assert_eq!(
            literal.readings[0].reason.as_deref(),
            Some("invalid calendar date")
        );
        opts.invalid_date_policy = InvalidDatePolicy::Preserve;
        assert_eq!(
            normalize_for_speech("2024-02-30", &opts).unwrap().text,
            "2024-02-30"
        );
        assert_eq!(
            normalize_for_speech("令和4年", &opts).unwrap().text,
            "令和よねん"
        );
        opts.number_notation = NumberNotation::Orthographic;
        assert_eq!(
            normalize_for_speech("2024-02-29", &opts).unwrap().text,
            "二千二十四年二月二十九日"
        );
        assert_eq!(
            normalize_for_speech("令和4年", &opts).unwrap().text,
            "令和四年"
        );
    }
    let mut automatic = SpeechOptions::new(SpeechLocale::Ja);
    automatic.reading_style = ReadingStyle::Natural;
    automatic.invalid_date_policy = InvalidDatePolicy::ReadComponents;
    assert_eq!(
        normalize_for_speech("2024-02-29", &automatic).unwrap().text,
        "にせんにじゅうよねんにがつにじゅうくにち"
    );
}

#[test]
fn japanese_temporal_clocks_use_counter_helpers_without_inserting_absent_minutes() {
    use textintel::speech::normalize_for_speech;
    for style in [ReadingStyle::Precise, ReadingStyle::Natural] {
        let mut opts = SpeechOptions::new(SpeechLocale::Ja);
        opts.reading_style = style;
        opts.interpret_as = SpeechInterpretation::Time;
        for (source, expected) in [
            ("4時", "よじ"),
            ("7時", "しちじ"),
            ("9時", "くじ"),
            ("4時1分", "よじいっぷん"),
            ("7時10分", "しちじじゅっぷん"),
            ("4時0分", "よじれいふん"),
            ("4時3秒", "よじさんびょう"),
            ("04:01:03", "よじいっぷんさんびょう"),
        ] {
            let result = normalize_for_speech(source, &opts).unwrap();
            assert_eq!(result.text, expected, "{style:?} {source}");
            assert!(result.fully_supported);
        }
        for (source, expected) in [("4時", "四時"), ("4時1分", "四時一分")] {
            opts.number_notation = NumberNotation::Orthographic;
            assert_eq!(normalize_for_speech(source, &opts).unwrap().text, expected);
        }
    }
}

#[test]
fn compact_parser_projection_preserves_labels_periods_and_explicit_precision() {
    use textintel::speech::{SpeechValue, parse_for_speech};
    let precise = SpeechOptions::new(SpeechLocale::En);
    for (source, expected) in [
        (
            "2024-02-29",
            SpeechValue::Date {
                year: 2024,
                month: 2,
                day: 29,
            },
        ),
        (
            "4:15 PM",
            SpeechValue::Time {
                hour: 16,
                minute: 15,
                second: None,
            },
        ),
        (
            "4:15:00 PM",
            SpeechValue::Time {
                hour: 16,
                minute: 15,
                second: Some(0),
            },
        ),
        (
            "12:15 AM",
            SpeechValue::Time {
                hour: 0,
                minute: 15,
                second: None,
            },
        ),
    ] {
        let doc = parse_for_speech(source, &precise).unwrap();
        assert_eq!(doc.spans.len(), 1);
        assert_eq!(doc.spans[0].value, expected);
    }
    for (locale, source) in [
        (SpeechLocale::Ru, "6/3/1006г."),
        (SpeechLocale::Es, "30 julio del 2020"),
        (SpeechLocale::En, "jul 25 2012"),
        (SpeechLocale::Ja, "令和4年"),
        (SpeechLocale::Ja, "3時"),
        (SpeechLocale::Ko, "오후3시30분"),
    ] {
        let mut opts = SpeechOptions::new(locale);
        opts.date_order = DateOrder::DayMonthYear;
        let doc = parse_for_speech(source, &opts).unwrap();
        assert_eq!(doc.spans.len(), 1, "{source}: {doc:?}");
        assert!(
            matches!(doc.spans[0].value, SpeechValue::Temporal { .. }),
            "{source}: {doc:?}"
        );
    }
    let mut opts = SpeechOptions::new(SpeechLocale::Ru);
    opts.date_order = DateOrder::DayMonthYear;
    let doc = parse_for_speech("6/3/1006г.", &opts).unwrap();
    assert!(matches!(
        &doc.spans[0].value,
        SpeechValue::Temporal {
            value: TemporalValue::Date {
                year_label: true,
                ..
            }
        }
    ));
    for source in [
        "`2024-02-29 4:15 PM`",
        "https://example.org/2024-02-29",
        "user4:15@example.org",
    ] {
        let doc = parse_for_speech(source, &precise).unwrap();
        assert!(
            doc.spans.iter().all(|span| !matches!(
                span.value,
                SpeechValue::Date { .. } | SpeechValue::Time { .. } | SpeechValue::Temporal { .. }
            )),
            "{source}"
        );
    }
}

#[test]
fn vietnamese_written_day_label_is_consumed_once_with_source_metadata() {
    use textintel::speech::{SpeechValue, normalize_for_speech, parse_for_speech};
    for style in [ReadingStyle::Precise, ReadingStyle::Natural] {
        let mut opts = SpeechOptions::new(SpeechLocale::Vi);
        opts.reading_style = style;
        opts.date_order = DateOrder::DayMonthYear;
        for (source, labelled) in [
            ("ngày 15/01/2024", "ngày 15/01/2024"),
            ("Ngày 15/01/2024", "Ngày 15/01/2024"),
            (
                "hôm nay là ngày 19/05/2025 sinh nhật Bác Hồ",
                "ngày 19/05/2025",
            ),
        ] {
            let doc = parse_for_speech(source, &opts).unwrap();
            let span = doc
                .spans
                .iter()
                .find(|span| span.source == labelled)
                .unwrap();
            assert_eq!(&source[span.start..span.end], labelled);
            assert!(matches!(
                span.value,
                SpeechValue::Temporal {
                    value: TemporalValue::Date {
                        presentation: temporal::DatePresentation::Component,
                        ..
                    }
                }
            ));
            let rendered = normalize_for_speech(source, &opts).unwrap();
            assert_eq!(
                rendered.text.matches("ngày").count(),
                1,
                "{style:?}: {}",
                rendered.text
            );
            assert!(!rendered.text.contains("ngày ngày"));
            if source.starts_with("hôm nay") {
                assert!(rendered.text.starts_with("hôm nay là ngày "));
                assert!(rendered.text.ends_with(" sinh nhật Bác Hồ"));
            }
        }
        assert_eq!(
            normalize_for_speech("ngày 15/01/2024", &opts).unwrap().text,
            "ngày mười lăm tháng một năm hai nghìn hai mươi tư"
        );
    }
}

#[test]
fn english_whole_hour_readings_distinguish_twelve_and_twenty_four_hour_forms() {
    use textintel::speech::normalize_for_speech;
    // The Cambridge English Grammar excerpt forbids o'clock for 24-hour times.
    // The US Navy military time reference gives complete hours with "hundred".
    // https://assets.cambridge.org/97813166/36176/excerpt/9781316636176_excerpt.pdf
    // https://media.defense.gov/2014/Jun/20/2002655870/-1/-1/1/140620-N-ZZ182-6514.pdf
    let mut opts = SpeechOptions::new(SpeechLocale::En);
    opts.reading_style = ReadingStyle::Natural;
    opts.interpret_as = SpeechInterpretation::Time;
    for (source, expected) in [
        ("00:00", "zero hundred"),
        ("12:00", "twelve o'clock"),
        ("13:00", "thirteen hundred"),
        ("23:00", "twenty three hundred"),
        ("24:00", "twenty four hundred"),
        ("1:00 PM", "one PM"),
        ("13:30", "thirteen thirty"),
    ] {
        assert_eq!(normalize_for_speech(source, &opts).unwrap().text, expected);
    }
    assert_eq!(normalize_for_speech("24:01", &opts).unwrap().text, "24:01");
}

#[test]
fn german_precise_times_remain_clocks_and_retain_explicit_zero_minutes() {
    use textintel::speech::normalize_for_speech;
    // https://www.duden.de/sprachwissen/sprachratgeber/Uhrzeitangaben
    for (style, expected_zero) in [
        (ReadingStyle::Precise, "acht uhr null minuten"),
        (ReadingStyle::Natural, "acht uhr"),
    ] {
        let mut opts = SpeechOptions::new(SpeechLocale::De);
        opts.reading_style = style;
        opts.interpret_as = SpeechInterpretation::Time;
        for source in ["08:00", "08:00 Uhr"] {
            let result = normalize_for_speech(source, &opts).unwrap();
            assert_eq!(result.text, expected_zero);
            assert!(result.fully_supported);
            assert!(!result.text.contains("Stunden"));
        }
        let seconds = normalize_for_speech("08:00:00", &opts).unwrap();
        assert_eq!(seconds.text, format!("{expected_zero} null sekunden"));
        assert_eq!(
            normalize_for_speech("08:05 Uhr", &opts).unwrap().text,
            "acht uhr fünf"
        );
    }
}

#[test]
fn explicit_meridiem_preserves_time_value_in_all_twenty_renderers() {
    use textintel::speech::{SpeechValue, normalize_for_speech, parse_for_speech};
    for locale in SpeechLocale::ALL {
        let mut opts = SpeechOptions::new(locale);
        opts.reading_style = ReadingStyle::Natural;
        opts.interpret_as = SpeechInterpretation::Time;
        for hour in [1, 3, 12] {
            let am_source = format!("{hour}:15am");
            let pm_source = format!("{hour}:15pm");
            let am = normalize_for_speech(&am_source, &opts).unwrap();
            let pm = normalize_for_speech(&pm_source, &opts).unwrap();
            assert!(am.fully_supported && pm.fully_supported, "{locale:?}");
            assert_ne!(am.text, pm.text, "period lost: {locale:?} {hour}");
            for (marker, source) in [("am", &am_source), ("pm", &pm_source)] {
                let doc = parse_for_speech(source, &opts).unwrap();
                assert_eq!(doc.spans.len(), 1);
                assert!(
                    matches!(&doc.spans[0].value, SpeechValue::Temporal { value: TemporalValue::Clock { hour: h, minute: Some(15), second: None, meridiem: Some(m), period: None, half: false } } if *h == hour && m == marker)
                );
                if !matches!(
                    locale,
                    SpeechLocale::En | SpeechLocale::Es | SpeechLocale::Zh
                ) {
                    let h24 = hour % 12 + if marker == "pm" { 12 } else { 0 };
                    let canonical = normalize_for_speech(&format!("{h24:02}:15"), &opts).unwrap();
                    let actual = normalize_for_speech(source, &opts).unwrap();
                    assert_eq!(actual.text, canonical.text, "{locale:?} {source}");
                }
            }
        }
        let native = "📚 ३:१५pm!";
        let doc = parse_for_speech(native, &opts).unwrap();
        let span = doc
            .spans
            .iter()
            .find(|span| span.source == "३:१५pm")
            .unwrap();
        assert_eq!(&native[span.start..span.end], "३:१५pm");
        assert_eq!(
            normalize_for_speech("३:१५pm", &opts).unwrap().text,
            normalize_for_speech("3:15pm", &opts).unwrap().text
        );
        let hour_only = normalize_for_speech("3pm", &opts).unwrap();
        let explicit_zero = normalize_for_speech("3:00:00pm", &opts).unwrap();
        assert!(hour_only.fully_supported && explicit_zero.fully_supported);
        assert_ne!(
            hour_only.text, explicit_zero.text,
            "explicit second lost: {locale:?}"
        );
        let partial = TemporalValue::Clock {
            hour: 3,
            minute: None,
            second: Some(0),
            meridiem: Some("pm".into()),
            period: None,
            half: false,
        };
        assert!(temporal::render(&partial, locale, &opts).is_some());
        if !matches!(
            locale,
            SpeechLocale::En | SpeechLocale::Es | SpeechLocale::Zh
        ) {
            let unmarked = TemporalValue::Clock {
                hour: 15,
                minute: None,
                second: Some(0),
                meridiem: None,
                period: None,
                half: false,
            };
            assert_eq!(
                temporal::render(&partial, locale, &opts),
                temporal::render(&unmarked, locale, &opts)
            );
        }
        for source in ["13:15am", "00:15pm", "24:00pm"] {
            let bad = normalize_for_speech(source, &opts).unwrap();
            assert_eq!(bad.text, source);
            assert!(!bad.fully_supported);
        }
    }
}

#[test]
fn partial_clock_fields_never_insert_unwritten_minutes() {
    let cases = [
        (SpeechLocale::Ja, "さんじさんびょう"),
        (SpeechLocale::Ko, "세시 삼초"),
        (SpeechLocale::Hi, "तीन बजे तीन सेकंड"),
        (SpeechLocale::Ar, "الساعة الثالثة وثلاث ثوان"),
    ];
    for (locale, expected) in cases {
        let mut opts = SpeechOptions::new(locale);
        opts.reading_style = ReadingStyle::Natural;
        let without_minutes = TemporalValue::Clock {
            hour: 3,
            minute: None,
            second: Some(3),
            meridiem: None,
            period: None,
            half: false,
        };
        let explicit_zero = TemporalValue::Clock {
            hour: 3,
            minute: Some(0),
            second: Some(3),
            meridiem: None,
            period: None,
            half: false,
        };
        let actual = temporal::render(&without_minutes, locale, &opts).unwrap();
        assert_eq!(actual, expected, "{locale:?}");
        assert_ne!(
            actual,
            temporal::render(&explicit_zero, locale, &opts).unwrap(),
            "{locale:?}"
        );
    }
    let mut ko = SpeechOptions::new(SpeechLocale::Ko);
    ko.reading_style = ReadingStyle::Natural;
    let source = "오후3시반";
    let (_, value) = temporal::parse_at(source, 0, &ko).unwrap();
    assert!(matches!(
        value,
        TemporalValue::Clock {
            half: true,
            minute: None,
            ..
        }
    ));
    assert!(
        temporal::render(&value, ko.locale, &ko)
            .unwrap()
            .contains("반")
    );
    let (_, bad) = temporal::parse_at("오전3시pm", 0, &ko).unwrap();
    assert!(matches!(bad, TemporalValue::Invalid { .. }));
}

#[test]
fn german_one_before_uhr_is_attributive_ein() {
    use textintel::speech::normalize_for_speech;
    // Duden: https://www.duden.de/sprachwissen/sprachratgeber/Ein-eins-unbestimmter-Artikel-oder-Zahlwort
    for style in [ReadingStyle::Precise, ReadingStyle::Natural] {
        let mut opts = SpeechOptions::new(SpeechLocale::De);
        opts.reading_style = style;
        for (source, expected) in [
            ("01:05 Uhr", "ein uhr fünf"),
            ("13:00 Uhr", "dreizehn uhr"),
            ("21:00 Uhr", "einundzwanzig uhr"),
        ] {
            let spoken = normalize_for_speech(source, &opts).unwrap().text;
            let expected = if style == ReadingStyle::Precise && source.contains(":00") {
                format!("{expected} null minuten")
            } else {
                expected.into()
            };
            assert_eq!(spoken, expected);
        }
        assert_eq!(
            normalize_for_speech("01:00 Uhr", &opts).unwrap().text,
            if style == ReadingStyle::Precise {
                "ein uhr null minuten"
            } else {
                "ein uhr"
            }
        );
    }
}

#[test]
fn clock_seconds_use_shared_singular_plural_and_gender_agreement() {
    use textintel::speech::normalize_for_speech;
    // Independent lexical gold: Unicode CLDR 48 long unit patterns for
    // duration-second/minute. Spanish/Portuguese units are masculine; German
    // Sekunde/Minute are feminine. Portuguese CLDR one includes integer 0.
    // https://raw.githubusercontent.com/unicode-org/cldr-json/48.0.0/cldr-json/cldr-units-full/main/de/units.json
    // https://raw.githubusercontent.com/unicode-org/cldr-json/48.0.0/cldr-json/cldr-core/supplemental/plurals.json
    for (locale, expected) in [
        (
            SpeechLocale::En,
            [
                "three oh one zero seconds PM",
                "three oh one one second PM",
                "three oh one two seconds PM",
            ],
        ),
        (
            SpeechLocale::Es,
            [
                "tres y uno y cero segundos de la tarde",
                "tres y uno y un segundo de la tarde",
                "tres y uno y dos segundos de la tarde",
            ],
        ),
        (
            SpeechLocale::Pt,
            [
                "quinze horas e um e zero segundo",
                "quinze horas e um e um segundo",
                "quinze horas e um e dois segundos",
            ],
        ),
        (
            SpeechLocale::De,
            [
                "fünfzehn uhr eins null sekunden",
                "fünfzehn uhr eins eine sekunde",
                "fünfzehn uhr eins zwei sekunden",
            ],
        ),
    ] {
        for (style, notation) in [
            (ReadingStyle::Natural, NumberNotation::Phonetic),
            (ReadingStyle::Natural, NumberNotation::Orthographic),
            (ReadingStyle::Precise, NumberNotation::Orthographic),
        ] {
            let mut opts = SpeechOptions::new(locale);
            opts.reading_style = style;
            opts.number_notation = notation;
            opts.interpret_as = SpeechInterpretation::Time;
            for (second, gold) in expected.iter().enumerate() {
                let source = format!("3:01:{second:02}pm");
                let rendered = normalize_for_speech(&source, &opts).unwrap();
                assert_eq!(&rendered.text, gold, "{locale:?} {style:?} {notation:?}");
                assert!(rendered.fully_supported);
                assert_eq!(rendered.readings[0].source, source);
            }
        }
    }
}

#[test]
fn precise_clock_minute_and_second_agreement_keeps_existing_clock_presentation() {
    use textintel::speech::normalize_for_speech;
    for (locale, golds) in [
        (
            SpeechLocale::En,
            [
                "three hours and zero minutes and zero seconds",
                "three hours and one minute and zero seconds",
                "three hours and two minutes and zero seconds",
            ],
        ),
        (
            SpeechLocale::Es,
            [
                "tres horas y cero minutos y cero segundos",
                "tres horas y un minuto y cero segundos",
                "tres horas y dos minutos y cero segundos",
            ],
        ),
        (
            SpeechLocale::Pt,
            [
                "três horas e zero minuto e zero segundo",
                "três horas e um minuto e zero segundo",
                "três horas e dois minutos e zero segundo",
            ],
        ),
        (
            SpeechLocale::De,
            [
                "drei uhr null minuten null sekunden",
                "drei uhr eins null sekunden",
                "drei uhr zwei null sekunden",
            ],
        ),
    ] {
        let mut opts = SpeechOptions::new(locale);
        opts.interpret_as = SpeechInterpretation::Time;
        for (minute, expected) in golds.iter().enumerate() {
            let source = format!("03:{minute:02}:00");
            assert_eq!(
                &normalize_for_speech(&source, &opts).unwrap().text,
                expected
            );
        }
    }
}

#[test]
fn clock_agreement_preserves_absent_fields_zero_precision_and_invalid_bounds() {
    use textintel::speech::{SpeechValue, normalize_for_speech, parse_for_speech};
    for (locale, partial_gold) in [
        (SpeechLocale::En, "three one second PM"),
        (SpeechLocale::Es, "tres y un segundo de la tarde"),
        (SpeechLocale::Pt, "quinze horas e um segundo"),
        (SpeechLocale::De, "fünfzehn uhr eine sekunde"),
    ] {
        let mut opts = SpeechOptions::new(locale);
        opts.reading_style = ReadingStyle::Natural;
        opts.interpret_as = SpeechInterpretation::Time;
        let partial = TemporalValue::Clock {
            hour: 3,
            minute: None,
            second: Some(1),
            period: None,
            meridiem: Some("pm".into()),
            half: false,
        };
        assert_eq!(
            temporal::render(&partial, locale, &opts).as_deref(),
            Some(partial_gold)
        );
        let none = parse_for_speech("3:01pm", &opts).unwrap();
        let zero = parse_for_speech("3:01:00pm", &opts).unwrap();
        assert!(matches!(
            none.spans[0].value,
            SpeechValue::Temporal {
                value: TemporalValue::Clock { second: None, .. }
            }
        ));
        assert!(matches!(
            zero.spans[0].value,
            SpeechValue::Temporal {
                value: TemporalValue::Clock {
                    second: Some(0),
                    ..
                }
            }
        ));
        assert_ne!(
            normalize_for_speech("3:01pm", &opts).unwrap().text,
            normalize_for_speech("3:01:00pm", &opts).unwrap().text
        );
        for source in ["3:01:60pm", "3:60:01pm", "13:01:01am"] {
            let result = normalize_for_speech(source, &opts).unwrap();
            assert_eq!(result.text, source);
            assert!(!result.fully_supported);
        }
        for (minute, second) in [(Some(60), Some(1)), (Some(1), Some(60))] {
            let forged = TemporalValue::Clock {
                hour: 3,
                minute,
                second,
                period: None,
                meridiem: Some("pm".into()),
                half: false,
            };
            assert!(temporal::render(&forged, locale, &opts).is_none());
        }
    }
    for (locale, source, expected) in [
        (SpeechLocale::Ja, "3時1分1秒", "さんじいっぷんいちびょう"),
        (SpeechLocale::Ko, "3시1분1초", "세시 일분 일초"),
    ] {
        let mut opts = SpeechOptions::new(locale);
        opts.reading_style = ReadingStyle::Natural;
        opts.interpret_as = SpeechInterpretation::Time;
        assert_eq!(normalize_for_speech(source, &opts).unwrap().text, expected);
    }
    let mut digits = SpeechOptions::new(SpeechLocale::En);
    digits.reading_style = ReadingStyle::Digits;
    digits.interpret_as = SpeechInterpretation::Time;
    assert!(
        normalize_for_speech("3:01:12pm", &digits)
            .unwrap()
            .text
            .contains("one two seconds")
    );
}

#[test]
fn portuguese_clock_hours_agree_with_hora_without_changing_duration_zero() {
    use textintel::speech::normalize_for_speech;
    for (style, notation) in [
        (ReadingStyle::Natural, NumberNotation::Phonetic),
        (ReadingStyle::Natural, NumberNotation::Orthographic),
        (ReadingStyle::Precise, NumberNotation::Orthographic),
    ] {
        let mut opts = SpeechOptions::new(SpeechLocale::Pt);
        opts.reading_style = style;
        opts.number_notation = notation;
        opts.interpret_as = SpeechInterpretation::Time;
        for (source, expected) in [
            ("01:00", "uma hora"),
            ("21:00", "vinte e uma horas"),
            ("22:00", "vinte e duas horas"),
            ("00:00", "zero horas"),
            ("1:00am", "uma hora"),
            ("1:00pm", "treze horas"),
            ("9:00pm", "vinte e uma horas"),
            ("10:00pm", "vinte e duas horas"),
            ("12:00am", "zero horas"),
            ("12:00pm", "doze horas"),
        ] {
            let result = normalize_for_speech(source, &opts).unwrap();
            assert_eq!(result.text, expected, "{style:?} {notation:?} {source}");
            assert!(result.fully_supported);
        }
    }
    let mut quantity = SpeechOptions::new(SpeechLocale::Pt);
    quantity.reading_style = ReadingStyle::Natural;
    quantity.interpret_as = SpeechInterpretation::Measure;
    for (source, expected) in [
        ("0 h", "zero hora"),
        ("1 h", "uma hora"),
        ("21 h", "vinte e uma horas"),
        ("22 h", "vinte e duas horas"),
    ] {
        assert_eq!(
            normalize_for_speech(source, &quantity).unwrap().text,
            expected
        );
    }
}
