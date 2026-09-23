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
/// packs), and `--model-path` (directory holding `similarity-v5.json` /
/// `spam-v2.json` / `reranker-v1.json`; present-but-invalid artifacts fail
/// loudly).
pub fn build_engine(parsed: &ParsedArgs) -> Result<TextIntelligence, Box<dyn std::error::Error>> {
    let mut config = EngineConfig::default();
    if let Some(hints) = language_hints(parsed) {
        config.language_hints = hints;
    }
    if parsed.flag("no-rebus") {
        config.rebus = false;
    }
    let mut engine = if parsed.flag("production") {
        TextIntelligence::builder()
            .config(config)
            .production_local()
            .build()
            .map_err(|error| error.to_string())?
    } else {
        TextIntelligence::new(config)
    };
    if let Some(root) = parsed.value("resource-root") {
        engine = engine.with_resources(
            ResourceLoader::from_resource_root(root).map_err(|error| error.to_string())?,
        );
    }
    if let Some(dir) = parsed.value("model-path") {
        let similarity =
            textintel::engine::preferred_similarity_artifact_in(std::path::Path::new(dir));
        if similarity.is_file() {
            let source = std::fs::read_to_string(&similarity)?;
            let artifact =
                textintel::SimilarityModelArtifact::from_json(&source).map_err(|error| {
                    format!("invalid scorer artifact {}: {error}", similarity.display())
                })?;
            engine = engine.with_similarity_scorer(artifact.to_scorer());
        }
        let spam = textintel::engine::preferred_spam_artifact_in(std::path::Path::new(dir));
        if spam.is_file() {
            let source = std::fs::read_to_string(&spam)?;
            let artifact = textintel::SpamModelArtifact::from_json(&source)
                .map_err(|error| format!("invalid spam artifact {}: {error}", spam.display()))?;
            engine = engine.with_spam_predictor(artifact.to_predictor());
        }
        let reranker = std::path::Path::new(dir).join("reranker-v1.json");
        if reranker.is_file() {
            let source = std::fs::read_to_string(&reranker)?;
            let artifact =
                textintel::RerankerModelArtifact::from_json(&source).map_err(|error| {
                    format!("invalid reranker artifact {}: {error}", reranker.display())
                })?;
            engine = engine.with_reranker_provider(
                artifact
                    .to_reranker(64)
                    .map_err(|error| format!("invalid reranker weights: {error}"))?,
            );
        }
    }
    Ok(engine)
}
