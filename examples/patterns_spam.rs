//! Patterns and spam: register a pattern, persist it to a versioned file,
//! reload it into a fresh engine, and score a message.
//!
//! Run with: `cargo run --example patterns_spam`

use textintel::{TextIntelError, TextIntelligence};

fn main() -> Result<(), TextIntelError> {
    let engine = TextIntelligence::default();
    engine.add_pattern(
        "promo",
        vec![
            "win a free prize".to_string(),
            "claim your reward now".to_string(),
        ],
    )?;

    let path = std::env::temp_dir().join("textintel-example-patterns.json");
    let _ = std::fs::remove_file(&path);
    engine.save_patterns_to(&path)?;

    let restored = TextIntelligence::default();
    let count = restored.load_patterns_from(&path)?;
    println!("loaded {count} pattern(s)");
    assert_eq!(restored.pattern_definitions()?.len(), 1);

    let spam = restored.detect_spam("win a free prize today, claim now")?;
    println!(
        "spam probability: {:.3} labels: {:?}",
        spam.probability, spam.labels
    );
    let ham = restored.detect_spam("nos vemos manana en la plaza")?;
    println!("ham probability: {:.3}", ham.probability);
    assert!(spam.probability >= ham.probability);
    let _ = std::fs::remove_file(&path);
    Ok(())
}
