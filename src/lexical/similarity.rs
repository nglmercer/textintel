use std::collections::{BTreeMap, BTreeSet};

use crate::lexical::tokenizer::{simple_lemmas, tokenize};
use crate::normalization::unicode::casefold_text;

pub fn jaccard(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    a.intersection(b).count() as f64 / a.union(b).count() as f64
}

pub fn lexical_similarity(a: &str, b: &str) -> f64 {
    let left: BTreeSet<_> = simple_lemmas(&tokenize(&casefold_text(a))).into_iter().collect();
    let right: BTreeSet<_> = simple_lemmas(&tokenize(&casefold_text(b))).into_iter().collect();
    jaccard(&left, &right)
}

pub fn term_frequency(tokens: &[String]) -> BTreeMap<String, f64> {
    let mut counts = BTreeMap::new();
    for token in tokens {
        *counts.entry(token.clone()).or_insert(0.0) += 1.0;
    }
    let total = tokens.len().max(1) as f64;
    counts.values_mut().for_each(|value| *value /= total);
    counts
}

/// Two-document TF-IDF similarity.  It is useful for small local collections;
/// large corpora should provide document statistics through a search backend.
pub fn tfidf_similarity(a: &[String], b: &[String]) -> f64 {
    let tf_a = term_frequency(a);
    let tf_b = term_frequency(b);
    let terms: BTreeSet<_> = tf_a.keys().chain(tf_b.keys()).collect();
    let mut left = Vec::with_capacity(terms.len());
    let mut right = Vec::with_capacity(terms.len());
    for term in terms {
        let df = usize::from(tf_a.contains_key(term)) + usize::from(tf_b.contains_key(term));
        let idf = (2.0 / df.max(1) as f64).ln_1p();
        left.push(tf_a.get(term).copied().unwrap_or(0.0) * idf);
        right.push(tf_b.get(term).copied().unwrap_or(0.0) * idf);
    }
    let dot: f64 = left.iter().zip(&right).map(|(x, y)| x * y).sum();
    let nl = left.iter().map(|value| value * value).sum::<f64>().sqrt();
    let nr = right.iter().map(|value| value * value).sum::<f64>().sqrt();
    if nl == 0.0 || nr == 0.0 { 0.0 } else { dot / (nl * nr) }
}

