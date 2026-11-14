use textintel::core::providers::LanguageDetectionProvider;
use textintel::{CapabilityLevel, ProfileLanguageDetector, ResourceLoader};

#[test]
fn resource_profile_detector_obeys_its_supplied_language_scope_and_limit() {
    let mut resources = ResourceLoader::default();
    resources
        .load_language_json(
            r#"{"language":"es","words":["hola","mundo","gracias"],"stop_words":["el","de"]}"#,
            "<es-fixture>",
        )
        .unwrap();
    let detector = ProfileLanguageDetector::new(resources).with_max_candidates(0);
    assert_eq!(detector.resources().languages(), vec!["es"]);
    let result = detector.detect("hola mundo gracias").unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].language, "es");
    assert!(result[0].probability.is_finite());
    assert_eq!(detector.capabilities().quality, CapabilityLevel::Basic);
    assert_eq!(detector.capabilities().languages, vec!["es"]);
    let unknown = detector.detect("").unwrap();
    assert_eq!(unknown.len(), 1);
    assert_eq!(unknown[0].language, "unknown");
    assert_eq!(unknown[0].probability, 1.0);
}

#[test]
fn embedded_detector_reports_actual_languages_and_returns_finite_candidates() {
    let detector = textintel::language::DefaultLanguageDetector;
    let capabilities = detector.capabilities();
    assert!(capabilities.local);
    assert!(capabilities.languages.iter().any(|v| v == "es"));
    for text in [
        "hola mundo",
        "hello world",
        "你好世界",
        "مرحبا بالعالم",
        "नमस्ते दुनिया",
    ] {
        let result = detector.detect(text).unwrap();
        assert!(!result.is_empty(), "{text}");
        assert!(
            result
                .iter()
                .all(|v| v.probability.is_finite() && (0.0..=1.0).contains(&v.probability))
        );
    }
    let unknown = detector.detect("").unwrap();
    assert_eq!(unknown.len(), 1);
    assert_eq!(unknown[0].language, "unknown");
    assert_eq!(unknown[0].probability, 1.0);
}
