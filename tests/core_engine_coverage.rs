//! Slice 1/3 full-coverage tests: core engine entry points (`analyze`,
//! `compare`, `decode`, batch variants), normalization, lexical, and
//! `core::{config, error, types}`.
//!
//! Sibling slices cover phonetic/semantic/rebus/symbols/transliteration and
//! detection/storage/resources/cache/comparison.

use std::collections::{BTreeMap, BTreeSet};

use textintel::lexical::character::{
    char_features, character_similarity, combined_character_similarity, damerau_levenshtein, jaro,
    jaro_winkler, lcs_len, lcs_similarity, levenshtein, ngram_similarity,
};
use textintel::lexical::minhash::{
    minhash_from_text, minhash_signature, minhash_similarity, simhash,
};
use textintel::lexical::ngrams::{character_ngrams, word_ngrams};
use textintel::lexical::similarity::{
    jaccard, lexical_similarity, term_frequency, tfidf_similarity,
};
use textintel::lexical::tokenizer::{is_emoji, simple_lemmas, stop_words, tokenize};
use textintel::normalization::confusables::{confusable_map, skeleton};
use textintel::normalization::leetspeak::{apply_leet, detect_leet, leet_map};
use textintel::normalization::repetition::{collapse_repetition, repetition_ratio};
use textintel::normalization::unicode::{casefold_text, nfc, nfkc, strip_diacritics};
use textintel::normalization::whitespace::{is_extra_whitespace, normalize_whitespace};
use textintel::{
    CacheLimits, ChannelAvailability, DecodedCandidate, DuplicateMode, EngineConfig,
    LanguageCandidate, ProviderError, RebusWeights, SimilarityWeights, StageTimings, SymbolConcept,
    SymbolReading, TextIntelError, TextIntelligence, Transformation, API_VERSION,
    FINGERPRINT_SCHEMA_VERSION,
};

// ---------------------------------------------------------------------------
// Engine: analyze
// ---------------------------------------------------------------------------

#[test]
fn analyze_preserves_raw_and_populates_channels() {
    let engine = TextIntelligence::default();
    let raw = "Fra🏠do";
    let fp = engine.analyze(raw).unwrap();
    assert_eq!(fp.raw, raw);
    assert_eq!(fp.schema_version, FINGERPRINT_SCHEMA_VERSION);
    assert_eq!(fp.char_features.length, raw.chars().count());
    assert_eq!(fp.tokens, tokenize(raw));
    assert!(!fp.language_candidates.is_empty());
    assert!(!fp.segments.is_empty());
    assert!(!fp.unicode_features.nfc.is_empty());
    assert!(!fp.decoded_texts().collect::<Vec<_>>().is_empty());
    assert!(fp.top_language().is_some());
    // Model-free default: no embedding or phonetic providers engaged.
    assert!(fp.semantic_embeddings.is_empty());
    assert!(fp.phonetic_candidates.is_empty());
}

#[test]
fn analyze_accepts_empty_input() {
    let fp = TextIntelligence::default().analyze("").unwrap();
    assert_eq!(fp.raw, "");
    assert_eq!(fp.char_features.length, 0);
    assert!(fp.tokens.is_empty());
}

#[test]
fn analyze_rejects_input_over_char_limit() {
    let engine = TextIntelligence::new(EngineConfig {
        max_input_length: 3,
        ..Default::default()
    });
    // Boundary: exactly at the limit passes.
    assert!(engine.analyze("abc").is_ok());
    let err = engine.analyze("abcd").unwrap_err();
    match err {
        TextIntelError::InputTooLong { length, maximum } => {
            assert_eq!((length, maximum), (4, 3));
        }
        other => panic!("expected InputTooLong, got {other:?}"),
    }
}

#[test]
fn analyze_counts_multibyte_chars_as_single_units() {
    let engine = TextIntelligence::new(EngineConfig {
        max_input_length: 2,
        ..Default::default()
    });
    // Three chars in six bytes: the limit counts chars, not bytes.
    let err = engine.analyze("ééé").unwrap_err();
    match err {
        TextIntelError::InputTooLong { length, maximum } => {
            assert_eq!((length, maximum), (3, 2));
        }
        other => panic!("expected InputTooLong, got {other:?}"),
    }
    assert!(engine.analyze("éé").is_ok());
}

#[test]
fn analyze_with_tiny_segment_budget_truncates_without_error() {
    let engine = TextIntelligence::new(EngineConfig {
        max_segments: 1,
        ..Default::default()
    });
    let fp = engine.analyze("hello world foo bar").unwrap();
    assert!(fp.segments.len() <= 1, "segments: {:?}", fp.segments);
}

#[test]
fn analyze_with_timing_reports_nonnegative_stage_timings() {
    let engine = TextIntelligence::default();
    let (fp, timings) = engine.analyze_with_timing("hello world").unwrap();
    assert_eq!(fp.raw, "hello world");
    for (stage, micros) in timings.as_map() {
        assert!(
            micros >= 0.0,
            "stage {stage} reported negative timing {micros}"
        );
    }
    assert_eq!(timings.as_map().len(), StageTimings::stage_names().len());
}

#[test]
fn try_new_rejects_invalid_config_and_new_panics() {
    let bad = EngineConfig {
        max_input_length: 0,
        ..Default::default()
    };
    let err = TextIntelligence::try_new(bad).err().expect("must fail");
    assert!(
        matches!(err, TextIntelError::InvalidConfiguration(_)),
        "got {err:?}"
    );
}

#[test]
fn provider_errors_chain_through_source() {
    use std::error::Error;
    let err = TextIntelError::from(ProviderError::new("embedding", "boom"));
    let source = err.source().expect("provider chain must expose source");
    assert_eq!(source.to_string(), "embedding provider failed: boom");
    assert!(TextIntelError::Storage("x".to_string()).source().is_none());
}

