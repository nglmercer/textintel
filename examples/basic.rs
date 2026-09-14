//! Basic analysis: fingerprint, compare, and rebus-decode one message.
//!
//! Run with: `cargo run --example basic`

use textintel::{TextIntelError, TextIntelligence};

fn main() -> Result<(), TextIntelError> {
    let engine = TextIntelligence::default();

    let fingerprint = engine.analyze("Fra🏠do")?;
    println!("raw: {}", fingerprint.raw);
    println!("top language: {:?}", fingerprint.top_language());
    println!("segments: {}", fingerprint.segments.len());

    let comparison = engine.compare("c0mpr4 ah0r4", "compra ahora")?;
    println!("similarity score: {:.3}", comparison.score);

    for candidate in engine.decode("salU2")?.iter().take(3) {
        println!("decode: {} ({:.3})", candidate.text, candidate.score);
    }
    Ok(())
}
