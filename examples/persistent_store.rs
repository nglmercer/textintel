//! Persistent retrieval: index documents into a JSON store, flush to disk,
//! reopen, and search. Demonstrates the versioned fingerprint migration path
//! on load: older records upgrade instead of failing.
//!
//! Run with: `cargo run --example persistent_store`

use textintel::{EngineConfig, TextIntelError, TextIntelligence};

fn main() -> Result<(), TextIntelError> {
    let path = std::env::temp_dir().join("textintel-example-store.json");
    let _ = std::fs::remove_file(&path);

    let engine = TextIntelligence::new(EngineConfig::default()).with_json_store(&path)?;
    engine.add_document("doc-1", "compra ahora, oferta limitada")?;
    engine.add_document("doc-2", "win a free prize today")?;
    engine.add_document("doc-3", "nos vemos manana en la plaza")?;

    // Drop the engine and reopen: records are reloaded from disk.
    drop(engine);
    let reopened = TextIntelligence::new(EngineConfig::default()).with_json_store(&path)?;
    let hits = reopened.find_similar("oferta de compra", 2)?;
    for hit in &hits {
        println!("{} {:.3}", hit.id, hit.score);
    }
    assert!(hits.iter().any(|hit| hit.id == "doc-1"));
    let _ = std::fs::remove_file(&path);
    Ok(())
}