#[test]
#[should_panic(expected = "invalid TextIntelligence configuration")]
fn new_panics_on_invalid_config() {
    let _ = TextIntelligence::new(EngineConfig {
        beam_width: 0,
        ..Default::default()
    });
}

#[test]
fn engine_accessors_report_config_capabilities_and_diagnostics() {
    let engine = TextIntelligence::default();
    assert_eq!(engine.config().max_input_length, 8_192);
    assert!(!engine.provider_capabilities().is_empty());
    assert!(engine.health_check().is_ok());
    let diagnostics = engine.diagnostics();
    assert_eq!(diagnostics.api_version.as_str(), API_VERSION);
    // Default engine ships no resource packs; the manifest call still works.
    let _ = engine.resource_manifest();
}

// ---------------------------------------------------------------------------
// Engine: batch analyze / compare
// ---------------------------------------------------------------------------

#[test]
fn analyze_batch_preserves_order_and_accepts_empty_batch() {
    let engine = TextIntelligence::default();
    let empty: Vec<String> = Vec::new();
    assert!(engine.analyze_batch(&empty).unwrap().is_empty());

    let texts = vec![
        "first".to_string(),
        "second message".to_string(),
        "".to_string(),
    ];
    let fps = engine.analyze_batch(&texts).unwrap();
    assert_eq!(fps.len(), 3);
    for (fp, raw) in fps.iter().zip(&texts) {
        assert_eq!(&fp.raw, raw);
    }
}

#[test]
fn analyze_batch_rejects_oversized_batch() {
    let engine = TextIntelligence::new(EngineConfig {
        max_batch_size: 2,
        ..Default::default()
    });
    let texts = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let err = engine.analyze_batch(&texts).unwrap_err();
    match err {
        TextIntelError::InvalidConfiguration(message) => {
            assert!(message.contains("max_batch_size"), "message: {message}");
        }
        other => panic!("expected InvalidConfiguration, got {other:?}"),
    }
}

#[test]
fn analyze_batch_propagates_per_item_length_errors() {
    let engine = TextIntelligence::new(EngineConfig {
        max_input_length: 2,
        ..Default::default()
    });
    let texts = vec!["ok".to_string(), "too long".to_string()];
    assert!(matches!(
        engine.analyze_batch(&texts).unwrap_err(),
        TextIntelError::InputTooLong { .. }
    ));
}

#[test]
fn compare_batch_matches_pairwise_compare() {
    let engine = TextIntelligence::default();
    let pairs = vec![
        ("hello".to_string(), "hello".to_string()),
        ("hello".to_string(), "goodbye".to_string()),
    ];
    let batched = engine.compare_batch(&pairs).unwrap();
    assert_eq!(batched.len(), 2);
    for ((left, right), result) in pairs.iter().zip(&batched) {
        let single = engine.compare(left, right).unwrap();
        assert!((result.score - single.score).abs() < f64::EPSILON);
    }
    assert!(engine.compare_batch(&[]).unwrap().is_empty());
}

#[test]
fn compare_batch_rejects_oversized_batch() {
    let engine = TextIntelligence::new(EngineConfig {
        max_batch_size: 1,
        ..Default::default()
    });
    let pairs = vec![
        ("a".to_string(), "a".to_string()),
        ("b".to_string(), "b".to_string()),
    ];
    assert!(matches!(
        engine.compare_batch(&pairs).unwrap_err(),
        TextIntelError::InvalidConfiguration(_)
    ));
}

// ---------------------------------------------------------------------------
// Engine: compare
// ---------------------------------------------------------------------------

#[test]
fn compare_identical_texts_scores_one() {
    let engine = TextIntelligence::default();
    let result = engine.compare("hello world", "hello world").unwrap();
    assert_eq!(result.character, Some(1.0));
    assert_eq!(result.lexical, Some(1.0));
    assert!(result.score > 0.99, "score: {}", result.score);
    // Model-free default leaves provider channels empty.
    assert_eq!(result.semantic, None);
    assert_eq!(result.phonetic, None);
}

#[test]
fn compare_orders_identical_above_paraphrase_above_unrelated() {
    let engine = TextIntelligence::default();
    let identical = engine
        .compare("buy now cheap", "buy now cheap")
        .unwrap()
        .score;
    let overlap = engine
        .compare("buy now cheap", "buy now fast")
        .unwrap()
        .score;
    let unrelated = engine
        .compare("buy now cheap", "quantum photosynthesis")
        .unwrap()
        .score;
    assert!(identical > overlap, "{identical} vs {overlap}");
    assert!(overlap > unrelated, "{overlap} vs {unrelated}");
}

#[test]
fn compare_is_case_and_accent_insensitive_on_character_channel() {
    let engine = TextIntelligence::default();
    let result = engine.compare("COMPRA AHORA", "compra ahora").unwrap();
    assert!(result.character.unwrap() > 0.7);
    let accented = engine.compare("música", "musica").unwrap();
    assert!(accented.character.unwrap() > 0.99);
}

#[test]
fn compare_propagates_length_errors_from_either_side() {
    let engine = TextIntelligence::new(EngineConfig {
        max_input_length: 3,
        ..Default::default()
    });
    assert!(matches!(
        engine.compare("toolong", "ok").unwrap_err(),
        TextIntelError::InputTooLong { .. }
    ));
    assert!(matches!(
        engine.compare("ok", "toolong").unwrap_err(),
        TextIntelError::InputTooLong { .. }
    ));
}

#[test]
fn compare_with_timing_matches_compare_and_reports_comparison_stage() {
    let engine = TextIntelligence::default();
    let (timed, timings) = engine.compare_with_timing("hello", "hallo").unwrap();
    let plain = engine.compare("hello", "hallo").unwrap();
    assert!((timed.score - plain.score).abs() < f64::EPSILON);
    assert!(timings.comparison_micros >= 0.0);
    assert!(timings.total_micros >= 0.0);
    // Both analyze stages are summed into the compare timings.
    assert!(timings.normalization_micros >= 0.0);
}

