//! Regression tests for fuzz-discovered hardening (see `fuzz/fuzz_targets/`).
//!
//! Every target's core property is pinned here so `cargo test` guards it:
//! hostile inputs produce errors or degraded scores, never panics or NaN.
//! Short live-fuzz runs (45–90s per new target, nightly libFuzzer) found no
//! crashes; these tests lock in the manual crash-hunting below plus the
//! properties the targets assert.

use textintel::comparison::scorer::score_fingerprints;
use textintel::core::types::MessageFingerprint;
use textintel::phonetic::ipa::parse_ipa;
use textintel::storage::migrate::migrate_fingerprint_bytes;
use textintel::{EngineConfig, ResourceLoader, SimilarityWeights, TextIntelligence};

fn assert_finite_comparison(left: &MessageFingerprint, right: &MessageFingerprint) {
    let result = score_fingerprints(left, right, &SimilarityWeights::default());
    assert!(
        result.score.is_finite(),
        "score must be finite, got {}",
        result.score
    );
    for channel in [
        result.semantic,
        result.lexical,
        result.character,
        result.visual,
        result.phonetic,
        result.symbolic,
        result.decoded_similarity,
        result.obfuscation_similarity,
    ] {
        assert!(
            channel.map(|value| value.is_finite()).unwrap_or(true),
            "channel must be finite or absent: {channel:?}"
        );
    }
}

#[test]
fn resource_parser_rejects_hostile_packs_without_panicking() {
    let hostile = [
        "",
        "{",
        "[[[[[[[[[[",
        "{\"schema_version\": \"one\"}",
        "{\"schema_version\": 999}",
        "{\"language\": \"\", \"entries\": []}",
        "{\"entries\": [{\"word\": 42}]}",
        "{\"entries\": [{\"word\": \"a\", \"weight\": \"heavy\"}]}",
        "{\"symbols\": [{\"token\": \"x\", \"readings\": \"nope\"}]}",
        "{\"symbols\": [{\"token\": \"x\", \"concepts\": [{\"id\": 7}]}]}",
        "{\"symbols\": [{\"token\": \"x\", \"readings\": [{\"text\": \"y\", \"probability\": -1.0}]}]}",
        "{\"symbols\": [{\"token\": \"\"}]}",
        "null",
        "[]",
        "\"just a string\"",
        "{\"language\": \"en\", \"entries\": [{\"word\": \"a\", \"weight\": 1e308}]}",
        // Nul bytes and replacement chars inside strings.
        "{\"language\": \"en\", \"entries\": [{\"word\": \"a\0b\"}]}",
        // Declared hash that does not match the payload.
        "{\"schema_version\": 1, \"sha256\": \"deadbeef\", \"language\": \"en\"}",
    ];
    for payload in hostile {
        let mut loader = ResourceLoader::with_limits(Default::default());
        // Every outcome is `Ok`/`Err` — the assertion is the absence of panic.
        let _ = loader.load_language_json(payload, std::path::PathBuf::from("<regression>"));
        let _ = loader.load_symbol_json(payload, std::path::PathBuf::from("<regression>"));
        let _ = loader.load_abbreviation_json(payload, std::path::PathBuf::from("<regression>"));
    }
    // Oversize payloads are rejected before parsing.
    let big = "x".repeat(1024);
    let mut loader = ResourceLoader::with_limits(textintel::ResourceLimits {
        max_resource_bytes: 16,
        ..Default::default()
    });
    assert!(loader
        .load_language_json(&big, std::path::PathBuf::from("<regression>"))
        .is_err());
}

#[test]
fn ipa_parser_is_total_over_hostile_unicode() {
    let long = "a".repeat(10_000);
    let hostile = [
        long.as_str(),
        "",
        " ",
        "ˈˌː",
        "\u{300}\u{301}\u{36f}",
        "t\u{361}ʃ",        // decomposed affricate
        "t͡ʃd͡ʒ",             // precomposed affricates
        "\u{200d}\u{fe0f}", // joiners outside IPA
        "a\u{ffff}b",       // noncharacter
        "\u{202e}abc",      // bidi override
        "ksɡɲʝaɪaʊeɪɔɪ",    // every multi-char token glued
        "x\u{0}y",          // nul char
    ];
    for input in hostile {
        let tokens = parse_ipa(input);
        assert!(
            tokens.iter().all(|token| !token.is_empty()),
            "empty token for {input:?}"
        );
        // Parsing is deterministic.
        assert_eq!(tokens, parse_ipa(input));
    }
}

