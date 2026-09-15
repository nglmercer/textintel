#![no_main]

use libfuzzer_sys::fuzz_target;

use textintel::TextIntelligence;

fn engine() -> &'static TextIntelligence {
    static ENGINE: std::sync::OnceLock<TextIntelligence> = std::sync::OnceLock::new();
    ENGINE.get_or_init(TextIntelligence::default)
}

fuzz_target!(|input: String| {
    let bounded: String = input.chars().take(512).collect();
    let _ = engine().decode(&bounded);
});