#[test]
fn compare_fingerprints_agrees_with_compare() {
    let engine = TextIntelligence::default();
    let left = engine.analyze("hello world").unwrap();
    let right = engine.analyze("hello there").unwrap();
    let via_fps = engine.compare_fingerprints(&left, &right);
    let direct = engine.compare("hello world", "hello there").unwrap();
    assert!((via_fps.score - direct.score).abs() < f64::EPSILON);
    assert_eq!(via_fps.character, direct.character);
}

// ---------------------------------------------------------------------------
// Engine: decode
// ---------------------------------------------------------------------------

#[test]
fn decode_recovers_leet_and_symbol_readings() {
    let engine = TextIntelligence::default();
    let candidates = engine.decode("h3llo").unwrap();
    assert!(
        candidates.iter().any(|c| c.text == "hello"),
        "candidates: {:?}",
        candidates.iter().map(|c| &c.text).collect::<Vec<_>>()
    );
    let candidates = engine.decode("salU2").unwrap();
    assert!(candidates
        .iter()
        .any(|c| c.text.eq_ignore_ascii_case("saludos")));
}

#[test]
fn decode_with_languages_respects_candidate_limit() {
    let engine = TextIntelligence::default();
    let limited = engine
        .decode_with_languages("salU2", None, Some(2))
        .unwrap();
    assert!(limited.len() <= 2 && !limited.is_empty());
    // A zero limit clamps to one candidate rather than returning nothing.
    let zero = engine
        .decode_with_languages("salU2", None, Some(0))
        .unwrap();
    assert_eq!(zero.len(), 1);
}

#[test]
fn decode_with_explicit_languages_runs_language_pinned() {
    let engine = TextIntelligence::default();
    let languages = vec!["en".to_string()];
    let candidates = engine
        .decode_with_languages("h3llo", Some(&languages), None)
        .unwrap();
    assert!(!candidates.is_empty());
    let empty: Vec<String> = Vec::new();
    assert!(!engine
        .decode_with_languages("h3llo", Some(&empty), None)
        .unwrap()
        .is_empty());
}

#[test]
fn decode_rejects_overlong_input() {
    let engine = TextIntelligence::new(EngineConfig {
        max_input_length: 2,
        ..Default::default()
    });
    assert!(matches!(
        engine.decode("toolong").unwrap_err(),
        TextIntelError::InputTooLong { .. }
    ));
    assert!(matches!(
        engine
            .decode_with_languages("toolong", None, None)
            .unwrap_err(),
        TextIntelError::InputTooLong { .. }
    ));
}

// ---------------------------------------------------------------------------
// Normalization: unicode
// ---------------------------------------------------------------------------

#[test]
fn nfc_and_nfkc_normalize_composed_and_compatibility_forms() {
    assert_eq!(nfc("e\u{301}"), "é");
    assert_eq!(nfc(""), "");
    // Ligature and full-width forms fold under NFKC only.
    assert_eq!(nfkc("ﬁ"), "fi");
    assert_eq!(nfkc("ａｂｃ"), "abc");
    assert_eq!(nfkc("2²"), "22");
    assert_eq!(nfkc(""), "");
}

#[test]
fn casefold_handles_special_cased_scalars() {
    assert_eq!(casefold_text("ß"), "ss");
    assert_eq!(casefold_text("ẞ"), "ss");
    assert_eq!(casefold_text("ς"), "σ");
    assert_eq!(casefold_text("ſ"), "s");
    assert_eq!(casefold_text("İ"), "i\u{307}");
    assert_eq!(casefold_text("ABC XYZ"), "abc xyz");
    assert_eq!(casefold_text(""), "");
    // Casefolding applies NFKC first.
    assert_eq!(casefold_text("Ａ"), "a");
}

#[test]
fn strip_diacritics_folds_accents_and_preserves_strokes_and_script_marks() {
    assert_eq!(strip_diacritics("música"), "musica");
    assert_eq!(strip_diacritics("niño"), "nino");
    assert_eq!(strip_diacritics(""), "");
    assert_eq!(strip_diacritics("plain"), "plain");
    // Precomposed stroke letters survive; only the ń folds.
    assert_eq!(strip_diacritics("łódź"), "łodz");
    // Arabic harakat carry lexical weight and survive.
    assert_eq!(strip_diacritics("مَدرسة"), "مَدرسة");
    // Combining dot from casefolded İ completes the fold.
    assert_eq!(strip_diacritics(&casefold_text("İ")), "i");
}

// ---------------------------------------------------------------------------
// Normalization: confusables
// ---------------------------------------------------------------------------

#[test]
fn confusable_map_covers_cyrillic_greek_and_fullwidth_ranges() {
    let map = confusable_map();
    assert_eq!(map.get(&'а'), Some(&'a')); // Cyrillic small a
    assert_eq!(map.get(&'А'), Some(&'A')); // Cyrillic capital a
    assert_eq!(map.get(&'Α'), Some(&'A')); // Greek capital alpha
    assert_eq!(map.get(&'０'), Some(&'0')); // Fullwidth digit
    assert_eq!(map.get(&'ａ'), Some(&'a')); // Fullwidth a
    assert!(!map.contains_key(&'q'));
}

#[test]
fn skeleton_folds_mixed_script_spoofing_to_ascii() {
    assert_eq!(skeleton("pаypal"), "paypal"); // Cyrillic а
    assert_eq!(skeleton("ABC"), "abc"); // casefold applies first
    assert_eq!(skeleton("Р"), "p"); // Cyrillic ER lowercases then maps
    assert_eq!(skeleton(""), "");
    assert_eq!(skeleton("hello"), "hello");
}

