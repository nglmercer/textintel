use textintel::speech::temporal::{TemporalValue, TimestampOffset};
use textintel::speech::{
    SpeechInterpretation, SpeechLocale, SpeechOptions, SpeechValue, normalize_for_speech,
    parse_for_speech,
};

fn timestamp(source: &str, options: &SpeechOptions) -> TemporalValue {
    let parsed = parse_for_speech(source, options).unwrap();
    assert_eq!(parsed.spans.len(), 1, "{source}: {parsed:?}");
    assert_eq!(parsed.spans[0].source, source);
    let SpeechValue::Temporal { value } = &parsed.spans[0].value else {
        panic!(
            "expected temporal value for {source:?}: {:?}",
            parsed.spans[0]
        );
    };
    value.clone()
}

#[test]
fn rfc3339_timestamps_keep_components_and_render_in_all_twenty_locales() {
    let cases = [
        (
            "2024-02-29T03:15:01.001000Z",
            TimestampOffset::Utc,
            Some("001000"),
        ),
        (
            "2024-02-29t03:15:01.001000z",
            TimestampOffset::Utc,
            Some("001000"),
        ),
        (
            "2024-02-29T03:15:01.001000+05:30",
            TimestampOffset::Known {
                negative: false,
                hours: 5,
                minutes: 30,
            },
            Some("001000"),
        ),
        (
            "2024-02-29T03:15:01-08:00",
            TimestampOffset::Known {
                negative: true,
                hours: 8,
                minutes: 0,
            },
            None,
        ),
        (
            "2024-02-29T03:15:01-00:00",
            TimestampOffset::UnknownLocal,
            None,
        ),
    ];
    for locale in SpeechLocale::ALL {
        let options = SpeechOptions::new(locale);
        for (source, expected_offset, expected_fraction) in &cases {
            assert!(
                matches!(
                timestamp(source, &options),
                    TemporalValue::Timestamp {
                        year,
                        month: 2,
                        day: 29,
                        hour: 3,
                        minute: 15,
                        second: 1,
                        fraction,
                        offset,
                    } if year == "2024"
                        && fraction.as_deref() == *expected_fraction
                    && offset == *expected_offset
                ),
                "{locale:?}: {source}"
            );
            let rendered = normalize_for_speech(source, &options).unwrap();
            assert!(
                rendered.fully_supported,
                "{locale:?}: {source}: {rendered:?}"
            );
            assert_ne!(rendered.text, *source);
        }
    }
}

#[test]
fn explicit_datetime_hint_is_available_and_other_hints_preserve_the_full_value() {
    let source = "2024-02-29T03:15:01Z";
    let mut options = SpeechOptions::new(SpeechLocale::En);
    options.interpret_as = SpeechInterpretation::DateTime;
    assert!(matches!(
        timestamp(source, &options),
        TemporalValue::Timestamp { .. }
    ));
    assert!(
        normalize_for_speech(source, &options)
            .unwrap()
            .fully_supported
    );

    for hint in [SpeechInterpretation::Date, SpeechInterpretation::Time] {
        options.interpret_as = hint;
        let parsed = parse_for_speech(source, &options).unwrap();
        assert_eq!(parsed.spans.len(), 1, "{hint:?}: {parsed:?}");
        assert!(matches!(
            parsed.spans[0].value,
            SpeechValue::Ambiguous { .. }
        ));
        let rendered = normalize_for_speech(source, &options).unwrap();
        assert_eq!(rendered.text, source);
        assert!(!rendered.fully_supported);
    }
}

