use std::collections::BTreeMap;

use crate::core::types::{CharacterFeatures, CharacterSimilarity};
use crate::lexical::ngrams::character_ngrams;
use crate::normalization::unicode::casefold_text;

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
    for (i, ca) in a.iter().enumerate() {
        let mut current = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let insert = current[j] + 1;
            let delete = previous[j + 1] + 1;
            let substitute = previous[j] + usize::from(ca != cb);
            current.push(insert.min(delete).min(substitute));
        }
        previous = current;
    }
    *previous.last().unwrap_or(&0)
}

pub fn damerau_levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut matrix = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in matrix.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, value) in matrix[0].iter_mut().enumerate() {
        *value = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                matrix[i][j] = matrix[i][j].min(matrix[i - 2][j - 2] + 1);
            }
        }
    }
    matrix[a.len()][b.len()]
}

fn normalized_edit(distance: usize, a: &[char], b: &[char]) -> f64 {
    1.0 - distance as f64 / a.len().max(b.len()).max(1) as f64
}

pub fn jaro(a: &str, b: &str) -> f64 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
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
    let jaro_score = jaro(a, b);
    let prefix = a
        .chars()
        .zip(b.chars())
        .take_while(|(left, right)| left == right)
        .take(4)
        .count();
    (jaro_score + prefix as f64 * 0.1 * (1.0 - jaro_score)).min(1.0)
}

pub fn ngram_similarity(a: &str, b: &str, n: usize) -> f64 {
    let left = character_ngrams(a, n);
    let right = character_ngrams(b, n);
    if left.is_empty() && right.is_empty() {
        return if a == b { 1.0 } else { 0.0 };
    }
    let mut left_counts = BTreeMap::new();
    let mut right_counts = BTreeMap::new();
    for gram in left {
        *left_counts.entry(gram).or_insert(0usize) += 1;
    }
    for gram in right {
        *right_counts.entry(gram).or_insert(0usize) += 1;
    }
    let keys: std::collections::BTreeSet<_> =
        left_counts.keys().chain(right_counts.keys()).collect();
    let intersection = keys
        .iter()
        .map(|key| {
            left_counts
                .get(*key)
                .unwrap_or(&0)
                .min(right_counts.get(*key).unwrap_or(&0))
        })
        .sum::<usize>();
    let union = keys
        .iter()
        .map(|key| {
            left_counts
                .get(*key)
                .unwrap_or(&0)
                .max(right_counts.get(*key).unwrap_or(&0))
        })
        .sum::<usize>();
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

pub fn lcs_len(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() || b.is_empty() {
        return 0;
    }
    let mut previous = vec![0usize; b.len() + 1];
    for ca in a {
        let mut current = vec![0usize];
        for (j, cb) in b.iter().enumerate() {
            current.push(if ca == *cb {
                previous[j] + 1
            } else {
                previous[j + 1].max(current[j])
            });
        }
        previous = current;
    }
    *previous.last().unwrap_or(&0)
}

pub fn lcs_similarity(a: &str, b: &str) -> f64 {
    lcs_len(a, b) as f64 / a.chars().count().max(b.chars().count()).max(1) as f64
}

pub fn character_similarity(a: &str, b: &str) -> CharacterSimilarity {
    let aa = casefold_text(a);
    let bb = casefold_text(b);
    let ac: Vec<char> = aa.chars().collect();
    let bc: Vec<char> = bb.chars().collect();
    let lev = normalized_edit(levenshtein(&aa, &bb), &ac, &bc);
    let dam = normalized_edit(damerau_levenshtein(&aa, &bb), &ac, &bc);
    let ja = jaro(&aa, &bb);
    let jw = jaro_winkler(&aa, &bb);
    let ng = 0.5 * ngram_similarity(&aa, &bb, 2) + 0.5 * ngram_similarity(&aa, &bb, 3);
    let lcs = lcs_similarity(&aa, &bb);
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
