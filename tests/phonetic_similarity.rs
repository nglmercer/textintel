//! Golden regression tests for phonetic similarity. The implementation
//! interns phonemes and packs n-gram keys for speed; these values pin the
//! exact legacy behavior (verified bit-identical against the pre-optimization
//! implementation on 460+ fuzzed pairs before commit).

use textintel::phonetic::similarity::{
    phoneme_edit_distance, phoneme_ngram_similarity, weighted_phoneme_distance,
};

fn phones(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-12,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn similarity_goldens_match_legacy_behavior() {
    let a = phones(&["tʃ", "ə", "n", "dʒ"]);
    let b = phones(&["dʒ", "ə", "n", "tʃ"]);
    assert_eq!(phoneme_edit_distance(&a, &b), 2);
    close(weighted_phoneme_distance(&a, &b), 0.6);
    close(phoneme_ngram_similarity(&a, &b, 2), 0.2);
    close(phoneme_ngram_similarity(&a, &b, 3), 0.0);
    // n > 3 takes the boxed-keys path; windows still disagree here.
    close(phoneme_ngram_similarity(&a, &b, 4), 0.0);
    close(weighted_phoneme_distance(&a, &a), 0.0);
}

#[test]
fn similarity_edges_match_legacy_behavior() {
    let a = phones(&["a"]);
    assert_eq!(phoneme_edit_distance(&[], &a), 1);
    assert_eq!(phoneme_edit_distance(&a, &[]), 1);
    assert_eq!(phoneme_edit_distance(&[], &[]), 0);
    close(weighted_phoneme_distance(&[], &[]), 0.0);
    close(phoneme_ngram_similarity(&a, &a, 0), 0.0);
    // Both sides shorter than n: equal slices score 1.0.
    close(phoneme_ngram_similarity(&a, &a, 3), 1.0);
    close(phoneme_ngram_similarity(&a, &phones(&["p"]), 3), 0.0);
}
