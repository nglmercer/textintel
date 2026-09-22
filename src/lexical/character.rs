use std::collections::{BTreeMap, HashMap};

use crate::core::types::{CharacterFeatures, CharacterSimilarity};
use crate::lexical::ngrams::character_ngrams;
use crate::normalization::unicode::{casefold_text, strip_diacritics};

pub fn char_features(text: &str) -> CharacterFeatures {
    let mut letters = 0;
    let mut digits = 0;
    let mut whitespace = 0;
    let mut punctuation = 0;
    let mut other = 0;
    for ch in text.chars() {
        if ch.is_alphabetic() {
            letters += 1;
        } else if ch.is_numeric() {
            digits += 1;
        } else if ch.is_whitespace() {
            whitespace += 1;
        } else if ch.is_alphanumeric() {
            other += 1;
        } else if ch.is_ascii_punctuation()
            || matches!(ch as u32, 0x2000..=0x206f | 0x2e00..=0x2e7f | 0x3000..=0x303f)
        {
            punctuation += 1;
        } else {
            other += 1;
        }
    }
    let folded = casefold_text(text);
    CharacterFeatures {
        length: text.chars().count(),
        letters,
        digits,
        whitespace,
        punctuation,
        other,
        ngrams_2: count_ngrams(&folded, 2),
        ngrams_3: count_ngrams(&folded, 3),
    }
}

fn count_ngrams(text: &str, n: usize) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for gram in character_ngrams(text, n) {
        *counts.entry(gram).or_insert(0) += 1;
    }
    counts
}

pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    levenshtein_chars(&a, &b)
}

/// [`levenshtein`] over pre-collected chars, so callers scoring several
/// metrics pay for collection once. Same recurrence, same value.
fn levenshtein_chars(a: &[char], b: &[char]) -> usize {
    if a == b {
        return 0;
    }
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        // Pre-sized row (same recurrence, no per-row growth reallocations).
        current[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let insert = current[j] + 1;
            let delete = previous[j + 1] + 1;
            let substitute = previous[j] + usize::from(ca != cb);
            current[j + 1] = insert.min(delete).min(substitute);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    *previous.last().unwrap_or(&0)
}

pub fn damerau_levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    damerau_levenshtein_chars(&a, &b)
}

/// [`damerau_levenshtein`] over pre-collected chars. Same recurrence,
/// same value.
fn damerau_levenshtein_chars(a: &[char], b: &[char]) -> usize {
    if a == b {
        return 0;
    }
    // Three rolling rows (same optimal-string-alignment recurrence — the
    // transposition term reaches back two rows) instead of the full
    // matrix: identical values, O(min) memory. `row_1` always holds the
    // previous row on loop entry (matrix row 0 at start).
    let mut row_0 = vec![0usize; b.len() + 1];
    let mut row_1: Vec<usize> = (0..=b.len()).collect();
    let mut row_2 = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        row_2[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            row_2[j] = (row_1[j] + 1)
                .min(row_2[j - 1] + 1)
                .min(row_1[j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                row_2[j] = row_2[j].min(row_0[j - 2] + 1);
            }
        }
        std::mem::swap(&mut row_0, &mut row_1);
        std::mem::swap(&mut row_1, &mut row_2);
    }
    row_1[b.len()]
}

fn normalized_edit(distance: usize, a: &[char], b: &[char]) -> f64 {
    1.0 - distance as f64 / a.len().max(b.len()).max(1) as f64
}

pub fn jaro(a: &str, b: &str) -> f64 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    jaro_chars(&a, &b)
}

/// [`jaro`] over pre-collected chars. Same matching pass, same value.
fn jaro_chars(a: &[char], b: &[char]) -> f64 {
    if a == b {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let distance = (a.len().max(b.len()) / 2).saturating_sub(1);
    let mut a_matches = vec![false; a.len()];
    let mut b_matches = vec![false; b.len()];
    let mut matches = 0usize;
    for (i, ca) in a.iter().enumerate() {
        let start = i.saturating_sub(distance);
        let end = (i + distance + 1).min(b.len());
        for j in start..end {
            if b_matches[j] || ca != &b[j] {
                continue;
            }
            a_matches[i] = true;
            b_matches[j] = true;
            matches += 1;
            break;
        }
    }
    if matches == 0 {
        return 0.0;
    }
    let mut k = 0;
    let mut transpositions = 0;
    for i in 0..a.len() {
        if !a_matches[i] {
            continue;
        }
        while !b_matches[k] {
            k += 1;
        }
        if a[i] != b[k] {
            transpositions += 1;
        }
        k += 1;
    }
    let transpositions = transpositions as f64 / 2.0;
    (matches as f64 / a.len() as f64
        + matches as f64 / b.len() as f64
        + (matches as f64 - transpositions) / matches as f64)
        / 3.0
}

pub fn jaro_winkler(a: &str, b: &str) -> f64 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    jaro_winkler_from_chars(jaro_chars(&a, &b), &a, &b)
}

/// [`jaro_winkler`] over a precomputed Jaro score and pre-collected
/// chars, so callers that need both pay for collection and the matching
/// pass once. Identical formula, same value.
fn jaro_winkler_from_chars(jaro_score: f64, a: &[char], b: &[char]) -> f64 {
    let prefix = a
        .iter()
        .zip(b.iter())
        .take_while(|(left, right)| left == right)
        .take(4)
        .count();
    (jaro_score + prefix as f64 * 0.1 * (1.0 - jaro_score)).min(1.0)
}

