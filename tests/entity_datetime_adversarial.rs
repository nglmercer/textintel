use textintel::RuleBasedEntityProvider;
use textintel::core::providers::EntityProvider;

#[test]
fn slash_dates_require_a_real_calendar_date() {
    let provider = RuleBasedEntityProvider::default();
    for text in [
        "31/31/2024",
        "29/02/2023",
        "04/31/2024",
        "31/04/2024",
        "0/1/2024",
        "1/0/2024",
        "1/1/123",
        "99/99/99",
    ] {
        assert!(
            !provider
                .extract(text, None)
                .iter()
                .any(|e| e.entity_type == "date"),
            "{text}"
        );
    }
    for text in [
        "29/02/2024",
        "02/29/2024",
        "31/12/2024",
        "12/31/2024",
        "1/2/24",
        "1/2/2024",
    ] {
        assert!(
            provider
                .extract(text, None)
                .iter()
                .any(|e| e.entity_type == "date" && e.value == text),
            "{text}"
        );
    }
}

#[test]
fn embedded_and_malformed_temporal_tokens_do_not_yield_valid_suffixes() {
    let provider = RuleBasedEntityProvider::default();
    for text in [
        "a2024-01-01z",
        "12024-01-01",
        "123:45",
        "00:60:20",
        "23:59:99",
        "2024-01-01-99",
        "123/12/2024",
    ] {
        assert!(
            !provider
                .extract(text, None)
                .iter()
                .any(|e| matches!(e.entity_type.as_str(), "date" | "time")),
            "{text}"
        );
    }
}

#[test]
fn clock_and_iso_spans_preserve_offsets_and_do_not_overlap() {
    let provider = RuleBasedEntityProvider::default();
    let text = "📅 2024-02-29, 00:00:00 y 23:59:59.";
    let values: Vec<_> = provider
        .extract(text, Some("es"))
        .into_iter()
        .filter(|e| matches!(e.entity_type.as_str(), "date" | "time"))
        .collect();
    assert_eq!(values.len(), 3);
    for value in &values {
        assert_eq!(&text[value.start..value.end], value.value);
    }
    assert!(values.windows(2).all(|pair| pair[0].end <= pair[1].start));
}
