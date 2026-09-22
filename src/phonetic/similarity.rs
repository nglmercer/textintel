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

fn ngram_counts_packed(items: &[usize], distinct: usize, n: usize) -> BTreeMap<u64, usize> {
    // Ordered, not hashed: these maps are small with integer keys, where
    // BTree compares beat hashing (measured: HashMap<SipHash> is slower).
    let mut map = BTreeMap::new();
    for window in items.windows(n) {
        if let Some(key) = packed_key(window, distinct) {
            *map.entry(key).or_insert(0) += 1;
        }
    }
    map
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
        let packed_left = ngram_counts_packed(left, distinct, n);
        let packed_right = ngram_counts_packed(right, distinct, n);
        if packed_left.is_empty() && packed_right.is_empty() {
            return if sides_equal { 1.0 } else { 0.0 };
        }
        return jaccard(&packed_left, &packed_right);
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
    let edit =
        1.0 - weighted_distance_idx(&interned.left, &interned.right, &interned.features) / max_len;
    let exact = 1.0 - edit_distance_idx(&interned.left, &interned.right) as f64 / max_len;
    let grams =
        0.5 * ngram_similarity_idx(
            &interned.left,
            &interned.right,
            interned.features.len(),
            2,
            sides_equal,
        ) + 0.5
            * ngram_similarity_idx(
                &interned.left,
                &interned.right,
                interned.features.len(),
                3,
                sides_equal,
            );
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
