//! Normalization coverage: unicode, confusables, leetspeak, repetition,
//! and whitespace.

use textintel::normalization::confusables::{confusable_map, skeleton};
use textintel::normalization::leetspeak::{apply_leet, detect_leet, leet_map};
use textintel::normalization::repetition::{collapse_repetition, repetition_ratio};
use textintel::normalization::unicode::{casefold_text, nfc, nfkc, strip_diacritics};
use textintel::normalization::whitespace::{is_extra_whitespace, normalize_whitespace};

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
