//! Local production preset: resource packs, trained models when present,
//! espeak-ng G2P with a rule-based fallback, and graceful degradation notes.
//!
//! ```sh
//! cargo run --example production_local
//! ```

use textintel::TextIntelligence;

fn main() -> Result<(), textintel::TextIntelError> {
    let engine = TextIntelligence::production_local()?;
    let diagnostics = engine.diagnostics();
    println!("api: {}", diagnostics.api_version);
    println!("g2p: {}", diagnostics.g2p.provider);
    println!("embedding: {}", diagnostics.embedding.provider);
    println!("spam: {}", diagnostics.spam.provider);
    if diagnostics.degraded.is_empty() {
        println!("degraded: none");
    }
    for item in &diagnostics.degraded {
        println!(
            "degraded: {} (serving {}; want {})",
            item.capability, item.configured, item.wanted
        );
    }

    let comparison = engine.compare("Fra🏠do", "fracasado")?;
    println!("score(Fra🏠do, fracasado) = {:.3}", comparison.score);
    Ok(())
}
