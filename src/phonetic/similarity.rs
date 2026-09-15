use std::collections::BTreeMap;

use crate::core::types::PhoneticCandidate;
use crate::phonetic::features::articulatory_distance;

pub fn phoneme_edit_distance(a: &[String], b: &[String]) -> usize {
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, left) in a.iter().enumerate() {
        let mut current = vec![i + 1];
        for (j, right) in b.iter().enumerate() {
            let substitute = previous[j] + usize::from(left != right);
            current.push((current[j] + 1).min(previous[j + 1] + 1).min(substitute));
        }
        previous = current;
    }
    previous[b.len()]
}

fn feature_distance(left: &str, right: &str) -> f64 {
    articulatory_distance(left, right)
}

pub fn weighted_phoneme_distance(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let mut previous: Vec<f64> = (0..=b.len()).map(|value| value as f64).collect();
    for (i, left) in a.iter().enumerate() {
        let mut current = vec![(i + 1) as f64];
        for (j, right) in b.iter().enumerate() {
            let substitute = previous[j] + feature_distance(left, right);
            current.push(
                (current[j] + 1.0)
                    .min(previous[j + 1] + 1.0)
                    .min(substitute),
            );
        }
        previous = current;
    }
    previous[b.len()]
}

pub fn phoneme_ngram_similarity(a: &[String], b: &[String], n: usize) -> f64 {
    if n == 0 {
        return 0.0;
    }
    let grams = |items: &[String]| -> BTreeMap<Vec<String>, usize> {
        let mut map = BTreeMap::new();
        for window in items.windows(n) {
            *map.entry(window.to_vec()).or_insert(0) += 1;
        }
        map
    };
    let left = grams(a);
    let right = grams(b);
    if left.is_empty() && right.is_empty() {
        return if a == b { 1.0 } else { 0.0 };
    }
    let keys: std::collections::BTreeSet<_> = left.keys().chain(right.keys()).collect();
    let intersection: usize = keys
        .iter()
        .map(|key| {
            left.get(*key)
                .unwrap_or(&0)
                .min(right.get(*key).unwrap_or(&0))
        })
        .sum();
    let union: usize = keys
        .iter()
        .map(|key| {
            left.get(*key)
                .unwrap_or(&0)
                .max(right.get(*key).unwrap_or(&0))
        })
        .sum();
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

pub fn phonetic_similarity(a: &PhoneticCandidate, b: &PhoneticCandidate) -> f64 {
    if a.phonemes.is_empty() || b.phonemes.is_empty() {
        return 0.0;
    }
    let max_len = a.phonemes.len().max(b.phonemes.len()).max(1) as f64;
    let edit = 1.0 - weighted_phoneme_distance(&a.phonemes, &b.phonemes) / max_len;
    let exact = 1.0 - phoneme_edit_distance(&a.phonemes, &b.phonemes) as f64 / max_len;
    let grams = 0.5 * phoneme_ngram_similarity(&a.phonemes, &b.phonemes, 2)
        + 0.5 * phoneme_ngram_similarity(&a.phonemes, &b.phonemes, 3);
    // Hostile fingerprints can carry non-finite confidences; sanitize so
    // the channel stays total.
    let confidence_a = if a.confidence.is_finite() {
        a.confidence
    } else {
        0.0
    };
    let confidence_b = if b.confidence.is_finite() {
        b.confidence
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