pub fn ngram_similarity(a: &str, b: &str, n: usize) -> f64 {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    ngram_similarity_chars(&a_chars, &b_chars, n, a == b)
}

/// Packed n-gram key for `n <= 3`: every `char` fits 21 bits
/// (`char::MAX` is `0x10FFFF`), so three chars pack into a `u64` with
/// room to spare. Packed equality is exactly sequence equality, and
/// `n` is fixed per call, so no length tag is needed.
fn packed_ngram(window: &[char]) -> u64 {
    let mut key = 0u64;
    for (position, ch) in window.iter().enumerate() {
        key |= (*ch as u64) << (21 * position);
    }
    key
}

/// [`ngram_similarity`] over pre-collected chars. Short n-grams count by
/// packed integer key instead of per-gram `String`s (no allocation per
/// window); longer n-grams keep the string-keyed path over the same
/// windows. Either way intersection and union are exact integer sums,
/// so the value matches the string-based computation bit for bit.
fn ngram_similarity_chars(a: &[char], b: &[char], n: usize, sides_equal: bool) -> f64 {
    if n == 0 || (a.len() < n && b.len() < n) {
        return if sides_equal { 1.0 } else { 0.0 };
    }
    if n <= 3 {
        let mut left_counts = HashMap::new();
        let mut right_counts = HashMap::new();
        for window in a.windows(n) {
            *left_counts.entry(packed_ngram(window)).or_insert(0usize) += 1;
        }
        for window in b.windows(n) {
            *right_counts.entry(packed_ngram(window)).or_insert(0usize) += 1;
        }
        return jaccard_counts(&left_counts, &right_counts);
    }
    let mut left_counts = HashMap::new();
    let mut right_counts = HashMap::new();
    for window in a.windows(n) {
        *left_counts
            .entry(window.iter().collect::<String>())
            .or_insert(0usize) += 1;
    }
    for window in b.windows(n) {
        *right_counts
            .entry(window.iter().collect::<String>())
            .or_insert(0usize) += 1;
    }
    jaccard_counts(&left_counts, &right_counts)
}

/// Jaccard index over count maps: intersection and union are exact
/// integer sums, so iteration order cannot change the result.
fn jaccard_counts<K: Eq + std::hash::Hash>(
    left_counts: &HashMap<K, usize>,
    right_counts: &HashMap<K, usize>,
) -> f64 {
    let mut intersection = 0usize;
    let mut union = 0usize;
    for (gram, left_count) in left_counts {
        let right_count = right_counts.get(gram).copied().unwrap_or(0);
        intersection += left_count.min(&right_count);
        union += left_count.max(&right_count);
    }
    for (gram, right_count) in right_counts {
        if !left_counts.contains_key(gram) {
            union += right_count;
        }
    }
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

pub fn lcs_len(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    lcs_len_chars(&a, &b)
}

/// [`lcs_len`] over pre-collected chars. Same recurrence, same value.
fn lcs_len_chars(a: &[char], b: &[char]) -> usize {
    if a.is_empty() || b.is_empty() {
        return 0;
    }
    let mut previous = vec![0usize; b.len() + 1];
    let mut current = vec![0usize; b.len() + 1];
    for ca in a.iter().copied() {
        // Pre-sized row (same recurrence, no per-row growth reallocations).
        for (j, cb) in b.iter().enumerate() {
            current[j + 1] = if ca == *cb {
                previous[j] + 1
            } else {
                previous[j + 1].max(current[j])
            };
        }
        std::mem::swap(&mut previous, &mut current);
    }
    *previous.last().unwrap_or(&0)
}

pub fn lcs_similarity(a: &str, b: &str) -> f64 {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    lcs_len_chars(&a_chars, &b_chars) as f64 / a_chars.len().max(b_chars.len()).max(1) as f64
}

pub fn character_similarity(a: &str, b: &str) -> CharacterSimilarity {
    let aa = strip_diacritics(&casefold_text(a));
    let bb = strip_diacritics(&casefold_text(b));
    let ac: Vec<char> = aa.chars().collect();
    let bc: Vec<char> = bb.chars().collect();
    // One collection serves all six metrics; each core keeps its
    // original recurrence, so the blend below is unchanged.
    let lev = normalized_edit(levenshtein_chars(&ac, &bc), &ac, &bc);
    let dam = normalized_edit(damerau_levenshtein_chars(&ac, &bc), &ac, &bc);
    let ja = jaro_chars(&ac, &bc);
    let jw = jaro_winkler_from_chars(ja, &ac, &bc);
    let ng = 0.5 * ngram_similarity_chars(&ac, &bc, 2, aa == bb)
        + 0.5 * ngram_similarity_chars(&ac, &bc, 3, aa == bb);
    let lcs = lcs_len_chars(&ac, &bc) as f64 / ac.len().max(bc.len()).max(1) as f64;
    let combined = 0.2 * lev + 0.15 * dam + 0.15 * ja + 0.2 * jw + 0.15 * ng + 0.15 * lcs;
    CharacterSimilarity {
        levenshtein: lev,
        damerau_levenshtein: dam,
        jaro: ja,
        jaro_winkler: jw,
        ngram_similarity: ng,
        lcs,
        combined: combined.clamp(0.0, 1.0),
    }
}

pub fn combined_character_similarity(a: &str, b: &str) -> f64 {
    character_similarity(a, b).combined
}
