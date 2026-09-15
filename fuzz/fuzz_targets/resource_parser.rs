#![no_main]

//! Resource-pack parser hardening: arbitrary bytes must produce `Ok` or a
//! typed `ResourceError`, never a panic. See `tests/fuzz_regression.rs` for
//! the checked-in regression corpus.

use libfuzzer_sys::fuzz_target;

use textintel::ResourceLoader;

fuzz_target!(|input: &[u8]| {
    let Ok(text) = std::str::from_utf8(input) else {
        return;
    };
    let bounded: String = text.chars().take(4096).collect();
    let mut loader = ResourceLoader::with_limits(Default::default());
    let _ = loader.load_language_json(&bounded, std::path::PathBuf::from("<fuzz>"));
    let _ = loader.load_symbol_json(&bounded, std::path::PathBuf::from("<fuzz>"));
    let _ = loader.load_abbreviation_json(&bounded, std::path::PathBuf::from("<fuzz>"));
});
