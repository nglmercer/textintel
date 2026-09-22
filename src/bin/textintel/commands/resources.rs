//! Introspection commands: `resources`, `diagnostics`, `provider-info`,
//! `schema-version`.

use textintel::ResourceLoader;
use textintel::cli::ParsedArgs;

use super::common::{build_engine, print_json};

pub fn resources(parsed: &ParsedArgs) -> Result<i32, Box<dyn std::error::Error>> {
    let json = parsed.flag("json");
    if parsed.positional(0) == Some("validate") {
        let path = parsed
            .positional(1)
            .ok_or("resources validate requires <path>")?;
        let mut loader = ResourceLoader::with_limits(Default::default());
        let result = loader
            .load_language_file(path)
            .or_else(|_| loader.load_symbol_file(path));
        let report = match result {
            Ok(()) => serde_json::json!({"path": path, "valid": true, "issues": []}),
            Err(error) => {
                serde_json::json!({"path": path, "valid": false, "issues": [error.to_string()]})
            }
        };
        if json {
            print_json(&report)?;
        } else {
            println!("valid: {}", report["valid"]);
            if let Some(issues) = report["issues"].as_array() {
                for issue in issues {
                    println!("- {issue}");
                }
            }
        }
        return Ok(0);
    }
    let root = parsed.positional(0).unwrap_or("resources");
    let resources = ResourceLoader::from_resource_root(root)?;
    let report = serde_json::json!({
        "languages": resources.languages(),
        "language_count": resources.language_count(),
        "word_count": resources.word_count(),
        "symbol_count": resources.symbol_count(),
        "symbol_tokens": resources.symbol_tokens(),
    });
    if json {
        print_json(&report)?;
    } else {
        println!("languages: {:?}", resources.languages());
        println!("words: {}", resources.word_count());
        println!("symbols: {}", resources.symbol_count());
    }
    Ok(0)
}

pub fn diagnostics(parsed: &ParsedArgs) -> Result<i32, Box<dyn std::error::Error>> {
    let engine = build_engine(parsed)?;
    let diagnostics = engine.diagnostics();
    let report = serde_json::json!({
        "api_version": textintel::API_VERSION,
        "fingerprint_schema_version": textintel::FINGERPRINT_SCHEMA_VERSION,
        "providers": engine.provider_capabilities(),
        "embedding_model": diagnostics.embedding_model,
        "ann_enabled": diagnostics.ann_enabled,
        "store_capabilities": diagnostics.store_capabilities,
        "candidate_budgets": diagnostics.candidate_budgets,
        "caches": diagnostics.caches,
        "degraded": diagnostics.degraded,
        "symbol_languages": diagnostics.symbol_languages,
        "abbreviation_languages": diagnostics.abbreviation_languages,
    });
    if parsed.flag("json") {
        print_json(&report)?;
    } else {
        println!("api: {}", textintel::API_VERSION);
        println!("providers: {:?}", engine.provider_capabilities());
        println!("embedding_model: {:?}", diagnostics.embedding_model);
        println!("ann_enabled: {}", diagnostics.ann_enabled);
        println!("store: {:?}", diagnostics.store_capabilities);
        println!("candidate_budgets: {:?}", diagnostics.candidate_budgets);
        println!("caches: {:?}", diagnostics.caches);
        for item in &diagnostics.degraded {
            println!(
                "degraded: {} (serving {}; want {}): {}",
                item.capability, item.configured, item.wanted, item.detail
            );
        }
    }
    Ok(0)
}

pub fn schema_version(parsed: &ParsedArgs) -> Result<i32, Box<dyn std::error::Error>> {
    let report = serde_json::json!({
        "api_version": textintel::API_VERSION,
        "fingerprint_schema_version": textintel::FINGERPRINT_SCHEMA_VERSION,
    });
    if parsed.flag("json") {
        print_json(&report)?;
    } else {
        println!("api: {}", textintel::API_VERSION);
        println!(
            "fingerprint_schema: {}",
            textintel::FINGERPRINT_SCHEMA_VERSION
        );
    }
    Ok(0)
}
