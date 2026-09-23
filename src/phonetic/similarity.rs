use std::collections::BTreeMap;

use crate::core::types::PhoneticCandidate;
use crate::phonetic::features::{Features, classify_phoneme, distance_for_features};

/// Phoneme sequences interned to small indices. The phone inventory is
/// tiny, so every comparison below works on indices: index equality is
/// exactly string equality, and each distinct phone is classified once.
/// All recurrences keep their original operation order, so results are
/// bit-identical to the string-based computation.
struct InternedPhones {
    left: Vec<usize>,
    right: Vec<usize>,
    features: Vec<Features>,
}

fn intern_index<'a>(phones: &mut Vec<&'a str>, phone: &'a str) -> usize {
    match phones.iter().position(|known| *known == phone) {
        Some(index) => index,
        None => {
            phones.push(phone);
            phones.len() - 1
        }
    }
}

fn intern_phones(a: &[String], b: &[String]) -> InternedPhones {
    let mut phones = Vec::new();
    let left = a
        .iter()
        .map(|phone| intern_index(&mut phones, phone.as_str()))
        .collect();
    let right = b
        .iter()
        .map(|phone| intern_index(&mut phones, phone.as_str()))
        .collect();
    let features = phones.iter().map(|phone| classify_phoneme(phone)).collect();
    InternedPhones {
        left,
        right,
        features,
    }
}