// ---------------------------------------------------------------------------
// Normalization: leetspeak
// ---------------------------------------------------------------------------

#[test]
fn leet_map_lists_digits_before_symbol_aliases() {
    let map = leet_map();
    assert_eq!(map.get(&'0'), Some(&vec!["o", "0"]));
    assert_eq!(map.get(&'4'), Some(&vec!["a", "4"]));
    assert_eq!(map.get(&'@'), Some(&vec!["a"]));
    assert_eq!(map.get(&'$'), Some(&vec!["s"]));
    assert!(!map.contains_key(&'z'));
}

#[test]
fn detect_leet_requires_digit_adjacent_to_letter() {
    assert!(detect_leet("c0mpr4 ah0r4"));
    assert!(detect_leet("h3llo"));
    assert!(detect_leet("a1"));
    assert!(detect_leet("1a"));
    assert!(!detect_leet("123")); // bare numbers are not leet
    assert!(!detect_leet("abc")); // no digits at all
    assert!(!detect_leet("")); // empty edge
    assert!(!detect_leet("@")); // symbols map but never trigger detection
    assert!(!detect_leet("1 2")); // digits without letter neighbors
}

#[test]
fn apply_leet_greedily_takes_first_reading() {
    assert_eq!(apply_leet("h3llo"), "hello");
    assert_eq!(apply_leet("c0mpr4"), "compra");
    assert_eq!(apply_leet("@$!"), "asi");
    assert_eq!(apply_leet("plain"), "plain");
    assert_eq!(apply_leet(""), "");
    // Digits map to letters even without letter neighbors (greedy view).
    assert_eq!(apply_leet("123"), "ize");
}

// ---------------------------------------------------------------------------
// Normalization: repetition
// ---------------------------------------------------------------------------

#[test]
fn collapse_repetition_keeps_short_runs_and_collapses_long_runs() {
    assert_eq!(collapse_repetition("helloooo", 1), "hello");
    assert_eq!(collapse_repetition("helloooo", 2), "helloo");
    assert_eq!(collapse_repetition("aa", 1), "aa"); // run < 3 untouched
    assert_eq!(collapse_repetition("", 1), "");
    assert_eq!(collapse_repetition("abc", 1), "abc");
    assert_eq!(collapse_repetition("üüüü", 1), "ü"); // unicode run
}

#[test]
fn collapse_repetition_only_touches_alphanumeric_and_emphatic_marks() {
    assert_eq!(collapse_repetition("!!!", 1), "!");
    assert_eq!(collapse_repetition("???", 2), "??");
    assert_eq!(collapse_repetition("...", 1), ".");
    assert_eq!(collapse_repetition("   ", 1), "   "); // spaces untouched
    assert_eq!(collapse_repetition("---", 1), "---"); // other punct untouched
}

#[test]
fn collapse_repetition_clamps_zero_keep_to_one() {
    assert_eq!(collapse_repetition("helloooo", 0), "hello");
}

#[test]
fn repetition_ratio_is_zero_for_clean_and_empty_text() {
    assert_eq!(repetition_ratio(""), 0.0);
    assert_eq!(repetition_ratio("hello"), 0.0);
    let ratio = repetition_ratio("helloooo");
    assert!((ratio - 3.0 / 8.0).abs() < 1e-12, "ratio: {ratio}");
    assert!((0.0..=1.0).contains(&repetition_ratio("aaaaabbbbb")));
}

// ---------------------------------------------------------------------------
// Normalization: whitespace
// ---------------------------------------------------------------------------

#[test]
fn normalize_whitespace_collapses_trims_and_folds_unicode_spaces() {
    assert_eq!(normalize_whitespace("  a  b  "), "a b");
    assert_eq!(normalize_whitespace("a\u{00a0}b"), "a b");
    assert_eq!(normalize_whitespace("a\u{2003}b"), "a b"); // em space
    assert_eq!(normalize_whitespace("a\u{200b}b"), "a b"); // zero-width space
    assert_eq!(normalize_whitespace("a\t\nb"), "a b");
    assert_eq!(normalize_whitespace(""), "");
    assert_eq!(normalize_whitespace("   "), "");
    assert_eq!(normalize_whitespace("a"), "a");
}

#[test]
fn is_extra_whitespace_matches_nonstandard_spaces_only() {
    assert!(is_extra_whitespace('\u{00a0}'));
    assert!(is_extra_whitespace('\u{200b}'));
    assert!(is_extra_whitespace('\u{3000}'));
    assert!(!is_extra_whitespace(' ')); // plain space via char::is_whitespace
    assert!(!is_extra_whitespace('a'));
}

// ---------------------------------------------------------------------------
// Lexical: character metrics
// ---------------------------------------------------------------------------

#[test]
fn char_features_counts_categories_and_ngrams() {
    let features = char_features("a1! ");
    assert_eq!(features.length, 4);
    assert_eq!(features.letters, 1);
    assert_eq!(features.digits, 1);
    assert_eq!(features.punctuation, 1);
    assert_eq!(features.whitespace, 1);
    assert_eq!(features.other, 0);

    let features = char_features("ab");
    assert_eq!(features.ngrams_2.get("ab"), Some(&1));
    assert!(features.ngrams_3.is_empty());

    let empty = char_features("");
    assert_eq!(empty.length, 0);
    assert!(empty.ngrams_2.is_empty());
}

#[test]
fn levenshtein_matches_known_distances_and_edges() {
    assert_eq!(levenshtein("kitten", "sitting"), 3);
    assert_eq!(levenshtein("", "ab"), 2);
    assert_eq!(levenshtein("ab", ""), 2);
    assert_eq!(levenshtein("", ""), 0);
    assert_eq!(levenshtein("same", "same"), 0);
    assert_eq!(levenshtein("café", "cafe"), 1); // char-based, not byte-based
}