#[test]
fn malformed_and_unverified_timestamp_candidates_stay_atomic() {
    for source in [
        "2023-02-29T03:15:01Z",
        "2024-02-29T24:00:00Z",
        "2024-02-29T03:60:01Z",
        "2024-02-29T03:15:61Z",
        "2024-02-29T03:15:01",
        "2024-02-29T03:15:01+24:00",
        "2024-02-29T03:15:01+05:60",
        "2024-02-29T03:15:01,001Z",
        "2024-02-29T03:15:01.Z",
        "2024-02-29T03:15:01Z[Europe/Paris]",
        "2024-02-29T03:15:60Z",
        "2024-02-29T03:15Z",
        "2024-02-29T03:15:01+0530",
        "2024-02-29T03:15:01+05:30:00",
        "2026-12-31T23:59:60Z",
    ] {
        for locale in SpeechLocale::ALL {
            let options = SpeechOptions::new(locale);
            let parsed = parse_for_speech(source, &options).unwrap();
            assert_eq!(parsed.spans.len(), 1, "{locale:?}/{source}: {parsed:?}");
            assert_eq!(parsed.spans[0].source, source);
            assert!(matches!(
                parsed.spans[0].value,
                SpeechValue::Ambiguous { .. }
            ));
            let rendered = normalize_for_speech(source, &options).unwrap();
            assert_eq!(rendered.text, source);
            assert!(!rendered.fully_supported);
        }
    }

    let long_fraction = format!("2024-02-29T03:15:01.{}Z", "1".repeat(129));
    let options = SpeechOptions::new(SpeechLocale::En);
    let parsed = parse_for_speech(&long_fraction, &options).unwrap();
    assert_eq!(parsed.spans.len(), 1);
    assert_eq!(parsed.spans[0].source, long_fraction);
    assert!(matches!(
        parsed.spans[0].value,
        SpeechValue::Ambiguous { .. }
    ));
}

#[test]
fn timestamp_fraction_cap_is_exact_and_supported_in_all_twenty_locales() {
    let fraction = "0".repeat(128);
    let source = format!("2024-02-29T03:15:01.{fraction}Z");
    for locale in SpeechLocale::ALL {
        let options = SpeechOptions::new(locale);
        assert!(
            matches!(
                timestamp(&source, &options),
                TemporalValue::Timestamp { fraction: Some(actual), .. } if actual == fraction
            ),
            "{locale:?}"
        );
        let rendered = normalize_for_speech(&source, &options).unwrap();
        assert!(rendered.fully_supported, "{locale:?}: {rendered:?}");
    }
}

#[test]
fn verified_leap_seconds_validate_in_utc_across_offsets() {
    let options = SpeechOptions::new(SpeechLocale::En);
    for source in [
        "1990-12-31T23:59:60Z",
        "1990-12-31T15:59:60-08:00",
        "1991-01-01T00:59:60+01:00",
    ] {
        assert!(matches!(
            timestamp(source, &options),
            TemporalValue::Timestamp { second: 60, .. }
        ));
        let rendered = normalize_for_speech(source, &options).unwrap();
        assert!(rendered.fully_supported, "{source}: {rendered:?}");
    }
    let invalid = "1990-12-31T15:58:60-08:00";
    let parsed = parse_for_speech(invalid, &options).unwrap();
    assert_eq!(parsed.spans.len(), 1);
    assert!(matches!(
        parsed.spans[0].value,
        SpeechValue::Ambiguous { .. }
    ));
}

#[test]
fn four_digit_year_zero_follows_rfc_3339_calendar_rules() {
    for locale in SpeechLocale::ALL {
        let options = SpeechOptions::new(locale);
        let source = "0000-02-29T03:15:01Z";
        assert!(matches!(
            timestamp(source, &options),
            TemporalValue::Timestamp { year, month: 2, day: 29, .. } if year == "0000"
        ));
        let rendered = normalize_for_speech(source, &options).unwrap();
        assert!(rendered.fully_supported, "{locale:?}: {rendered:?}");
    }
}

#[test]
fn timestamps_keep_following_sentence_punctuation_outside_the_value() {
    let source = "Event: 2024-02-29T03:15:01Z, received.";
    let options = SpeechOptions::new(SpeechLocale::En);
    let parsed = parse_for_speech(source, &options).unwrap();
    assert_eq!(parsed.spans[1].source, "2024-02-29T03:15:01Z");
    assert_eq!(parsed.spans[2].source, ", received.");
    assert!(
        normalize_for_speech(source, &options)
            .unwrap()
            .fully_supported
    );
}
