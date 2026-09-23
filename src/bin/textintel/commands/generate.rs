//! The `generate` command: one completion from an explicit local model.

use textintel::cli::ParsedArgs;

use super::common::{build_engine, print_json};

pub fn generate(parsed: &ParsedArgs) -> Result<i32, Box<dyn std::error::Error>> {
    let prompt = parsed.positional(0).ok_or("generate requires <prompt>")?;
    let endpoint = parsed
        .value("liquid-endpoint")
        .unwrap_or(textintel::LIQUID_DEFAULT_ENDPOINT)
        .to_string();
    let model = parsed
        .value("liquid-model")
        .unwrap_or(textintel::LIQUID_DEFAULT_MODEL)
        .to_string();
    let mut options = textintel::GenerationOptions::default();
    if let Some(max_tokens) = parsed.value("max-tokens") {
        options = options.with_max_tokens(max_tokens.parse::<u32>()?);
    }
    if let Some(temperature) = parsed.value("temperature") {
        options = options.with_temperature(temperature.parse::<f32>()?);
    }
    if let Some(system) = parsed.value("system") {
        options = options.with_system_prompt(system);
    }
    let provider = textintel::LiquidInstructProvider::new(&endpoint, &model)
        .map_err(|error| error.to_string())?;
    let engine = build_engine(parsed)?.with_generative_provider(provider);
    let generated = engine.generate(prompt, &options)?;
    if parsed.flag("json") {
        print_json(&serde_json::json!({
            "text": generated.text,
            "model": generated.model,
            "prompt_tokens": generated.prompt_tokens,
            "completion_tokens": generated.completion_tokens,
        }))?;
    } else {
        println!("{}", generated.text);
    }
    Ok(0)
}