#[test]
fn damerau_counts_transposition_as_single_edit() {
    assert_eq!(damerau_levenshtein("abcd", "acbd"), 1);
    assert_eq!(levenshtein("abcd", "acbd"), 2);
    assert_eq!(damerau_levenshtein("same", "same"), 0);
    assert_eq!(damerau_levenshtein("", "abc"), 3);
    assert_eq!(damerau_levenshtein("abc", ""), 3);
}

#[test]
fn jaro_and_jaro_winkler_cover_identity_empty_and_mismatch() {
    assert_eq!(jaro("abc", "abc"), 1.0);
    assert_eq!(jaro("", "abc"), 0.0);
    assert_eq!(jaro("abc", ""), 0.0);
    assert_eq!(jaro("abc", "xyz"), 0.0);
    assert_eq!(jaro_winkler("abc", "abc"), 1.0);
    // Shared prefix boosts winkler above jaro.
    let (jw, j) = (jaro_winkler("martha", "marhta"), jaro("martha", "marhta"));
    assert!(jw >= j && j > 0.0 && jw <= 1.0, "jw={jw} j={j}");
}

#[test]
fn ngram_similarity_covers_identity_empty_and_partial_overlap() {
    assert_eq!(ngram_similarity("abc", "abc", 2), 1.0);
    assert_eq!(ngram_similarity("", "", 2), 1.0); // both empty, equal inputs
    assert_eq!(ngram_similarity("", "a", 2), 0.0); // both empty, unequal
    assert_eq!(ngram_similarity("abc", "xyz", 2), 0.0);
    let partial = ngram_similarity("abcd", "abce", 2);
    assert!(partial > 0.0 && partial < 1.0, "partial: {partial}");
}

#[test]
fn lcs_covers_known_subsequence_and_empty_edges() {
    assert_eq!(lcs_len("abcde", "ace"), 3);
    assert_eq!(lcs_len("", "abc"), 0);
    assert_eq!(lcs_len("abc", ""), 0);
    assert_eq!(lcs_similarity("abc", "abc"), 1.0);
    assert_eq!(lcs_similarity("", ""), 0.0); // max(0,0).max(1) denominator
    assert_eq!(lcs_similarity("abc", "xyz"), 0.0);
}

#[test]
fn character_similarity_is_bounded_case_and_accent_insensitive() {
    let identical = character_similarity("hello", "hello");
    assert_eq!(identical.combined, 1.0);
    assert_eq!(identical.levenshtein, 1.0);
    assert_eq!(combined_character_similarity("hello", "hello"), 1.0);

    for (a, b) in [("Hello", "hello"), ("música", "musica"), ("Paris", "París")] {
        let score = character_similarity(a, b);
        assert!(score.combined > 0.99, "{a} vs {b}: {}", score.combined);
    }
    // Cross-script pairs stay below identity.
    assert!(character_similarity("paypal", "pаypal").combined < 1.0);
    // Every channel stays in [0, 1].
    let score = character_similarity("kitten", "sitting");
    for value in [
        score.levenshtein,
        score.damerau_levenshtein,
        score.jaro,
        score.jaro_winkler,
        score.ngram_similarity,
        score.lcs,
        score.combined,
    ] {
        assert!((0.0..=1.0).contains(&value), "value: {value}");
    }
}

// ---------------------------------------------------------------------------
// Lexical: similarity, ngrams, minhash, tokenizer
// ---------------------------------------------------------------------------

#[test]
fn jaccard_covers_empty_and_partial_sets() {
    let empty: BTreeSet<String> = BTreeSet::new();
    assert_eq!(jaccard(&empty, &empty), 1.0);
    let one: BTreeSet<String> = ["a".to_string()].into_iter().collect();
    assert_eq!(jaccard(&one, &empty), 0.0);
    assert_eq!(jaccard(&empty, &one), 0.0);
    let two: BTreeSet<String> = ["a".to_string(), "b".to_string()].into_iter().collect();
    assert!((jaccard(&one, &two) - 0.5).abs() < 1e-12);
}

#[test]
fn lexical_similarity_matches_identical_and_rejects_disjoint() {
    assert_eq!(lexical_similarity("gana dinero", "gana dinero"), 1.0);
    assert_eq!(
        lexical_similarity("feliz cumpleaños", "feliz cumpleanos"),
        1.0
    );
    assert!(lexical_similarity("gana dinero", "xyz") < 0.2);
}

#[test]
fn term_frequency_normalizes_and_handles_empty() {
    assert!(term_frequency(&[]).is_empty());
    let tf = term_frequency(&["a".to_string(), "a".to_string(), "b".to_string()]);
    assert!((tf["a"] - 2.0 / 3.0).abs() < 1e-12);
    assert!((tf["b"] - 1.0 / 3.0).abs() < 1e-12);
    assert!((tf.values().sum::<f64>() - 1.0).abs() < 1e-12);
}

#[test]
fn tfidf_similarity_covers_identity_disjoint_and_empty() {
    let doc = vec!["hello".to_string(), "world".to_string()];
    assert!((tfidf_similarity(&doc, &doc) - 1.0).abs() < 1e-12);
    let other = vec!["quantum".to_string(), "physics".to_string()];
    assert_eq!(tfidf_similarity(&doc, &other), 0.0);
    let empty: Vec<String> = Vec::new();
    assert_eq!(tfidf_similarity(&empty, &doc), 0.0);
    assert_eq!(tfidf_similarity(&empty, &empty), 0.0);
}

#[test]
fn word_ngrams_covers_unit_short_and_window_cases() {
    let tokens = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    assert_eq!(word_ngrams(&tokens, 1), tokens);
    assert_eq!(word_ngrams(&tokens, 0), tokens);
    assert_eq!(word_ngrams(&tokens, 2), vec!["a b", "b c"]);
    assert!(word_ngrams(&tokens, 5).is_empty());
    assert!(word_ngrams(&[], 2).is_empty());
}

