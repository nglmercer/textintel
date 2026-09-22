use std::cmp::Ordering;

use crate::normalization::unicode::casefold_text;

/// Canonical key used for matching. It preserves Unicode letters, digits,
/// and punctuation; it does not transliterate or strip accents.
pub fn normalize_key(value: &str) -> String {
    // Fast path: trimmed lowercase ASCII casefolds to itself, so skip
    // the Unicode machinery. Correctness: within ASCII, `trim` only
    // strips ASCII whitespace (checked at both ends) and casefolding
    // only maps A-Z (absent by the scan below).
    if is_normalized_ascii(value) {
        return value.to_string();
    }
    casefold_text(value.trim())
}

fn is_normalized_ascii(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return true;
    }
    if bytes[0].is_ascii_whitespace() || bytes[bytes.len() - 1].is_ascii_whitespace() {
        return false;
    }
    bytes
        .iter()
        .all(|byte| byte.is_ascii() && !byte.is_ascii_uppercase())
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
    if left == right {
        return Ordering::Equal;
    }
    // Byte cursors that only ever rest on char boundaries, plus consumed
    // char counts for the trailing length comparison. No allocation;
    // every comparison below matches the old char-vector algorithm.
    let mut left_cursor = 0usize;
    let mut right_cursor = 0usize;
    let mut left_chars = 0usize;
    let mut right_chars = 0usize;
    while left_cursor < left.len() && right_cursor < right.len() {
        let left_ch = char_at(left, left_cursor);
        let right_ch = char_at(right, right_cursor);
        if left_ch.is_numeric() && right_ch.is_numeric() {
            let (left_end, left_run) = numeric_run_end(left, left_cursor);
            let (right_end, right_run) = numeric_run_end(right, right_cursor);
            let ordering = compare_numeric_runs_str(
                &left[left_cursor..left_end],
                &right[right_cursor..right_end],
            );
            if ordering != Ordering::Equal {
                return ordering;
            }
            left_chars += left_run;
            right_chars += right_run;
            left_cursor = left_end;
            right_cursor = right_end;
            continue;
        }

        let ordering = rank(left_ch)
            .cmp(&rank(right_ch))
            .then_with(|| left_ch.cmp(&right_ch));
        if ordering != Ordering::Equal {
            return ordering;
        }
        left_chars += 1;
        right_chars += 1;
        left_cursor += left_ch.len_utf8();
        right_cursor += right_ch.len_utf8();
    }
    left_chars += left[left_cursor..].chars().count();
    right_chars += right[right_cursor..].chars().count();
    left_chars.cmp(&right_chars)
}

fn char_at(value: &str, byte: usize) -> char {
    value[byte..]
        .chars()
        .next()
        .expect("cursor rests on a char boundary")
}

/// Byte end plus char length of the maximal numeric run at `start`.
/// `start` must be a char boundary at a numeric char.
fn numeric_run_end(value: &str, start: usize) -> (usize, usize) {
    let mut end = start;
    let mut length = 0usize;
    for character in value[start..].chars() {
        if !character.is_numeric() {
            break;
        }
        end += character.len_utf8();
        length += 1;
    }
    (end, length)
}

fn compare_chars(left: &str, right: &str) -> Ordering {
    left.chars().cmp(right.chars())
}

fn compare_numeric_runs_str(left: &str, right: &str) -> Ordering {
    // Both sides are maximal numeric runs. Only ASCII '0' trims, exactly
    // as before; lengths are char counts, not bytes.
    let left_trimmed = left.trim_start_matches('0');
    let right_trimmed = right.trim_start_matches('0');
    left_trimmed
        .chars()
        .count()
        .cmp(&right_trimmed.chars().count())
        .then_with(|| compare_chars(left_trimmed, right_trimmed))
        .then_with(|| left.chars().count().cmp(&right.chars().count()).reverse())
        .then_with(|| compare_chars(left, right))
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
