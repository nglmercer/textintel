//! Local Liquid LFM2.5 generation through an explicit endpoint.
//!
//! Start a server first (nothing is downloaded or started for you):
//!
//! ```sh
//! llama-server -hf LiquidAI/LFM2.5-230M-GGUF:Q4_K_M -c 2048
//! cargo run --example liquid_generate
//! ```

use textintel::{
    GenerationOptions, LIQUID_DEFAULT_MODEL, LiquidInstructProvider, TextIntelligence,
};

fn main() -> Result<(), textintel::TextIntelError> {
    let endpoint = std::env::var("TEXTINTEL_LIQUID_ENDPOINT")
        .unwrap_or_else(|_| textintel::LIQUID_DEFAULT_ENDPOINT.to_string());
    let provider = LiquidInstructProvider::new(&endpoint, LIQUID_DEFAULT_MODEL)
        .map_err(|error| textintel::TextIntelError::InvalidConfiguration(error.to_string()))?;
    println!("endpoint: {}", provider.endpoint());
    println!("model: {}", provider.model());
    let engine = TextIntelligence::default().with_generative_provider(provider);
    match engine.generate(
        "Explain in one sentence why local-first AI matters.",
        &GenerationOptions::default().with_max_tokens(64),
    ) {
        Ok(generated) => {
            println!("---\n{}", generated.text);
            println!(
                "(model {}, prompt/completion tokens {:?}/{:?})",
                generated.model, generated.prompt_tokens, generated.completion_tokens
            );
        }
        Err(error) => {
            println!("generation failed ({error}); is the local server running?");
            println!("hint: llama-server -hf LiquidAI/LFM2.5-230M-GGUF:Q4_K_M -c 2048");
        }
    }
    Ok(())
}
