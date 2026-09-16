//! Lexical coverage: character metrics, string similarity, ngrams,
//! minhash, and tokenization.

use std::collections::BTreeSet;

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
