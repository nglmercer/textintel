use std::cmp::Ordering;

use crate::normalization::unicode::casefold_text;

/// Canonical key used for matching. It preserves Unicode letters, digits,
/// and punctuation; it does not transliterate or strip accents.
pub fn normalize_key(value: &str) -> String {
    casefold_text(value.trim())
}

/// Deterministic index key: numeric runs first, then alphabetic runs, then
/// other Unicode characters. Numeric runs use natural ordering (`2` before
/// `10`) while leading zeroes remain a deterministic tie-breaker.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct IndexKey(String);

impl IndexKey {
    pub fn new(value: &str) -> Self {
        Self(normalize_key(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Ord for IndexKey {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_keys(&self.0, &other.0)
    }
}

impl PartialOrd for IndexKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn compare_keys(left: &str, right: &str) -> Ordering {
    let left = left.chars().collect::<Vec<_>>();
    let right = right.chars().collect::<Vec<_>>();
    let mut left_index = 0;
    let mut right_index = 0;

    while left_index < left.len() && right_index < right.len() {
        let left_digit = left[left_index].is_numeric();
        let right_digit = right[right_index].is_numeric();
        if left_digit && right_digit {
            let left_end = run_end(&left, left_index, true);
            let right_end = run_end(&right, right_index, true);
            let ordering =
                compare_numeric_runs(&left[left_index..left_end], &right[right_index..right_end]);
            if ordering != Ordering::Equal {
                return ordering;
            }
            left_index = left_end;
            right_index = right_end;
            continue;
        }

        let ordering = rank(left[left_index])
            .cmp(&rank(right[right_index]))
            .then_with(|| left[left_index].cmp(&right[right_index]));
        if ordering != Ordering::Equal {
            return ordering;
        }
        left_index += 1;
        right_index += 1;
    }
    left.len().cmp(&right.len())
}

fn run_end(value: &[char], start: usize, numeric: bool) -> usize {
    value
        .iter()
        .enumerate()
        .skip(start)
        .find(|(_, ch)| ch.is_numeric() != numeric)
        .map_or(value.len(), |(index, _)| index)
}

fn compare_numeric_runs(left: &[char], right: &[char]) -> Ordering {
    let left_trimmed = trim_leading_zeroes(left);
    let right_trimmed = trim_leading_zeroes(right);
    left_trimmed
        .len()
        .cmp(&right_trimmed.len())
        .then_with(|| left_trimmed.cmp(right_trimmed))
        .then_with(|| left.len().cmp(&right.len()).reverse())
        .then_with(|| left.cmp(right))
}

fn trim_leading_zeroes(value: &[char]) -> &[char] {
    let first = value
        .iter()
        .position(|ch| *ch != '0')
        .unwrap_or(value.len());
    &value[first..]
}

fn rank(ch: char) -> u8 {
    if ch.is_numeric() {
        0
    } else if ch.is_alphabetic() {
        1
    } else {
        2
    }
}
