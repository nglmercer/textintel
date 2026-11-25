use textintel::speech::temporal::{self, DatePresentation, TemporalValue};
use textintel::speech::{
    ClockPolicy, NumberNotation, ReadingStyle, SpeechInterpretation, SpeechLocale, SpeechOptions,
    SpeechValue, normalize_for_speech, parse_for_speech,
};

fn fractional(value: &SpeechValue) -> Option<(&TemporalValue, &str)> {
    match value {
        SpeechValue::Temporal {
            value: TemporalValue::FractionalClock { clock, fraction },
        } => Some((clock, fraction)),
        _ => None,
    }
}

#[test]
fn fractional_seconds_keep_exact_precision_in_all_twenty_languages() {
    for locale in SpeechLocale::ALL {
        for style in [
            ReadingStyle::Precise,
            ReadingStyle::Natural,
            ReadingStyle::Digits,
        ] {
            for notation in [NumberNotation::Phonetic, NumberNotation::Orthographic] {
                let mut opts = SpeechOptions::new(locale);
                opts.reading_style = style;
                opts.number_notation = notation;
                for second in [0, 1, 2] {
                    let mut spoken = Vec::new();
                    for digits in ["001", "0010", "001000", "000", "0000"] {
                        let source = format!("03:15:{second:02}.{digits}");
                        let parsed = parse_for_speech(&source, &opts).unwrap();
                        assert_eq!(parsed.spans.len(), 1, "{locale:?}: {parsed:?}");
                        let (clock, fraction) = fractional(&parsed.spans[0].value).unwrap();
                        assert_eq!(fraction, digits);
                        assert!(matches!(
                            clock,
                            TemporalValue::Clock {
                                hour: 3,
                                minute: Some(15),
                                second: Some(actual_second),
                                half: false,
                                ..
                            } if *actual_second == second
                        ));
                        let rendered = normalize_for_speech(&source, &opts).unwrap();
                        assert!(rendered.fully_supported, "{locale:?}: {rendered:?}");
                        assert!(rendered.readings.iter().all(|r| r.reason.is_none()));
                        spoken.push(rendered.text);
                    }
                    for (index, current) in spoken.iter().enumerate() {
                        assert!(
                            !spoken[..index].contains(current),
                            "precision lost for {locale:?}/{style:?}/{notation:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn english_clock_reads_decimal_seconds_and_keeps_the_written_period() {
    let mut opts = SpeechOptions::new(SpeechLocale::En);
    opts.reading_style = ReadingStyle::Natural;
    assert_eq!(
        normalize_for_speech("03:15:01.001000 pm", &opts)
            .unwrap()
            .text,
        "three fifteen one point zero zero one zero zero zero seconds PM"
    );
    assert_eq!(
        normalize_for_speech("03:15:00.000", &opts).unwrap().text,
        "three fifteen zero point zero zero zero seconds"
    );
}

#[test]
fn fractional_clock_period_suffixes_reuse_the_same_clock_decoder_in_all_locales() {
    for locale in SpeechLocale::ALL {
        let opts = SpeechOptions::new(locale);
        for (source, marker) in [
            ("03:15:01.001000 p.m.", "pm"),
            ("03:15:01.001000am", "am"),
            ("03:15:01.001000 A.M.", "am"),
        ] {
            let parsed = parse_for_speech(source, &opts).unwrap();
            assert_eq!(parsed.spans.len(), 1, "{parsed:?}");
            assert_eq!(parsed.spans[0].source, source);
            let (clock, fraction) = fractional(&parsed.spans[0].value).unwrap();
            assert_eq!(fraction, "001000");
            assert!(matches!(clock, TemporalValue::Clock {
                hour: 3, meridiem: Some(actual), ..
            } if actual == marker));
            assert!(normalize_for_speech(source, &opts).unwrap().fully_supported);
        }
    }
}

#[test]
fn korean_fractional_clock_consumes_and_preserves_its_period_prefix() {
    for style in [
        ReadingStyle::Precise,
        ReadingStyle::Natural,
        ReadingStyle::Digits,
    ] {
        for notation in [NumberNotation::Phonetic, NumberNotation::Orthographic] {
            let mut opts = SpeechOptions::new(SpeechLocale::Ko);
            opts.reading_style = style;
            opts.number_notation = notation;
            for (period, marker) in [
                ("오전", "am"),
                ("오후", "pm"),
                ("새벽", "am"),
                ("아침", "am"),
                ("저녁", "pm"),
                ("밤", "pm"),
            ] {
                for space in ["", " ", "\u{a0}"] {
                    let source = format!("{period}{space}03:15:01.001000{marker}");
                    let parsed = parse_for_speech(&source, &opts).unwrap();
                    assert_eq!(parsed.spans.len(), 1, "{parsed:?}");
                    assert_eq!(parsed.spans[0].start, 0);
                    assert_eq!(parsed.spans[0].end, source.len());
                    assert_eq!(parsed.spans[0].source, source);
                    let (clock, fraction) = fractional(&parsed.spans[0].value).unwrap();
                    assert_eq!(fraction, "001000");
                    assert!(matches!(clock, TemporalValue::Clock {
                        hour: 3, minute: Some(15), second: Some(1),
                        period: Some(actual_period), meridiem: Some(actual_marker), half: false,
                    } if actual_period == period && actual_marker == marker));
                    let normalized = normalize_for_speech(&source, &opts).unwrap();
                    assert!(normalized.fully_supported, "{normalized:?}");
                    assert!(normalized.text.starts_with(&format!("{period} ")));
                }
            }
        }
    }
}

#[test]
fn contradictory_korean_fractional_periods_are_preserved_as_one_invalid_value() {
    for style in [
        ReadingStyle::Precise,
        ReadingStyle::Natural,
        ReadingStyle::Digits,
    ] {
        for policy in [ClockPolicy::Strict, ClockPolicy::NormalizeRedundantPeriod] {
            let mut opts = SpeechOptions::new(SpeechLocale::Ko);
            opts.reading_style = style;
            opts.clock_policy = policy;
            for (period, marker) in [("오전", "pm"), ("오후", "am")] {
                for space in ["", " "] {
                    let source = format!("{period}{space}03:15:01.001{marker}");
                    let parsed = parse_for_speech(&source, &opts).unwrap();
                    assert_eq!(parsed.spans.len(), 1, "{parsed:?}");
                    assert_eq!(parsed.spans[0].source, source);
                    assert!(matches!(&parsed.spans[0].value,
                        SpeechValue::Ambiguous { reason } if reason == "contradictory clock periods"));
                    let normalized = normalize_for_speech(&source, &opts).unwrap();
                    assert!(!normalized.fully_supported, "{normalized:?}");
                    assert_eq!(normalized.text, source);
                    assert!(normalized.readings[0].reason.is_some());
                }
            }
        }
    }
}

#[test]
fn korean_prefix_clocks_keep_fraction_bounds_and_invalid_candidates_atomic() {
    let mut opts = SpeechOptions::new(SpeechLocale::Ko);
    opts.reading_style = ReadingStyle::Natural;
    for space in ["", " "] {
        let source = format!("오전{space}03:15:01.{}am", "0".repeat(128));
        let parsed = parse_for_speech(&source, &opts).unwrap();
        assert_eq!(parsed.spans.len(), 1);
        assert_eq!(fractional(&parsed.spans[0].value).unwrap().1.len(), 128);
        assert!(
            normalize_for_speech(&source, &opts)
                .unwrap()
                .fully_supported
        );
        for body in [
            format!("03:15:01.{}am", "0".repeat(129)),
            "03:15:01.001Z".into(),
            "03:15:01.001 UTC".into(),
            "03:60:01.001am".into(),
            "03:15:60.001am".into(),
        ] {
            let source = format!("오전{space}{body}");
            let parsed = parse_for_speech(&source, &opts).unwrap();
            assert_eq!(parsed.spans.len(), 1, "{parsed:?}");
            assert_eq!(parsed.spans[0].source, source);
            assert!(matches!(
                parsed.spans[0].value,
                SpeechValue::Ambiguous { .. }
            ));
            assert_eq!(normalize_for_speech(&source, &opts).unwrap().text, source);
            assert!(
                !normalize_for_speech(&source, &opts)
                    .unwrap()
                    .fully_supported
            );
        }
    }
}

#[test]
fn korean_formal_periods_render_twelve_hour_clocks_without_redundant_conversion() {
    let mut opts = SpeechOptions::new(SpeechLocale::Ko);
    opts.reading_style = ReadingStyle::Natural;
    for (source, expected) in [
        ("오전03:15:01.001am", "오전 세시 십오분 일 점 영 영 일초"),
        ("오전 03:15:01.001am", "오전 세시 십오분 일 점 영 영 일초"),
        ("오후03:15:01.001pm", "오후 세시 십오분 일 점 영 영 일초"),
        ("오후 03:15:01.001pm", "오후 세시 십오분 일 점 영 영 일초"),
        ("오후03:15:01.001", "오후 세시 십오분 일 점 영 영 일초"),
        ("오후03:15", "오후 세시 십오분"),
    ] {
        let normalized = normalize_for_speech(source, &opts).unwrap();
        assert!(normalized.fully_supported, "{normalized:?}");
        assert_eq!(normalized.text, expected);
        assert_eq!(normalized.readings.len(), 1);
    }
    opts.clock_policy = ClockPolicy::NormalizeRedundantPeriod;
    for (source, expected) in [
        ("오후15:15:01.001", "오후 세시 십오분 일 점 영 영 일초"),
        ("오전00:15:01.001", "오전 열두시 십오분 일 점 영 영 일초"),
    ] {
        let normalized = normalize_for_speech(source, &opts).unwrap();
        assert!(normalized.fully_supported, "{normalized:?}");
        assert_eq!(normalized.text, expected);
    }
}

#[test]
fn korean_formal_periods_apply_the_same_strict_bounds_without_an_english_marker() {
    let mut opts = SpeechOptions::new(SpeechLocale::Ko);
    opts.reading_style = ReadingStyle::Natural;
    for source in [
        "오전15:15",
        "오전 15:15",
        "오후00:15",
        "오후 00:15",
        "오전15:15:01.001",
        "오전 15:15:01.001",
        "오후00:15:01.001",
    ] {
        let parsed = parse_for_speech(source, &opts).unwrap();
        assert_eq!(parsed.spans.len(), 1, "{parsed:?}");
        assert_eq!(parsed.spans[0].source, source);
        assert!(matches!(
            parsed.spans[0].value,
            SpeechValue::Ambiguous { .. }
        ));
        let normalized = normalize_for_speech(source, &opts).unwrap();
        assert!(!normalized.fully_supported, "{normalized:?}");
        assert_eq!(normalized.text, source);
    }
    // Broad day periods remain lexical; they are not guessed AM/PM intervals.
    for period in ["새벽", "아침", "저녁", "밤"] {
        let source = format!("{period}15:15:01.001");
        let parsed = parse_for_speech(&source, &opts).unwrap();
        assert_eq!(parsed.spans.len(), 1, "{parsed:?}");
        assert!(matches!(
            fractional(&parsed.spans[0].value).unwrap().0,
            TemporalValue::Clock {
                hour: 15,
                meridiem: None,
                ..
            }
        ));
    }
    opts.clock_policy = ClockPolicy::NormalizeRedundantPeriod;
    for hour in [0, 15] {
        let clock = TemporalValue::Clock {
            hour,
            minute: Some(15),
            second: Some(1),
            period: Some("오후".into()),
            meridiem: None,
            half: false,
        };
        assert!(temporal::render(&clock, opts.locale, &opts).is_none());
        let fractional = TemporalValue::FractionalClock {
            clock: Box::new(clock),
            fraction: "001".into(),
        };
        assert!(temporal::render(&fractional, opts.locale, &opts).is_none());
    }
}

#[test]
fn comma_fraction_requires_explicit_time_intent() {
    for locale in SpeechLocale::ALL {
        let mut opts = SpeechOptions::new(locale);
        opts.reading_style = ReadingStyle::Natural;
        let source = "03:15:01,001000";
        let auto = parse_for_speech(source, &opts).unwrap();
        assert!(auto.spans.iter().all(|s| fractional(&s.value).is_none()));
        opts.interpret_as = SpeechInterpretation::Time;
        let explicit = parse_for_speech(source, &opts).unwrap();
        assert_eq!(explicit.spans.len(), 1, "{locale:?}: {explicit:?}");
        assert_eq!(fractional(&explicit.spans[0].value).unwrap().1, "001000");
        assert!(normalize_for_speech(source, &opts).unwrap().fully_supported);
    }
}

#[test]
fn native_digits_are_canonicalized_without_changing_source_or_offsets() {
    for digits in [
        "０３:１５:０１.００１０００",
        "٠٣:١٥:٠١.٠٠١٠٠٠",
        "০৩:১৫:০১.০০১০০০",
    ] {
        for locale in SpeechLocale::ALL {
            let source = format!("🙂 ({digits})。 listo");
            let opts = SpeechOptions::new(locale);
            let parsed = parse_for_speech(&source, &opts).unwrap();
            let span = parsed
                .spans
                .iter()
                .find(|s| fractional(&s.value).is_some())
                .unwrap();
            assert_eq!(span.source, digits);
            assert_eq!(&source[span.start..span.end], digits);
            assert_eq!(fractional(&span.value).unwrap().1, "001000");
            assert_eq!(
                parsed
                    .spans
                    .iter()
                    .map(|s| s.source.as_str())
                    .collect::<String>(),
                source
            );
        }
    }
}

#[test]
fn malformed_and_unsupported_clock_candidates_are_never_read_as_prefixes() {
    for locale in SpeechLocale::ALL {
        for style in [ReadingStyle::Precise, ReadingStyle::Natural] {
            let mut opts = SpeechOptions::new(locale);
            opts.reading_style = style;
            for source in [
                "25:15:01.001",
                "03:60:01.001",
                "03:15:60.001",
                "24:00:00.001",
                "03:15:01..001",
                "03:15:01.001.5",
                "03:15:01.001Z",
                "03:15:01.001+05:30",
                "03:15:01.001s",
                "03:15:01.001pmZ",
                "03:15:01.Z",
                "03:15:01.pm",
                "03:15:01.001UTC",
                "03:15:01.001 UTC",
                "03:15:01.001 Z",
                "03:15:01.001 +05:30",
                "03:15:01.001 -05:30",
                "03:15:01.001 pm UTC",
                "003:15:01.001",
                "03:1:01.001",
                "03:15:1.001",
                "-03:15:01.001",
                "03:15:-1.001",
            ] {
                let parsed = parse_for_speech(source, &opts).unwrap();
                assert_eq!(parsed.spans.len(), 1, "{locale:?}: {parsed:?}");
                assert!(
                    matches!(parsed.spans[0].value, SpeechValue::Ambiguous { .. }),
                    "{source}: {parsed:?}"
                );
                let rendered = normalize_for_speech(source, &opts).unwrap();
                assert!(!rendered.fully_supported, "{locale:?}: {rendered:?}");
                assert_eq!(rendered.text, source);
                assert!(rendered.readings[0].reason.is_some());
            }
        }
    }
}

#[test]
fn sentence_punctuation_stays_outside_fractional_and_integral_clocks() {
    for source in [
        "03:15:01.",
        "03:15:01.001.",
        "03:15:01.001。",
        "03:15:01.001!",
    ] {
        let opts = SpeechOptions::new(SpeechLocale::En);
        let parsed = parse_for_speech(source, &opts).unwrap();
        assert_eq!(parsed.spans.len(), 2, "{parsed:?}");
        assert_eq!(
            parsed.spans[1].source,
            source.chars().last().unwrap().to_string()
        );
        assert!(matches!(parsed.spans[1].value, SpeechValue::Text));
        assert!(normalize_for_speech(source, &opts).unwrap().fully_supported);
    }
}

#[test]
fn subsequent_prose_is_not_reclassified_as_an_unsupported_zone() {
    let opts = SpeechOptions::new(SpeechLocale::En);
    for source in [
        "03:15:01.001 listo",
        "03:15:01.001 UTCx",
        "03:15:01.001, listo",
    ] {
        let parsed = parse_for_speech(source, &opts).unwrap();
        assert_eq!(fractional(&parsed.spans[0].value).unwrap().1, "001");
        assert_eq!(parsed.spans[0].source, "03:15:01.001");
        assert!(normalize_for_speech(source, &opts).unwrap().fully_supported);
    }
}

#[test]
fn precision_limit_is_checked_for_the_complete_candidate() {
    let opts = SpeechOptions::new(SpeechLocale::En);
    let source = format!("03:15:01.{}", "0".repeat(128));
    let parsed = parse_for_speech(&source, &opts).unwrap();
    assert_eq!(fractional(&parsed.spans[0].value).unwrap().1.len(), 128);
    assert!(
        normalize_for_speech(&source, &opts)
            .unwrap()
            .fully_supported
    );
    let source = format!("03:15:01.{}", "0".repeat(129));
    let parsed = parse_for_speech(&source, &opts).unwrap();
    assert_eq!(parsed.spans.len(), 1);
    assert_eq!(parsed.spans[0].source, source);
    assert!(matches!(
        parsed.spans[0].value,
        SpeechValue::Ambiguous { .. }
    ));
    assert!(
        !normalize_for_speech(&source, &opts)
            .unwrap()
            .fully_supported
    );
}

#[test]
fn fractional_clock_does_not_bypass_explicit_hints_or_clock_policy() {
    let mut opts = SpeechOptions::new(SpeechLocale::En);
    for hint in [
        SpeechInterpretation::Number,
        SpeechInterpretation::Date,
        SpeechInterpretation::Currency,
        SpeechInterpretation::Measure,
        SpeechInterpretation::Duration,
    ] {
        opts.interpret_as = hint;
        assert!(
            !normalize_for_speech("03:15:01.001", &opts)
                .unwrap()
                .fully_supported,
            "{hint:?}"
        );
    }
    opts.interpret_as = SpeechInterpretation::Time;
    assert!(
        !normalize_for_speech("13:15:01.001pm", &opts)
            .unwrap()
            .fully_supported
    );
    opts.clock_policy = ClockPolicy::NormalizeRedundantPeriod;
    let parsed = parse_for_speech("13:15:01.001pm", &opts).unwrap();
    assert!(matches!(fractional(&parsed.spans[0].value).unwrap().0,
        TemporalValue::Clock { hour: 1, meridiem: Some(marker), .. } if marker == "pm"));
    assert!(
        normalize_for_speech("13:15:01.001pm", &opts)
            .unwrap()
            .fully_supported
    );
}

#[test]
fn protected_data_and_full_timestamps_are_not_fractional_clocks() {
    for locale in SpeechLocale::ALL {
        let opts = SpeechOptions::new(locale);
        for source in [
            "`03:15:01.001`",
            "```\n03:15:01.001\n```",
            "{\"time\":\"03:15:01.001\"}",
            "https://example.org/03:15:01.001",
            "/logs/03:15:01.001.wav",
            "03:15:01.001.wav",
            "<time value=\"03:15:01.001\">x</time>",
            "03:15:01.001@example.org",
        ] {
            let parsed = parse_for_speech(source, &opts).unwrap();
            assert!(
                parsed.spans.iter().all(|s| fractional(&s.value).is_none()),
                "{source}: {parsed:?}"
            );
            let rendered = normalize_for_speech(source, &opts).unwrap();
            assert_eq!(rendered.text, source);
            assert!(!rendered.fully_supported, "{source}: {rendered:?}");
        }
        let source = "2024-02-29T03:15:01.001Z";
        let parsed = parse_for_speech(source, &opts).unwrap();
        assert_eq!(parsed.spans.len(), 1);
        assert!(matches!(
            parsed.spans[0].value,
            SpeechValue::Temporal {
                value: TemporalValue::Timestamp { .. }
            }
        ));
        assert!(normalize_for_speech(source, &opts).unwrap().fully_supported);
    }
}

#[test]
fn externally_constructed_wrappers_are_bounded_nonrecursive_and_validated() {
    let opts = SpeechOptions::new(SpeechLocale::En);
    let valid = TemporalValue::Clock {
        hour: 3,
        minute: Some(15),
        second: Some(1),
        period: None,
        meridiem: None,
        half: false,
    };
    let wrap = |clock, fraction: &str| TemporalValue::FractionalClock {
        clock: Box::new(clock),
        fraction: fraction.into(),
    };
    for fraction in ["", "١", "1.2", "-1", "1e2", &"0".repeat(129)] {
        assert!(temporal::render(&wrap(valid.clone(), fraction), opts.locale, &opts).is_none());
    }
    for (minute, second, half, hour) in [
        (None, Some(1), false, 3),
        (Some(15), None, false, 3),
        (Some(15), Some(1), true, 3),
        (Some(15), Some(60), false, 3),
        (Some(0), Some(0), false, 24),
    ] {
        let mut clock = valid.clone();
        if let TemporalValue::Clock {
            minute: m,
            second: s,
            half: h,
            hour: n,
            ..
        } = &mut clock
        {
            *m = minute;
            *s = second;
            *h = half;
            *n = hour;
        }
        assert!(temporal::render(&wrap(clock, "001"), opts.locale, &opts).is_none());
    }
    let wrapped = wrap(valid, "001000");
    assert!(temporal::render(&wrapped, opts.locale, &opts).is_some());
    assert!(temporal::render(&wrap(wrapped.clone(), "001"), opts.locale, &opts).is_none());
    assert!(
        temporal::render(
            &wrap(TemporalValue::Decade { year: 1980 }, "001"),
            opts.locale,
            &opts
        )
        .is_none()
    );
    let date = TemporalValue::Date {
        year: Some("2024".into()),
        month: Some(2),
        day: Some(29),
        presentation: DatePresentation::Numeric,
        connector: None,
        year_label: false,
    };
    assert!(temporal::render(&wrap(date, "001"), opts.locale, &opts).is_none());
    assert_eq!(
        serde_json::from_str::<TemporalValue>(&serde_json::to_string(&wrapped).unwrap()).unwrap(),
        wrapped
    );
}