#[test]
fn scorer_is_total_over_hostile_fingerprints() {
    let engine = TextIntelligence::new(EngineConfig::default());
    // Empty, whitespace, and zalgo inputs analyze and compare finitely.
    let hostile_texts = [
        String::new(),
        " ".to_string(),
        "\n\t ".to_string(),
        "a".to_string(),
        "e\u{301}\u{302}\u{303}\u{304}\u{305}".to_string(),
        "🏠".repeat(64),
        "4".repeat(128),
        "mixed Fr4🏠d0 \u{200b} text".to_string(),
    ];
    let mut fingerprints = Vec::new();
    for text in hostile_texts {
        let fingerprint = engine.analyze(&text).unwrap();
        fingerprints.push(fingerprint);
    }
    for left in &fingerprints {
        for right in &fingerprints {
            assert_finite_comparison(left, right);
        }
    }
    // Hostile vectors and confidences degrade to 0/absent, never NaN.
    let mut hostile = engine.analyze("hello").unwrap();
    hostile
        .semantic_embeddings
        .insert("default".to_string(), vec![f32::NAN, f32::INFINITY]);
    hostile
        .phonetic_candidates
        .iter_mut()
        .for_each(|candidate| {
            candidate.confidence = f64::NAN;
        });
    assert_finite_comparison(&hostile, &hostile);
    // Hostile weights are skipped, never NaN the total.
    let hostile_weights = SimilarityWeights {
        semantic: f64::NAN,
        lexical: f64::INFINITY,
        character: -1.0,
        visual: 0.0,
        phonetic: 0.0,
        symbolic: 0.0,
        decoded: 0.0,
        obfuscation: 0.0,
    };
    let plain = engine.analyze("hello").unwrap();
    let result = score_fingerprints(&plain, &plain, &hostile_weights);
    assert!(result.score.is_finite());
    // Cross-view matching is strictly cross-side: a transliteration view
    // matching its own raw text must not inflate decoded similarity.
    // (Regression test for the phase-3 same-side bug: 你好 vs 再见 scored
    // decoded=1.0 through a junk pass-through view.)
    let different = engine.compare("你好", "再见").unwrap();
    assert!(
        different.decoded_similarity.unwrap_or(1.0) < 0.5,
        "same-side view leak: {:?}",
        different.decoded_similarity
    );
}

#[test]
fn migration_rejects_hostile_payloads_without_panicking() {
    let hostile: &[&[u8]] = &[
        b"",
        b"{",
        b"null",
        b"[]",
        b"\"str\"",
        b"{\"schema_version\": \"2\"}",
        b"{\"schema_version\": -1}",
        b"{\"schema_version\": 0}",
        b"{\"schema_version\": 99}",
        b"{\"schema_version\": 18446744073709551615}",
        b"{\"schema_version\": 1}",
        b"{\"schema_version\": 2, \"raw\": 42}",
        b"{\"raw\": \"x\"}",
        &[0xff, 0xfe, 0x00, 0x01],
        &[b'{'; 100_000],
    ];
    for payload in hostile {
        // `Ok` or typed `Err` — never a panic.
        let _ = migrate_fingerprint_bytes(payload);
    }
    // Round-trip: an engine-built fingerprint migrates cleanly at rest.
    let engine = TextIntelligence::new(EngineConfig::default());
    let fingerprint = engine.analyze("Fra🏠do restart").unwrap();
    let bytes = serde_json::to_vec(&fingerprint).unwrap();
    let migrated = migrate_fingerprint_bytes(&bytes).unwrap();
    assert_eq!(migrated.migrated_from, None);
    assert_eq!(migrated.fingerprint.raw, "Fra🏠do restart");
}

#[test]
fn unicode_and_rebus_fuzz_properties_hold() {
    // Mirrors of the long-standing unicode/tokenization/rebus targets.
    let engine = TextIntelligence::new(EngineConfig::default());
    let long = "x".repeat(2048);
    for text in [
        "",
        "🏠",
        "\u{200d}",
        "e\u{301}",
        "Fra🏠do",
        "a\u{0}b",
        long.as_str(),
    ] {
        let analyzed = engine.analyze(text).unwrap();
        assert_eq!(analyzed.raw, text, "raw must be preserved");
        let _ = engine.decode(text).unwrap();
    }
}
