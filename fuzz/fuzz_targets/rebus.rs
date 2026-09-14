#![no_main]

use libfuzzer_sys::fuzz_target;

use textintel::TextIntelligence;

fuzz_target!(|input: String| {
    let bounded: String = input.chars().take(512).collect();
    let engine = TextIntelligence::default();
    let _ = engine.decode(&bounded);
});
