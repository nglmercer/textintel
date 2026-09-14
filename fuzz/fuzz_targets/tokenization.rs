#![no_main]

use libfuzzer_sys::fuzz_target;

use textintel::language::segmentation::segment_message;
use textintel::lexical::tokenizer::tokenize;

fuzz_target!(|input: String| {
    let _ = segment_message(&input, 256);
    let _ = tokenize(&input);
});
