#![no_main]

use libfuzzer_sys::fuzz_target;

use textintel::visual::unicode_features::analyze_unicode;

fuzz_target!(|input: String| {
    let _ = analyze_unicode(&input);
});
