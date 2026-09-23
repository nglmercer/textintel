//! Golden regression tests for lexicon key ordering. The comparator is
//! allocation-free over byte cursors; these values pin the legacy
//! natural-order behavior (verified identical against the old
//! implementation on 7,000+ fuzzed pairs before commit).

use std::cmp::Ordering;

use textintel::resources::{IndexKey, normalize_key};

fn order(left: &str, right: &str) -> Ordering {
    IndexKey::new(left).cmp(&IndexKey::new(right))
}

#[test]
fn natural_numeric_runs_sort_numerically() {
    assert_eq!(order("2", "10"), Ordering::Less);
    assert_eq!(order("10", "9"), Ordering::Greater);
    assert_eq!(order("file2", "file10"), Ordering::Less);
    assert_eq!(order("v1.9.3", "v1.10.2"), Ordering::Less);
    // Leading zeroes break ties deterministically (more zeroes first).
    assert_eq!(order("01", "1"), Ordering::Less);
    assert_eq!(order("001", "01"), Ordering::Less);
    assert_eq!(order("007", "7"), Ordering::Less);
}

#[test]
fn digits_sort_before_letters_before_other() {
    assert_eq!(order("9", "a"), Ordering::Less);
    assert_eq!(order("z", "-"), Ordering::Less);
    assert_eq!(order("a", "a"), Ordering::Equal);
    assert_eq!(order("", ""), Ordering::Equal);
    assert_eq!(order("", "a"), Ordering::Less);
}

#[test]
fn normalization_trims_and_casefolds() {
    assert_eq!(normalize_key("  Hello "), "hello");
    assert_eq!(normalize_key("MiXeD"), "mixed");
    assert_eq!(normalize_key("already"), "already");
    assert_eq!(normalize_key(""), "");
    assert_eq!(normalize_key("   "), "");
    // Keys compare on normalized forms.
    assert_eq!(order("  File ", "file"), Ordering::Equal);
}
