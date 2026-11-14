use textintel::core::providers::EntityProvider;
use textintel::entities::RuleBasedEntityProvider;

#[test]
fn iso_dates_follow_calendar_and_leap_years() {
    let provider = RuleBasedEntityProvider::default();
    for date in ["2025-02-29", "2024-02-30", "2026-04-31", "1900-02-29"] {
        assert!(
            !provider
                .extract(date, None)
                .iter()
                .any(|e| e.entity_type == "date"),
            "{date}"
        );
    }
    for date in ["2024-02-29", "2000-02-29", "2026-04-30"] {
        assert!(
            provider
                .extract(date, None)
                .iter()
                .any(|e| e.entity_type == "date" && e.value == date),
            "{date}"
        );
    }
}

#[test]
fn amount_first_currency_preserves_decimal_and_thousands_groups() {
    let provider = RuleBasedEntityProvider::default();
    for (text, amount) in [
        ("Pay 1,234.50 USD today", "USD 1,234.50"),
        ("Pay 12,50 EUR", "EUR 12,50"),
    ] {
        let found = provider.extract(text, None);
        let currency = found.iter().find(|e| e.entity_type == "currency").unwrap();
        assert_eq!(currency.value, amount);
        assert_eq!(
            &text[currency.start..currency.end],
            &text[4..text.find(" today").unwrap_or(text.len())]
        );
    }
}

#[test]
fn urls_keep_case_insensitive_prefixes_and_utf8_offsets() {
    let text = "你好 HTTPS://example.test/a and WWW.example.test";
    let urls: Vec<_> = RuleBasedEntityProvider::default()
        .extract(text, None)
        .into_iter()
        .filter(|e| e.entity_type == "url")
        .collect();
    assert_eq!(urls.len(), 2);
    assert_eq!(&text[urls[0].start..urls[0].end], "HTTPS://example.test/a");
}
