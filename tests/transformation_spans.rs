//! Transformation provenance: every rewrite step carries its original UTF-8
//! span, source, replacement, type, confidence, provider, and language.

use textintel::{EngineConfig, TextIntelligence};

fn engine() -> TextIntelligence {
    TextIntelligence::new(EngineConfig::default())
}

#[test]
fn rebus_steps_point_at_exact_utf8_spans() {
    let languages = vec!["es".to_string()];
    let candidates = engine()
        .decode_with_languages("Fr4🏠d0", Some(&languages), Some(3))
        .unwrap();
    let top = &candidates[0];
    assert_eq!(top.text, "Fracasado");
    assert_eq!(top.transformations.len(), 3);
    let message = "Fr4🏠d0";
    let expected = [("4", "a", 2, 3), ("🏠", "casa", 3, 7), ("0", "o", 8, 9)];
    for (step, (source, replacement, start, end)) in top.transformations.iter().zip(expected) {
        assert_eq!(step.source, source);
        assert_eq!(step.replacement, replacement);
        assert_eq!((step.start, step.end), (Some(start), Some(end)));
        assert_eq!(step.span.as_deref(), Some(source));
        // The span must slice the original message on char boundaries.
        assert_eq!(&message[start..end], source);
        assert!(message.is_char_boundary(start) && message.is_char_boundary(end));
        assert_eq!(step.provider.as_deref(), Some("rebus"));
        let confidence = step.confidence.expect("confidence");
        assert!((0.0..=1.0).contains(&confidence));
    }
    // The symbol step knows its language; the neutral leet steps do not.
    assert_eq!(top.transformations[1].language.as_deref(), Some("es"));
    assert_eq!(top.transformations[0].language, None);
    // Human explanations render the full chain.
    let explanations: Vec<String> = top
        .transformations
        .iter()
        .map(|step| step.explain())
        .collect();
    assert_eq!(
        explanations,
        vec![
            "4 = a (number_reading)".to_string(),
            "🏠 = casa (symbol_reading; es)".to_string(),
            "0 = o (number_reading)".to_string(),
        ]
    );
}

#[test]
fn normalization_steps_carry_whole_text_spans() {
    let fingerprint = engine().analyze("HELLO world").unwrap();
    assert_eq!(fingerprint.raw, "HELLO world");
    assert!(!fingerprint.transformations.is_empty());
    for step in &fingerprint.transformations {
        assert_eq!(step.source, "HELLO world");
        assert_eq!(
            (step.start, step.end),
            (Some(0), "HELLO world".len().into())
        );
        assert_eq!(step.span.as_deref(), Some("HELLO world"));
        assert!(step.transformation_type.starts_with("normalization:"));
        assert!(matches!(
            step.provider.as_deref(),
            Some("normalization" | "transliteration")
        ));
    }
    // Transliteration steps are tagged with their own provider.
    let fingerprint = engine().analyze("привет").unwrap();
    assert!(
        fingerprint
            .transformations
            .iter()
            .any(|step| step.provider.as_deref() == Some("transliteration")),
        "transliteration steps must be tagged: {:?}",
        fingerprint.transformations
    );
}

#[test]
fn legacy_three_field_transformations_still_parse() {
    let legacy = serde_json::json!({
        "source": "4",
        "replacement": "a",
        "transformation_type": "leetspeak"
    });
    let step: textintel::core::types::Transformation = serde_json::from_value(legacy).unwrap();
    assert_eq!(step.source, "4");
    assert_eq!(step.start, None);
    assert_eq!(step.provider, None);
    assert_eq!(step.explain(), "4 = a (leetspeak)");
}
