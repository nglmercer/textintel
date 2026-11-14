//! Shared command plumbing: JSON printing, engine construction, and
//! language hints, all read from [`ParsedArgs`] by spec id.
//!
//! [`ParsedArgs`]: textintel::cli::ParsedArgs

use textintel::cli::ParsedArgs;
use textintel::{EngineConfig, ResourceLoader, TextIntelligence};

pub fn print_json<T: serde::Serialize>(value: &T) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

/// Language hints from `--languages <a,b>` or `--language <code>`.
pub fn language_hints(parsed: &ParsedArgs) -> Option<Vec<String>> {
    parsed
        .value("languages")
        .or_else(|| parsed.value("language"))
        .map(|value| {
            value
                .split(',')
                .map(|part| part.trim().to_string())
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|values| !values.is_empty())
}

/// Shared engine construction: default or `--production` preset, then
/// `--language` hints (decode/G2P preference), `--resource-root` (custom
/// packs).
pub fn build_engine(parsed: &ParsedArgs) -> Result<TextIntelligence, Box<dyn std::error::Error>> {
    let mut config = EngineConfig::default();
    if let Some(hints) = language_hints(parsed) {
        config.language_hints = hints;
    }
    let mut engine = if parsed.flag("production") {
        TextIntelligence::builder()
            .config(config)
            .production_local()
            .build()
            .map_err(|error| error.to_string())?
    } else {
        TextIntelligence::try_new(config)?
    };
    if let Some(root) = parsed.value("resource-root") {
        engine = engine.with_resources(
            ResourceLoader::from_resource_root(root).map_err(|error| error.to_string())?,
        );
    }
    Ok(engine)
}
