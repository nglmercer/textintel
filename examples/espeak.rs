//! Production G2P through a local `espeak-ng` binary, with graceful fallback.
//!
//! ```sh
//! cargo run --features phonetic-espeak --example espeak
//! ```
#![cfg(feature = "phonetic-espeak")]

use textintel::core::providers::G2PProvider;
use textintel::phonetic::EspeakNgG2PProvider;

fn main() {
    let provider = match EspeakNgG2PProvider::auto_detect() {
        Ok(provider) => provider,
        Err(error) => {
            println!("espeak-ng unavailable ({error}); nothing to demo offline.");
            return;
        }
    };
    match provider.installed_voices() {
        Ok(voices) => println!("installed voices: {}", voices.len()),
        Err(error) => println!("cannot list voices: {error}"),
    }
    for (text, language) in [("hola", "es"), ("hello", "en"), ("bonjour", "fr")] {
        match provider.phonemize(text, language) {
            Ok(candidate) => println!(
                "{text} [{language}] voice={} ipa={:?} syllables={} stress={:?} confidence={}",
                candidate.dialect.as_deref().unwrap_or("?"),
                candidate.ipa,
                candidate.syllables,
                candidate.stress,
                candidate.confidence
            ),
            Err(error) => println!("{text} [{language}] failed: {error}"),
        }
    }
}