#[test]
fn character_ngrams_covers_zero_short_and_window_cases() {
    assert!(character_ngrams("abc", 0).is_empty());
    assert!(character_ngrams("a", 2).is_empty());
    assert!(character_ngrams("", 2).is_empty());
    assert_eq!(character_ngrams("abc", 2), vec!["ab", "bc"]);
    assert_eq!(character_ngrams("abc", 3), vec!["abc"]);
}

#[test]
fn simhash_is_deterministic_with_empty_and_clamped_edges() {
    let tokens = vec!["hello".to_string(), "world".to_string()];
    assert_eq!(simhash(&tokens, 64), simhash(&tokens, 64));
    assert_eq!(simhash(&[], 64), 0);
    // Bit width clamps into [1, 64]; zero width yields a single-bit hash.
    assert!(simhash(&tokens, 0) <= 1);
    assert_eq!(simhash(&tokens, 1_000), simhash(&tokens, 64));
}

#[test]
fn minhash_signature_covers_zero_k_empty_and_determinism() {
    let tokens = vec!["hello".to_string(), "world".to_string()];
    assert!(minhash_signature(&tokens, 0).is_empty());
    assert_eq!(minhash_signature(&[], 4), vec![u32::MAX; 4]);
    assert_eq!(minhash_signature(&tokens, 8).len(), 8);
    assert_eq!(minhash_signature(&tokens, 8), minhash_signature(&tokens, 8));
    assert_eq!(minhash_from_text("hello world", 8).len(), 8);
}

#[test]
fn minhash_similarity_covers_identity_mismatch_and_empty() {
    let signature = minhash_signature(&["a".to_string()], 8);
    assert_eq!(minhash_similarity(&signature, &signature), 1.0);
    assert_eq!(minhash_similarity(&signature, &signature[..4]), 0.0);
    assert_eq!(minhash_similarity(&[], &signature), 0.0);
    assert_eq!(minhash_similarity(&signature, &[]), 0.0);
}

#[test]
fn tokenize_preserves_words_emoji_and_urls() {
    let tokens = tokenize("bro compra NOW");
    for expected in ["bro", "compra", "NOW"] {
        assert!(tokens.iter().any(|t| t == expected), "tokens: {tokens:?}");
    }
    assert!(tokenize("").is_empty());
    assert!(tokenize("hi 👋").iter().any(|t| t.contains('👋')));
    assert!(tokenize("see https://example.test/x")
        .iter()
        .any(|t| t.contains("example.test")));
}

#[test]
fn simple_lemmas_lowercases_and_strips_suffixes_on_long_words() {
    assert_eq!(simple_lemmas(&["HELLO".to_string()]), vec!["hello"]);
    assert_eq!(simple_lemmas(&["zzrunning".to_string()]), vec!["zzrunn"]);
    // Short alphabetic tokens keep their suffix.
    assert_eq!(simple_lemmas(&["runs".to_string()]), vec!["runs"]);
    // Non-alphabetic tokens are only lowercased.
    assert_eq!(simple_lemmas(&["ABC123".to_string()]), vec!["abc123"]);
    assert!(simple_lemmas(&[]).is_empty());
}

#[test]
fn stop_words_and_emoji_helpers_behave() {
    // Embedded default lexicon ships stop words; matching lowercases first.
    assert_eq!(stop_words(&["the".to_string()]), vec!["the"]);
    assert_eq!(stop_words(&["THE".to_string()]), vec!["the"]);
    assert!(stop_words(&["zxqv".to_string()]).is_empty());
    assert!(stop_words(&[]).is_empty());
    assert!(is_emoji('👋'));
    assert!(!is_emoji('a'));
}

// ---------------------------------------------------------------------------
// Core: config
// ---------------------------------------------------------------------------

#[test]
fn engine_config_default_validates() {
    assert!(EngineConfig::default().validate().is_ok());
    assert!(TextIntelligence::try_new(EngineConfig::default()).is_ok());
}

#[test]
fn engine_config_rejects_zero_limits() {
    let base = EngineConfig {
        max_input_length: 0,
        ..EngineConfig::default()
    };
    assert!(base.validate().is_err());

    for field in [
        "max_segments",
        "beam_width",
        "max_candidates",
        "max_symbol_readings",
        "max_recursion",
        "max_documents",
        "repetition_keep",
        "max_batch_size",
        "max_search_candidates",
        "max_decoded_branches",
    ] {
        let mut config = EngineConfig::default();
        match field {
            "max_segments" => config.max_segments = 0,
            "beam_width" => config.beam_width = 0,
            "max_candidates" => config.max_candidates = 0,
            "max_symbol_readings" => config.max_symbol_readings = 0,
            "max_recursion" => config.max_recursion = 0,
            "max_documents" => config.max_documents = 0,
            "repetition_keep" => config.repetition_keep = 0,
            "max_batch_size" => config.max_batch_size = 0,
            "max_search_candidates" => config.max_search_candidates = 0,
            "max_decoded_branches" => config.max_decoded_branches = 0,
            _ => unreachable!(),
        }
        assert!(config.validate().is_err(), "{field} = 0 must fail");
    }
}

