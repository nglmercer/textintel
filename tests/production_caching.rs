//! Production caching: the preset enables bounded revision-aware caches for
//! embeddings, G2P, language detection, and rebus decoding, and diagnostics
//! expose counts without ever leaking cached texts.

use textintel::engine::TextIntelligence;

fn production() -> TextIntelligence {
    TextIntelligence::production_local().expect("production_local must not fail")
}

#[test]
fn production_preset_enables_all_four_caches() {
    let engine = production();
    assert!(engine.config().cache.any_enabled());
    let caches = &engine.diagnostics().caches;
    for name in ["embeddings", "g2p", "language", "rebus"] {
        let cache = caches
            .get(name)
            .unwrap_or_else(|| panic!("missing {name} cache diagnostics"));
        assert!(cache.enabled, "{name} cache must be enabled");
        assert!(cache.capacity > 0, "{name} cache must be bounded");
        assert!(
            !cache.revision.is_empty(),
            "{name} cache must carry a revision"
        );
    }
    // The default engine enables only the decision fingerprint cache.
    let plain = TextIntelligence::default();
    assert_eq!(plain.config().cache.decision, 256);
    for (name, cache) in &plain.diagnostics().caches {
        assert_eq!(
            cache.enabled,
            name == "decision",
            "only the decision cache is on by default (failed on {name})"
        );
    }
}

#[test]
fn repeated_analysis_hits_every_cache() {
    let engine = production();
    let first = engine.analyze("Fra🏠do salU2").unwrap();
    let second = engine.analyze("Fra🏠do salU2").unwrap();
    assert_eq!(first, second, "cache hits must return clones");
    let caches = &engine.diagnostics().caches;
    for name in ["embeddings", "g2p", "language", "rebus"] {
        let cache = &caches[name];
        assert!(cache.entries > 0, "{name} cache should hold entries");
        assert!(cache.hits > 0, "{name} cache should hit on repeat");
    }
}

#[test]
fn repeated_decode_hits_the_rebus_cache() {
    let engine = production();
    let first = engine.decode("Fr4🏠d0").unwrap();
    assert!(!first.is_empty());
    let second = engine.decode("Fr4🏠d0").unwrap();
    assert_eq!(first, second);
    let rebus = &engine.diagnostics().caches["rebus"];
    assert!(rebus.entries > 0);
    assert!(rebus.hits > 0);
}

#[test]
fn diagnostics_never_expose_cached_text() {
    let engine = production();
    let secret = "supercalifragilistic-expialidocious-zzz";
    engine.analyze(secret).unwrap();
    engine.decode(secret).unwrap();
    let json = serde_json::to_string(&engine.diagnostics().caches).unwrap();
    assert!(
        !json.contains("supercalifragilistic"),
        "cache diagnostics leaked user text: {json}"
    );
}

#[test]
fn provider_swaps_invalidate_the_rebus_cache() {
    let engine = production();
    engine.decode("Fr4🏠d0").unwrap();
    assert!(engine.diagnostics().caches["rebus"].entries > 0);
    // A lexicon swap changes decoding behavior without changing the resource
    // revision, so the engine must invalidate explicitly.
    let swapped = engine.with_lexicon_provider(textintel::resources::DefaultLexiconProvider);
    let rebus = &swapped.diagnostics().caches["rebus"];
    assert!(rebus.enabled);
    assert_eq!(rebus.entries, 0);
    assert!(rebus.invalidations > 0);
}
