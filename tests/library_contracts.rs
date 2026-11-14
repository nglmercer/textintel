//! Core-library boundaries: explicit providers, bounded work, no legacy autoload.
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use textintel::{EmbeddingProvider, EngineConfig, ProviderError, TextIntelligence};

#[test]
fn rejects_wrong_vector_count_before_processing_the_next_chunk() {
    struct BrokenProvider(Arc<AtomicUsize>);
    impl EmbeddingProvider for BrokenProvider {
        fn embed(&self, _: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(Vec::new())
        }
    }
    let calls = Arc::new(AtomicUsize::new(0));
    let engine = TextIntelligence::new(EngineConfig {
        semantic: true,
        rebus: false,
        embedding_batch_size: 1,
        ..Default::default()
    })
    .with_embedding_provider(BrokenProvider(calls.clone()));
    let error = engine.analyze("two distinct tokens").unwrap_err();
    assert!(error.to_string().contains("chunk inputs"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn removed_experiment_commands_and_flags_are_usage_errors() {
    let spec = textintel::cli::textintel_spec();
    for words in [
        vec!["generate", "hello"],
        vec!["classify", "hello"],
        vec!["decide", "request.json"],
        vec!["eval-decision"],
        vec!["decision-model-info"],
        vec!["analyze", "hello", "--model-path", "models"],
    ] {
        let args: Vec<_> = words.iter().map(|s| s.to_string()).collect();
        let error = textintel::cli::parse_args(spec, &args).unwrap_err();
        assert_eq!(error.exit_code(), 2, "{words:?}");
    }
}

#[test]
fn local_preset_ignores_legacy_model_files_and_environment() {
    let root = std::env::temp_dir().join(format!("textintel-library-only-{}", std::process::id()));
    let models = root.join("models");
    std::fs::create_dir_all(&models).unwrap();
    for name in ["similarity-v5.json", "spam-v2.json", "reranker-v1.json"] {
        std::fs::write(models.join(name), "invalid legacy artifact").unwrap();
    }
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_textintel"))
        .current_dir(&root)
        .env("TEXTINTEL_TRANSFORMER_MODEL", models.join("nonexistent"))
        .args(["diagnostics", "--production", "--json"])
        .output()
        .unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["providers"]["spam"]["provider"], "heuristic_spam_v1");
    assert_eq!(
        report["providers"]["embedding"]["provider"],
        "feature_hash_embedding"
    );
    assert!(report["providers"].get("similarity").is_none());
    assert!(report["providers"].get("generative").is_none());
    assert!(report["providers"].get("decision").is_none());
}
