#![no_main]

//! IPA parser hardening: any UTF-8 string must tokenize without panicking,
//! and every token must be non-empty. See `tests/fuzz_regression.rs`.

use libfuzzer_sys::fuzz_target;

use textintel::phonetic::ipa::parse_ipa;

fuzz_target!(|input: String| {
    let bounded: String = input.chars().take(2048).collect();
    let tokens = parse_ipa(&bounded);
    for token in &tokens {
        assert!(!token.is_empty());
    }
});