#[test]
fn engine_config_validates_confidence_gap_and_language_hints() {
    for gap in [0.0, 1.0, 0.15] {
        let config = EngineConfig {
            strong_confidence_gap: gap,
            ..EngineConfig::default()
        };
        assert!(config.validate().is_ok(), "gap {gap} must pass");
    }
    for gap in [-0.1, 1.5, f64::NAN, f64::INFINITY] {
        let config = EngineConfig {
            strong_confidence_gap: gap,
            ..EngineConfig::default()
        };
        assert!(config.validate().is_err(), "gap {gap} must fail");
    }
    for hints in [vec!["".to_string()], vec!["   ".to_string()]] {
        let config = EngineConfig {
            language_hints: hints,
            ..EngineConfig::default()
        };
        assert!(config.validate().is_err());
    }
    let config = EngineConfig {
        language_hints: vec!["en".to_string(), "es".to_string()],
        ..EngineConfig::default()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn similarity_weights_validate_bounds_and_require_positive_total() {
    assert!(SimilarityWeights::default().validate().is_ok());
    assert_eq!(SimilarityWeights::default().as_map().len(), 8);

    let negative = SimilarityWeights {
        lexical: -0.1,
        ..SimilarityWeights::default()
    };
    assert!(negative.validate().is_err());

    let nan = SimilarityWeights {
        semantic: f64::NAN,
        ..SimilarityWeights::default()
    };
    assert!(nan.validate().is_err());

    let infinite = SimilarityWeights {
        visual: f64::INFINITY,
        ..SimilarityWeights::default()
    };
    assert!(infinite.validate().is_err());

    let zero = SimilarityWeights {
        semantic: 0.0,
        lexical: 0.0,
        character: 0.0,
        visual: 0.0,
        phonetic: 0.0,
        symbolic: 0.0,
        decoded: 0.0,
        obfuscation: 0.0,
    };
    assert!(zero.validate().is_err());
}

#[test]
fn rebus_weights_validate_every_clause() {
    assert!(RebusWeights::default().validate().is_ok());
    assert!(RebusWeights::default().channel_sum() > 0.0);

    let scale = RebusWeights {
        frequency_scale: 0.0,
        ..RebusWeights::default()
    };
    assert!(scale.validate().is_err());

    let cap = RebusWeights {
        transformation_penalty_cap: 1.0,
        ..RebusWeights::default()
    };
    assert!(cap.validate().is_err());

    let semantic = RebusWeights {
        semantic: 1.5,
        ..RebusWeights::default()
    };
    assert!(semantic.validate().is_err());

    let switch = RebusWeights {
        language_switch_penalty: 1.0,
        ..RebusWeights::default()
    };
    assert!(switch.validate().is_err());

    for discount in [0.0, -0.5, 1.5, f64::NAN] {
        let weights = RebusWeights {
            in_word_digit_discount: discount,
            ..RebusWeights::default()
        };
        assert!(weights.validate().is_err(), "discount {discount} must fail");
    }

    let channels = RebusWeights {
        lexical: 0.0,
        phonetic: 0.0,
        symbol: 0.0,
        language: 0.0,
        context: 0.0,
        ..RebusWeights::default()
    };
    assert!(channels.validate().is_err());

    let cost = RebusWeights {
        cost_symbol: -1.0,
        ..RebusWeights::default()
    };
    assert!(cost.validate().is_err());
}

#[test]
fn rebus_weights_json_roundtrip_and_reject_bad_payloads() {
    let weights = RebusWeights::default();
    let json = weights.to_json().unwrap();
    let parsed = RebusWeights::from_json(&json).unwrap();
    assert_eq!(parsed, weights);
    // Empty object picks up serde defaults and stays valid.
    assert!(RebusWeights::from_json("{}").is_ok());
    assert!(RebusWeights::from_json("not json").is_err());
    assert!(RebusWeights::from_json(r#"{"frequency_scale": 0.0}"#).is_err());
}

#[test]
fn cache_limits_production_preset_and_defaults() {
    assert!(!CacheLimits::default().any_enabled());
    assert!(CacheLimits::production().any_enabled());

    let filled = CacheLimits::default().with_production_defaults();
    assert_eq!(filled, CacheLimits::production());

    let explicit = CacheLimits {
        g2p: 8,
        ..CacheLimits::default()
    };
    let merged = explicit.with_production_defaults();
    assert_eq!(merged.g2p, 8);
    assert_eq!(merged.embeddings, CacheLimits::production().embeddings);
}

// ---------------------------------------------------------------------------
// Core: errors
// ---------------------------------------------------------------------------

#[test]
fn provider_error_carries_provider_message_and_display() {
    let err = ProviderError::new("embedding", "boom");
    assert_eq!(err.provider, "embedding");
    assert_eq!(err.message, "boom");
    assert_eq!(format!("{err}"), "embedding provider failed: boom");
}

#[test]
fn text_intel_error_display_covers_every_variant() {
    let cases: Vec<(TextIntelError, &str)> = vec![
        (
            TextIntelError::InputTooLong {
                length: 9,
                maximum: 8,
            },
            "input length 9 exceeds max_input_length=8",
        ),
        (
            TextIntelError::TooManySegments { maximum: 4 },
            "message exceeds max_segments=4",
        ),
        (
            TextIntelError::InvalidConfiguration("bad".to_string()),
            "invalid configuration: bad",
        ),
        (
            TextIntelError::Provider(ProviderError::new("p", "m")),
            "p provider failed: m",
        ),
        (
            TextIntelError::Storage("disk".to_string()),
            "storage error: disk",
        ),
        (
            TextIntelError::Serialization("json".to_string()),
            "serialization error: json",
        ),
    ];
    for (err, expected) in cases {
        assert_eq!(format!("{err}"), expected);
    }
}

#[test]
fn text_intel_error_converts_from_provider_and_json_errors() {
    let converted = TextIntelError::from(ProviderError::new("p", "m"));
    assert!(matches!(converted, TextIntelError::Provider(_)));

    let json_err = serde_json::from_str::<serde_json::Value>("{bad").unwrap_err();
    let converted = TextIntelError::from(json_err);
    assert!(matches!(converted, TextIntelError::Serialization(_)));
}

// ---------------------------------------------------------------------------
// Core: types
// ---------------------------------------------------------------------------

#[test]
fn language_candidate_and_symbol_builders_set_fields() {
    let candidate = LanguageCandidate::new("en", 0.9);
    assert_eq!(candidate.language, "en");
    assert_eq!(candidate.probability, 0.9);

    let reading = SymbolReading::new("house", Some("en"), 0.8, "name").with_source("pack:test");
    assert_eq!(reading.text, "house");
    assert_eq!(reading.language.as_deref(), Some("en"));
    assert_eq!(reading.source.as_deref(), Some("pack:test"));

    let concept = SymbolConcept::new("concept:building.house", 0.7).with_source("builtin:test");
    assert_eq!(concept.id, "concept:building.house");
    assert_eq!(concept.source.as_deref(), Some("builtin:test"));
}

#[test]
fn transformation_builders_clamp_confidence_and_explain() {
    let base = Transformation::new("4", "a", "leetspeak");
    assert_eq!(base.start, None);
    assert_eq!(base.confidence, None);
    assert_eq!(base.explain(), "4 = a (leetspeak)");

    let spanned = Transformation::new("x", "y", "symbol").with_span(0, 1, "🏠");
    assert_eq!(spanned.explain(), "🏠 = y (symbol)");

    let full = Transformation::new("x", "y", "symbol")
        .with_span(0, 1, "🏠")
        .with_language("en")
        .with_provider("rebus")
        .with_confidence(0.9);
    assert_eq!(full.explain(), "🏠 = y (symbol; en)");
    assert_eq!(full.confidence, Some(0.9));

    assert_eq!(
        Transformation::new("a", "b", "t")
            .with_confidence(1.5)
            .confidence,
        Some(1.0)
    );
    assert_eq!(
        Transformation::new("a", "b", "t")
            .with_confidence(-2.0)
            .confidence,
        Some(0.0)
    );
}

#[test]
fn channel_availability_clamps_and_marks_unavailable() {
    let available = ChannelAvailability::available("rule-g2p", 0.7);
    assert!(available.available);
    assert_eq!(available.confidence, 0.7);

    let clamped = ChannelAvailability::available("x", 9.0);
    assert_eq!(clamped.confidence, 1.0);

    let unavailable = ChannelAvailability::unavailable("http-embed");
    assert!(!unavailable.available);
    assert_eq!(unavailable.confidence, 0.0);
}

#[test]
fn decoded_candidate_confidence_discounts_contested_ranks() {
    let lonely = DecodedCandidate {
        text: "hello".to_string(),
        score: 0.8,
        transformations: Vec::new(),
        language: None,
        lexical_score: 0.0,
        phonetic_score: 0.0,
        context_score: 0.0,
        symbol_score: 0.0,
        confidence_gap: 1.0,
        strong: true,
    };
    assert!((lonely.confidence() - 0.8).abs() < 1e-12);

    let contested = DecodedCandidate {
        confidence_gap: 0.0,
        ..lonely.clone()
    };
    assert!((contested.confidence() - 0.4).abs() < 1e-12);

    // Out-of-range scores clamp instead of escaping [0, 1].
    let over = DecodedCandidate {
        score: 5.0,
        confidence_gap: 1.0,
        ..lonely.clone()
    };
    assert!((over.confidence() - 1.0).abs() < 1e-12);
}

#[test]
fn transliteration_views_filter_clamp_and_default_confidence() {
    let mut fp = TextIntelligence::default().analyze("hello").unwrap();
    fp.normalization_views = BTreeMap::from([
        ("transliteration:a".to_string(), "hola".to_string()),
        ("transliteration:b".to_string(), "allo".to_string()),
        ("transliteration:c".to_string(), "ciao".to_string()),
        ("other".to_string(), "ignored".to_string()),
    ]);
    fp.transliteration_confidence = BTreeMap::from([
        ("transliteration:a".to_string(), 2.0), // clamps to 1.0
        ("transliteration:b".to_string(), f64::NAN), // hostile -> 0.0
                                                // transliteration:c missing -> defaults to 1.0
    ]);
    let mut views = fp.transliteration_views();
    views.sort_by(|a, b| a.0.cmp(b.0));
    assert_eq!(
        views,
        vec![("allo", 0.0), ("ciao", 1.0), ("hola", 1.0)],
        "views: {views:?}"
    );
}

#[test]
fn stage_timings_map_and_names_cover_pipeline_order() {
    assert_eq!(
        StageTimings::stage_names(),
        &[
            "normalization",
            "language",
            "symbols",
            "rebus",
            "semantic",
            "phonetic",
            "comparison",
            "total"
        ]
    );
    let timings = StageTimings {
        total_micros: 3.0,
        ..StageTimings::default()
    };
    let map = timings.as_map();
    assert_eq!(map.len(), 8);
    assert_eq!(map["total"], 3.0);
    let ordered: Vec<String> = StageTimings::stage_names()
        .iter()
        .map(ToString::to_string)
        .collect();
    let mut keys: Vec<String> = map.keys().cloned().collect();
    keys.sort();
    let mut expected = ordered.clone();
    expected.sort();
    assert_eq!(keys, expected);
}

#[test]
fn duplicate_mode_defaults_to_combined() {
    assert_eq!(DuplicateMode::default(), DuplicateMode::Combined);
}

#[test]
fn fingerprint_and_comparison_results_serialize_stably() {
    let engine = TextIntelligence::default();
    let fp = engine.analyze("hello").unwrap();
    let json = serde_json::to_string(&fp).unwrap();
    let back: textintel::MessageFingerprint = serde_json::from_str(&json).unwrap();
    assert_eq!(back.raw, "hello");
    assert_eq!(back.schema_version, FINGERPRINT_SCHEMA_VERSION);

    let comparison = engine.compare("hello", "hallo").unwrap();
    let json = serde_json::to_string(&comparison).unwrap();
    assert!(json.contains("\"score\""));
}