fn edit_distance_idx(left: &[usize], right: &[usize]) -> usize {
    if left.is_empty() {
        return right.len();
    }
    if right.is_empty() {
        return left.len();
    }
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0usize; right.len() + 1];
    for (i, left) in left.iter().enumerate() {
        current[0] = i + 1;
        for (j, right) in right.iter().enumerate() {
            let substitute = previous[j] + usize::from(left != right);
            current[j + 1] = (current[j] + 1).min(previous[j + 1] + 1).min(substitute);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

pub fn phoneme_edit_distance(a: &[String], b: &[String]) -> usize {
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let interned = intern_phones(a, b);
    edit_distance_idx(&interned.left, &interned.right)
}

/// Both edit recurrences in one pass: the weighted (feature) and exact
/// (index-equality) distances share loop structure but keep independent
/// rows, so each returns exactly what its separate function computes —
/// one nest, one walk over the sequences, half the row traffic.
fn edit_distances_idx(left: &[usize], right: &[usize], features: &[Features]) -> (f64, usize) {
    if left.is_empty() && right.is_empty() {
        return (0.0, 0);
    }
    // Pairwise feature distances, computed once per distinct pair. A
    // shared index means identical strings, which score 0.0 exactly as
    // the string equality fast path did.
    let distinct = features.len();
    let mut pair = vec![0.0f64; distinct * distinct];
    for (i, left) in features.iter().enumerate() {
        for (j, right) in features.iter().enumerate() {
            pair[i * distinct + j] = if i == j {
                0.0
            } else {
                distance_for_features(left, right)
            };
        }
    }
    let mut w_previous: Vec<f64> = (0..=right.len()).map(|value| value as f64).collect();
    let mut w_current = vec![0.0f64; right.len() + 1];
    let mut e_previous: Vec<usize> = (0..=right.len()).collect();
    let mut e_current = vec![0usize; right.len() + 1];
    for (i, left) in left.iter().enumerate() {
        w_current[0] = (i + 1) as f64;
        e_current[0] = i + 1;
        for (j, right) in right.iter().enumerate() {
            let w_substitute = w_previous[j] + pair[left * distinct + right];
            w_current[j + 1] = (w_current[j] + 1.0)
                .min(w_previous[j + 1] + 1.0)
                .min(w_substitute);
            let e_substitute = e_previous[j] + usize::from(left != right);
            e_current[j + 1] = (e_current[j] + 1)
                .min(e_previous[j + 1] + 1)
                .min(e_substitute);
        }
        std::mem::swap(&mut w_previous, &mut w_current);
        std::mem::swap(&mut e_previous, &mut e_current);
    }
    (w_previous[right.len()], e_previous[right.len()])
}

fn weighted_distance_idx(left: &[usize], right: &[usize], features: &[Features]) -> f64 {
    if left.is_empty() && right.is_empty() {
        return 0.0;
    }
    // Pairwise feature distances, computed once per distinct pair. A
    // shared index means identical strings, which score 0.0 exactly as
    // the string equality fast path did.
    let distinct = features.len();
    let mut pair = vec![0.0f64; distinct * distinct];
    for (i, left) in features.iter().enumerate() {
        for (j, right) in features.iter().enumerate() {
            pair[i * distinct + j] = if i == j {
                0.0
            } else {
                distance_for_features(left, right)
            };
        }
    }
    let mut previous: Vec<f64> = (0..=right.len()).map(|value| value as f64).collect();
    let mut current = vec![0.0f64; right.len() + 1];
    for (i, left) in left.iter().enumerate() {
        current[0] = (i + 1) as f64;
        for (j, right) in right.iter().enumerate() {
            let substitute = previous[j] + pair[left * distinct + right];
            current[j + 1] = (current[j] + 1.0)
                .min(previous[j + 1] + 1.0)
                .min(substitute);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

pub fn weighted_phoneme_distance(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let interned = intern_phones(a, b);
    weighted_distance_idx(&interned.left, &interned.right, &interned.features)
}

/// Packed n-gram key: length in the top 8 bits, up to three 16-bit phone
/// indices below. Falls back to boxed keys past either bound.
fn packed_key(window: &[usize], distinct: usize) -> Option<u64> {
    if window.len() > 3 || distinct > u16::MAX as usize {
        return None;
    }
    let mut key = (window.len() as u64) << 48;
    for (position, index) in window.iter().enumerate() {
        key |= (*index as u64) << (16 * position);
    }
    Some(key)
}

/// Packed keys of one side's n-grams as a plain vec: sorted below
/// instead of counted through a tree (no per-window pointer chasing or
/// node allocs; integer keys sort fast).
fn ngram_keys_packed(items: &[usize], distinct: usize, n: usize) -> Vec<u64> {
    items
        .windows(n)
        .filter_map(|window| packed_key(window, distinct))
        .collect()
}

/// Jaccard index over two key multisets: sort both sides, then walk
/// runs in lockstep. Intersection and union are exact integer sums
/// over the same multisets the tree-counted path sums, so the value
/// matches bit for bit.
fn jaccard_sorted(left_keys: &mut [u64], right_keys: &mut [u64]) -> f64 {
    left_keys.sort_unstable();
    right_keys.sort_unstable();
    fn run_length(keys: &[u64], mut index: usize) -> (usize, usize) {
        let key = keys[index];
        let mut count = 0;
        while index < keys.len() && keys[index] == key {
            count += 1;
            index += 1;
        }
        (index, count)
    }
    let (mut i, mut j) = (0usize, 0usize);
    let (mut intersection, mut union) = (0usize, 0usize);
    while i < left_keys.len() && j < right_keys.len() {
        let (left_key, right_key) = (left_keys[i], right_keys[j]);
        if left_key == right_key {
            let (next_i, left_count) = run_length(left_keys, i);
            let (next_j, right_count) = run_length(right_keys, j);
            intersection += left_count.min(right_count);
            union += left_count.max(right_count);
            i = next_i;
            j = next_j;
        } else if left_key < right_key {
            let (next_i, left_count) = run_length(left_keys, i);
            union += left_count;
            i = next_i;
        } else {
            let (next_j, right_count) = run_length(right_keys, j);
            union += right_count;
            j = next_j;
        }
    }
    while i < left_keys.len() {
        let (next_i, left_count) = run_length(left_keys, i);
        union += left_count;
        i = next_i;
    }
    while j < right_keys.len() {
        let (next_j, right_count) = run_length(right_keys, j);
        union += right_count;
        j = next_j;
    }
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

/// 2-gram and 3-gram packed keys in one pass: position `i` feeds the
/// 2-gram at `i` and (while in bounds) the 3-gram at `i`, the same window
/// multisets the two separate builds collect, sorted downstream.
fn ngram23_keys_packed(items: &[usize], distinct: usize) -> (Vec<u64>, Vec<u64>) {
    let mut two = Vec::new();
    let mut three = Vec::new();
    if items.len() >= 2 {
        for i in 0..items.len() - 1 {
            if let Some(key) = packed_key(&items[i..i + 2], distinct) {
                two.push(key);
            }
            if i + 3 <= items.len()
                && let Some(key) = packed_key(&items[i..i + 3], distinct)
            {
                three.push(key);
            }
        }
    }
    (two, three)
}

fn ngram_counts_boxed(items: &[usize], n: usize) -> BTreeMap<Vec<usize>, usize> {
    let mut map = BTreeMap::new();
    for window in items.windows(n) {
        *map.entry(window.to_vec()).or_insert(0) += 1;
    }
    map
}

fn jaccard<C: Ord>(left: &BTreeMap<C, usize>, right: &BTreeMap<C, usize>) -> f64 {
    let mut intersection = 0usize;
    let mut union = 0usize;
    for (key, &left_count) in left {
        let right_count = right.get(key).copied().unwrap_or(0);
        intersection += left_count.min(right_count);
        union += left_count.max(right_count);
    }
    for (key, &right_count) in right {
        if !left.contains_key(key) {
            union += right_count;
        }
    }
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

fn ngram_similarity_idx(
    left: &[usize],
    right: &[usize],
    distinct: usize,
    n: usize,
    sides_equal: bool,
) -> f64 {
    if n == 0 {
        return 0.0;
    }
    // windows(n) is empty on both sides exactly when both sides are
    // shorter than n; the original compared the raw slices there.
    if left.len() < n && right.len() < n {
        return if sides_equal { 1.0 } else { 0.0 };
    }
    if n <= 3 && distinct <= u16::MAX as usize {
        let mut packed_left = ngram_keys_packed(left, distinct, n);
        let mut packed_right = ngram_keys_packed(right, distinct, n);
        if packed_left.is_empty() && packed_right.is_empty() {
            return if sides_equal { 1.0 } else { 0.0 };
        }
        return jaccard_sorted(&mut packed_left, &mut packed_right);
    }
    let boxed_left = ngram_counts_boxed(left, n);
    let boxed_right = ngram_counts_boxed(right, n);
    if boxed_left.is_empty() && boxed_right.is_empty() {
        return if sides_equal { 1.0 } else { 0.0 };
    }
    jaccard(&boxed_left, &boxed_right)
}

pub fn phoneme_ngram_similarity(a: &[String], b: &[String], n: usize) -> f64 {
    if n == 0 {
        return 0.0;
    }
    let interned = intern_phones(a, b);
    ngram_similarity_idx(
        &interned.left,
        &interned.right,
        interned.features.len(),
        n,
        a == b,
    )
}

pub fn phonetic_similarity(a: &PhoneticCandidate, b: &PhoneticCandidate) -> f64 {
    phonetic_similarity_raw(&a.phonemes, a.confidence, &b.phonemes, b.confidence)
}

/// [`phonetic_similarity`] over bare phoneme sequences plus confidences:
/// the same channels, blend, and operation order for callers that never
/// materialize full candidates.
pub fn phonetic_similarity_raw(
    a_phonemes: &[String],
    a_confidence: f64,
    b_phonemes: &[String],
    b_confidence: f64,
) -> f64 {
    if a_phonemes.is_empty() || b_phonemes.is_empty() {
        return 0.0;
    }
    let max_len = a_phonemes.len().max(b_phonemes.len()).max(1) as f64;
    // One interning serves all four channels; each channel keeps its
    // original recurrence, so the blend below is unchanged.
    let interned = intern_phones(a_phonemes, b_phonemes);
    let sides_equal = a_phonemes == b_phonemes;
    let distinct = interned.features.len();
    let (weighted, exact_distance) =
        edit_distances_idx(&interned.left, &interned.right, &interned.features);
    let edit = 1.0 - weighted / max_len;
    let exact = 1.0 - exact_distance as f64 / max_len;
    // Long-enough sides take the packed path with non-empty maps for
    // both n = 2 and n = 3, so both Jaccards come out of one counting
    // pass per side; short or huge-alphabet sides keep the general path.
    let grams = if interned.left.len() >= 3
        && interned.right.len() >= 3
        && distinct <= u16::MAX as usize
    {
        let (mut left2, mut left3) = ngram23_keys_packed(&interned.left, distinct);
        let (mut right2, mut right3) = ngram23_keys_packed(&interned.right, distinct);
        0.5 * jaccard_sorted(&mut left2, &mut right2)
            + 0.5 * jaccard_sorted(&mut left3, &mut right3)
    } else {
        0.5 * ngram_similarity_idx(&interned.left, &interned.right, distinct, 2, sides_equal)
            + 0.5 * ngram_similarity_idx(&interned.left, &interned.right, distinct, 3, sides_equal)
    };
    // Hostile fingerprints can carry non-finite confidences; sanitize so
    // the channel stays total.
    let confidence_a = if a_confidence.is_finite() {
        a_confidence
    } else {
        0.0
    };
    let confidence_b = if b_confidence.is_finite() {
        b_confidence
    } else {
        0.0
    };
    let score = 0.45 * edit + 0.25 * exact + 0.20 * grams + 0.10 * confidence_a.min(confidence_b);
    if score.is_finite() {
        score.clamp(0.0, 1.0)
    } else {
        0.0
    }
}
