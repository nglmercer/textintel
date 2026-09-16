//! Search recall regression: the bounded multi-channel candidate union
//! must preserve retrievable documents (§6, §10).
//!
//! Uses synthetic strings only. The gated production recall probe ranks by
//! exhaustive comparison, so these tests pin the store path instead: a noisy
//! shared token must not exhaust the union, distinctive evidence must still
//! retrieve its document, and every id must surface exactly once.

use textintel::{TextIntelError, TextIntelligence};

/// 550 fillers share one noisy token with the query; the target matches the
/// query exactly but sorts after every filler, so the shared token's capped
/// contribution excludes it and only its distinctive tokens retrieve it.
#[test]
fn union_preserves_exact_match_under_noisy_channel() -> Result<(), TextIntelError> {
    let engine = TextIntelligence::default();
    let query = "zebra xylophone quantum";
    for index in 0..550 {
        engine.add_document(
            format!("filler-{index:04}"),
            &format!("quantum filler {index} ledger"),
        )?;
    }
    engine.add_document("target-doc", query)?;

    let results = engine.find_similar(query, 10)?;
    assert_eq!(results.len(), 10, "union must fill the limit");
    assert_eq!(results[0].id, "target-doc", "exact match must rank first");
    assert!(
        results[0].score > 0.99,
        "exact match must score ~1.0: {}",
        results[0].score
    );
    assert!(
        results[0].candidate_count > 10,
        "union path must retrieve beyond the limit cut ({} candidates)",
        results[0].candidate_count
    );
    assert!(
        results[0]
            .retrieval_channels
            .contains(&"lexical".to_string()),
        "lexical channel must contribute: {:?}",
        results[0].retrieval_channels
    );
    Ok(())
}

#[test]
fn union_returns_each_id_once() -> Result<(), TextIntelError> {
    use textintel::core::providers::VectorStore;
    use textintel::storage::MemoryStore;

    // Few documents: the small-store path returns every record exactly once.
    let engine = TextIntelligence::default();
    let mut store = MemoryStore::default();
    for (id, text) in [
        ("a", "red apple orchard"),
        ("b", "red apple orchard harvest"),
        ("c", "unrelated ledger entry"),
    ] {
        store
            .upsert(id.to_string(), engine.analyze(text)?)
            .expect("upsert");
    }
    let query = engine.analyze("red apple orchard")?;
    let candidates = store.search_candidates(&query, 10);
    let mut ids: Vec<&str> = candidates.iter().map(|(id, _)| id.as_str()).collect();
    ids.sort_unstable();
    let mut deduped = ids.clone();
    deduped.dedup();
    assert_eq!(ids, deduped, "candidate ids must be deduplicated");
    assert_eq!(ids.len(), 3);
    Ok(())
}
