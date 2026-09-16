//! Dataset split integrity: no pair/text/family may span splits.
//!
//! Guards the §9 contract: pair-level checks, text-level checks, and
//! duplicate-family grouping across train/validation/test.

use std::collections::{BTreeMap, BTreeSet};
use textintel::evaluation::EvaluationDataset;

fn dataset() -> EvaluationDataset {
    EvaluationDataset::from_dir("data/evaluation").expect("dataset must load")
}

#[test]
fn unordered_pairs_never_span_splits() {
    let dataset = dataset();
    let mut locations: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    for case in &dataset.cases {
        let mut pair = [case.a.clone(), case.b.clone()];
        pair.sort();
        locations
            .entry((pair[0].clone(), pair[1].clone()))
            .or_default()
            .insert(case.normalized_split().to_string());
    }
    let spanning: Vec<_> = locations
        .iter()
        .filter(|(_, splits)| splits.len() > 1)
        .take(10)
        .collect();
    assert!(
        spanning.is_empty(),
        "pairs spanning splits (leakage): {spanning:?}"
    );
}

#[test]
fn raw_texts_never_span_splits() {
    let dataset = dataset();
    let mut locations: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for case in &dataset.cases {
        let split = case.normalized_split();
        locations.entry(case.a.as_str()).or_default().insert(split);
        locations.entry(case.b.as_str()).or_default().insert(split);
    }
    let spanning: Vec<_> = locations
        .iter()
        .filter(|(_, splits)| splits.len() > 1)
        .take(10)
        .collect();
    assert!(
        spanning.is_empty(),
        "texts spanning splits (leakage): {spanning:?}"
    );
}

#[test]
fn duplicate_families_stay_within_one_split() {
    let dataset = dataset();
    let cases = &dataset.cases;
    let mut parent: Vec<usize> = (0..cases.len()).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    let mut text_owner: BTreeMap<&str, usize> = BTreeMap::new();
    for (index, case) in cases.iter().enumerate() {
        for text in [&case.a, &case.b] {
            if let Some(owner) = text_owner.get(text.as_str()) {
                let (left, right) = (find(&mut parent, *owner), find(&mut parent, index));
                parent[left] = right;
            } else {
                text_owner.insert(text.as_str(), index);
            }
        }
    }
    let mut family_splits: BTreeMap<usize, BTreeSet<&str>> = BTreeMap::new();
    for (index, case) in cases.iter().enumerate() {
        family_splits
            .entry(find(&mut parent, index))
            .or_default()
            .insert(case.normalized_split());
    }
    let spanning = family_splits
        .values()
        .filter(|splits| splits.len() > 1)
        .count();
    assert_eq!(spanning, 0, "families spanning splits (leakage)");
}

#[test]
fn splits_cover_every_case_exactly_once() {
    let dataset = dataset();
    assert!(!dataset.cases.is_empty());
    for case in &dataset.cases {
        assert!(
            ["train", "validation", "test"].contains(&case.normalized_split()),
            "case {} has bad split {:?}",
            case.id,
            case.split
        );
    }
    let (train, validation, test) = dataset.split_counts();
    assert!(train > 0 && validation > 0 && test > 0);
}
