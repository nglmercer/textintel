#![no_main]

//! Storage-migration hardening: arbitrary bytes must produce a migrated
//! fingerprint or a typed error string, never a panic.
//! See `tests/fuzz_regression.rs`.

use libfuzzer_sys::fuzz_target;

use textintel::storage::migrate::migrate_fingerprint_bytes;

fuzz_target!(|input: &[u8]| {
    let bounded = &input[..input.len().min(65536)];
    let _ = migrate_fingerprint_bytes(bounded);
});
