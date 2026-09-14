//! Versioned persistence for registered [`Pattern`] definitions.
//!
//! [`Pattern`]s added through [`TextIntelligence::add_pattern`] otherwise
//! live only in memory. This module snapshots the *definitions* (ids,
//! examples, thresholds, tags) to a versioned JSON envelope; example
//! fingerprints are re-analyzed on load instead of stored, so pattern files
//! never couple to the fingerprint schema and always reflect the loading
//! engine's configuration.
//!
//! [`Pattern`]: crate::core::types::Pattern
//! [`TextIntelligence::add_pattern`]: crate::engine::TextIntelligence::add_pattern

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::core::types::Pattern;

/// Schema version of the pattern envelope written by this build.
pub const PATTERN_STORE_SCHEMA_VERSION: u32 = 1;

/// Largest pattern file accepted on load (refuses oversized input before
/// parsing the whole payload into memory twice).
pub const DEFAULT_MAX_PATTERNS_FILE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
struct PatternEnvelope {
    schema_version: u32,
    patterns: BTreeMap<String, Pattern>,
}

/// Validate one pattern definition with the same rules as engine
/// registration: non-empty id, at least one example, finite threshold in
/// `0.0..=1.0`.
pub fn validate_pattern(pattern: &Pattern) -> Result<(), String> {
    if pattern.id.trim().is_empty() {
        return Err("pattern id must not be empty".to_string());
    }
    if pattern.examples.is_empty() {
        return Err(format!(
            "pattern {:?} must declare at least one example",
            pattern.id
        ));
    }
    if !pattern.threshold.is_finite() || !(0.0..=1.0).contains(&pattern.threshold) {
        return Err(format!(
            "pattern {:?} threshold must be between 0 and 1",
            pattern.id
        ));
    }
    Ok(())
}

/// Write pattern definitions to `path` atomically (temp file + rename).
pub fn save_patterns_to(path: &Path, patterns: &[Pattern]) -> Result<(), String> {
    let mut records = BTreeMap::new();
    for pattern in patterns {
        validate_pattern(pattern)?;
        if records.contains_key(&pattern.id) {
            return Err(format!("duplicate pattern id {:?}", pattern.id));
        }
        records.insert(pattern.id.clone(), pattern.clone());
    }
    let envelope = PatternEnvelope {
        schema_version: PATTERN_STORE_SCHEMA_VERSION,
        patterns: records,
    };
    let bytes = serde_json::to_vec_pretty(&envelope).map_err(|error| error.to_string())?;
    if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temporary = path.with_extension("patterns.tmp");
    fs::write(&temporary, &bytes).map_err(|error| error.to_string())?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(path);
        fs::rename(&temporary, path).map_err(|_| error.to_string())?;
    }
    Ok(())
}

/// Load pattern definitions from `path`, checking the envelope version and
/// validating every record before returning.
pub fn load_patterns_from(path: &Path) -> Result<Vec<Pattern>, String> {
    load_patterns_from_with_limit(path, DEFAULT_MAX_PATTERNS_FILE_BYTES)
}

fn load_patterns_from_with_limit(path: &Path, max_file_bytes: u64) -> Result<Vec<Pattern>, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > max_file_bytes {
        return Err(format!(
            "pattern file exceeds max_file_bytes={max_file_bytes}"
        ));
    }
    let source = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let envelope: PatternEnvelope = serde_json::from_str(&source)
        .map_err(|error| format!("invalid pattern file {}: {error}", path.display()))?;
    if envelope.schema_version != PATTERN_STORE_SCHEMA_VERSION {
        return Err(format!(
            "unsupported pattern schema_version={}",
            envelope.schema_version
        ));
    }
    let mut patterns = Vec::with_capacity(envelope.patterns.len());
    for (key, pattern) in envelope.patterns {
        if key != pattern.id {
            return Err(format!(
                "pattern key {key:?} does not match record id {:?}",
                pattern.id
            ));
        }
        validate_pattern(&pattern)?;
        patterns.push(pattern);
    }
    Ok(patterns)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::Pattern;

    fn sample(id: &str) -> Pattern {
        Pattern {
            id: id.to_string(),
            examples: vec!["buy now".to_string(), "limited offer".to_string()],
            negative_examples: vec!["buy milk".to_string()],
            threshold: 0.6,
            languages: vec!["en".to_string()],
            tags: vec!["spam".to_string()],
            enabled_channels: Vec::new(),
        }
    }

    fn test_path(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "textintel-patterns-{name}-{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn round_trip_preserves_definitions() {
        let path = test_path("roundtrip");
        let patterns = vec![sample("promo"), sample("prize")];
        save_patterns_to(&path, &patterns).unwrap();
        let mut loaded = load_patterns_from(&path).unwrap();
        loaded.sort_by(|left, right| left.id.cmp(&right.id));
        let mut expected = patterns.clone();
        expected.sort_by(|left, right| left.id.cmp(&right.id));
        assert_eq!(loaded, expected);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn wrong_schema_version_is_rejected() {
        let path = test_path("version");
        save_patterns_to(&path, &[sample("promo")]).unwrap();
        let mut envelope: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        envelope["schema_version"] = serde_json::json!(PATTERN_STORE_SCHEMA_VERSION + 1);
        std::fs::write(&path, serde_json::to_string_pretty(&envelope).unwrap()).unwrap();
        let error = load_patterns_from(&path).unwrap_err();
        assert!(error.contains("unsupported pattern schema_version"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn invalid_records_are_rejected() {
        let path = test_path("invalid");
        let mut bad = sample("bad");
        bad.threshold = 2.0;
        let error = save_patterns_to(&path, &[bad]).unwrap_err();
        assert!(error.contains("threshold"));

        let mut empty = sample("empty");
        empty.examples.clear();
        assert!(save_patterns_to(&path, &[empty]).is_err());
        assert!(!path.exists());

        let duplicate = vec![sample("promo"), sample("promo")];
        assert!(save_patterns_to(&path, &duplicate).is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_and_oversized_files_fail() {
        let missing = test_path("missing");
        assert!(load_patterns_from(&missing).is_err());

        let path = test_path("oversized");
        save_patterns_to(&path, &[sample("promo")]).unwrap();
        let error = load_patterns_from_with_limit(&path, 8).unwrap_err();
        assert!(error.contains("max_file_bytes"));
        let _ = std::fs::remove_file(&path);
    }
}
