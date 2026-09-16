//! Resource-pack validation (sizes, hashes, schemas, file discovery).
//! Pure helpers behind [`ResourceLoader`](super::ResourceLoader).

use std::fs;
use std::path::{Path, PathBuf};

use serde_json;
use sha2::{Digest, Sha256};

use super::super::error::ResourceError;
use super::super::pack::{AbbreviationPack, LanguagePack, SymbolPack, SUPPORTED_SCHEMA_VERSION};

pub(crate) fn canonical_language(language: &str) -> String {
    language.trim().to_lowercase().replace('_', "-")
}

pub(crate) fn validate_file_size(path: &Path, maximum: usize) -> Result<(), ResourceError> {
    let metadata = fs::metadata(path).map_err(|error| ResourceError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    if metadata.len() > maximum as u64 {
        return Err(ResourceError::Validation {
            path: path.to_path_buf(),
            message: format!("resource is {} bytes; maximum is {maximum}", metadata.len()),
        });
    }
    Ok(())
}

pub(crate) fn validate_declared_hash(
    expected: Option<&str>,
    source: &str,
    path: &Path,
) -> Result<(), ResourceError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let expected = expected.trim().to_ascii_lowercase();
    if expected.len() != 64
        || !expected
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(ResourceError::Validation {
            path: path.to_path_buf(),
            message: "sha256 must be a 64-character hexadecimal string".to_string(),
        });
    }
    // The declaration is part of the JSON document, so hashing the raw bytes
    // would be self-referential. Hash canonical JSON with the declaration
    // removed instead; this also makes formatting and object-key order stable.
    let mut document: serde_json::Value =
        serde_json::from_str(source).map_err(|error| ResourceError::Parse {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
    let Some(object) = document.as_object_mut() else {
        return Err(ResourceError::Validation {
            path: path.to_path_buf(),
            message: "resource root must be a JSON object".to_string(),
        });
    };
    object.remove("sha256");
    let canonical = serde_json::to_vec(&document).map_err(|error| ResourceError::Parse {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    let digest = Sha256::digest(canonical);
    let actual = format!("{digest:x}");
    if actual != expected {
        return Err(ResourceError::Validation {
            path: path.to_path_buf(),
            message: format!("sha256 mismatch: expected {expected}, got {actual}"),
        });
    }
    Ok(())
}

pub(crate) fn validate_language_pack(
    pack: &LanguagePack,
    path: &Path,
) -> Result<(), ResourceError> {
    if pack.schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(ResourceError::Validation {
            path: path.to_path_buf(),
            message: format!("unsupported schema_version={}", pack.schema_version),
        });
    }
    if canonical_language(&pack.language).is_empty() {
        return Err(ResourceError::Validation {
            path: path.to_path_buf(),
            message: "language cannot be empty".to_string(),
        });
    }
    for entry in &pack.entries {
        if entry.word.trim().is_empty() {
            return Err(ResourceError::Validation {
                path: path.to_path_buf(),
                message: "lexicon words cannot be empty".to_string(),
            });
        }
        if !entry.weight.is_finite() || entry.weight <= 0.0 {
            return Err(ResourceError::Validation {
                path: path.to_path_buf(),
                message: format!("invalid weight for word {:?}", entry.word),
            });
        }
    }
    for word in pack.words.iter().chain(pack.stop_words.iter()) {
        if word.trim().is_empty() {
            return Err(ResourceError::Validation {
                path: path.to_path_buf(),
                message: "lexicon words cannot be empty".to_string(),
            });
        }
    }
    Ok(())
}

pub(crate) fn abbreviation_language_allowed(
    language: Option<&str>,
    languages: Option<&[String]>,
) -> bool {
    let Some(languages) = languages.filter(|values| !values.is_empty()) else {
        return true;
    };
    let Some(language) = language else {
        return true;
    };
    language == "und"
        || languages.iter().any(|candidate| {
            candidate.eq_ignore_ascii_case(language)
                || candidate.eq_ignore_ascii_case("unknown")
                || candidate.eq_ignore_ascii_case("und")
        })
}

pub(crate) fn validate_abbreviation_pack(
    pack: &AbbreviationPack,
    path: &Path,
) -> Result<(), ResourceError> {
    if pack.schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(ResourceError::Validation {
            path: path.to_path_buf(),
            message: format!("unsupported schema_version={}", pack.schema_version),
        });
    }
    if canonical_language(&pack.language).is_empty() {
        return Err(ResourceError::Validation {
            path: path.to_path_buf(),
            message: "abbreviation pack language cannot be empty".to_string(),
        });
    }
    for entry in &pack.entries {
        if entry.token.trim().is_empty() {
            return Err(ResourceError::Validation {
                path: path.to_path_buf(),
                message: "abbreviation tokens cannot be empty".to_string(),
            });
        }
        for reading in &entry.readings {
            if reading.text.trim().is_empty()
                || reading.kind.trim().is_empty()
                || !reading.probability.is_finite()
                || !(0.0..=1.0).contains(&reading.probability)
            {
                return Err(ResourceError::Validation {
                    path: path.to_path_buf(),
                    message: format!("invalid reading for abbreviation {:?}", entry.token),
                });
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_symbol_pack(pack: &SymbolPack, path: &Path) -> Result<(), ResourceError> {
    if pack.schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(ResourceError::Validation {
            path: path.to_path_buf(),
            message: format!("unsupported schema_version={}", pack.schema_version),
        });
    }
    for symbol in &pack.symbols {
        if symbol.token.is_empty() {
            return Err(ResourceError::Validation {
                path: path.to_path_buf(),
                message: "symbol tokens cannot be empty".to_string(),
            });
        }
        for reading in &symbol.readings {
            if reading.text.trim().is_empty()
                || !reading.probability.is_finite()
                || reading.probability < 0.0
            {
                return Err(ResourceError::Validation {
                    path: path.to_path_buf(),
                    message: format!("invalid reading for symbol {:?}", symbol.token),
                });
            }
        }
        for concept in &symbol.concepts {
            if concept.id.trim().is_empty()
                || !concept.probability.is_finite()
                || concept.probability < 0.0
            {
                return Err(ResourceError::Validation {
                    path: path.to_path_buf(),
                    message: format!("invalid concept for symbol {:?}", symbol.token),
                });
            }
        }
    }
    Ok(())
}

pub(crate) fn json_files(path: &Path) -> Result<Vec<PathBuf>, ResourceError> {
    if !path.is_dir() {
        return Err(ResourceError::Validation {
            path: path.to_path_buf(),
            message: "resource directory does not exist".to_string(),
        });
    }
    let mut files = Vec::new();
    collect_json_files(path, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_json_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), ResourceError> {
    let entries = fs::read_dir(path).map_err(|error| ResourceError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| ResourceError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        let entry_path = entry.path();
        if entry
            .file_type()
            .map_err(|error| ResourceError::Io {
                path: entry_path.clone(),
                message: error.to_string(),
            })?
            .is_dir()
        {
            collect_json_files(&entry_path, files)?;
        } else if entry_path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        {
            files.push(entry_path);
        }
    }
    Ok(())
}
