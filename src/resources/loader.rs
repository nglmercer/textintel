use std::fs;
use std::path::{Path, PathBuf};

use serde_json;

use super::error::ResourceError;
use super::index::{LanguageIndex, LexiconLookup, LexiconRecord, SymbolIndex};
use super::pack::{LanguagePack, SymbolPack, SUPPORTED_SCHEMA_VERSION};

include!(concat!(env!("OUT_DIR"), "/embedded_resources.rs"));

/// Data-driven loader and index for all language and symbol resources.
#[derive(Debug, Clone, Default)]
pub struct ResourceLoader {
    pub(crate) language_index: LanguageIndex,
    pub(crate) symbol_index: SymbolIndex,
}

impl ResourceLoader {
    /// Load every embedded language pack and the embedded symbol pack.
    /// Embedded data is a seed only; directory loading accepts any language
    /// pack that follows the schema.
    pub fn embedded() -> Result<Self, ResourceError> {
        let mut loader = Self::default();
        for (name, source) in LANGUAGE_PACKS {
            loader.load_language_json(source, PathBuf::from(format!("<embedded:{name}>")))?;
        }
        for (name, source) in SYMBOL_PACKS {
            loader.load_symbol_json(source, PathBuf::from(format!("<embedded:{name}>")))?;
        }
        Ok(loader)
    }

    /// Backward-compatible alias for the embedded seed set.
    pub fn common() -> Result<Self, ResourceError> {
        Self::embedded()
    }

    /// Load all JSON language packs below `path` recursively.
    pub fn from_directory(path: impl AsRef<Path>) -> Result<Self, ResourceError> {
        let mut loader = Self::default();
        loader.load_language_directory(path)?;
        Ok(loader)
    }

    /// Load `languages/` and, when present, `symbols/` below a resource root.
    pub fn from_resource_root(path: impl AsRef<Path>) -> Result<Self, ResourceError> {
        let root = path.as_ref();
        let language_dir = root.join("languages");
        let symbol_dir = root.join("symbols");
        let mut loader = Self::default();
        if language_dir.is_dir() {
            loader.load_language_directory(&language_dir)?;
        } else {
            loader.load_language_directory(root)?;
        }
        if symbol_dir.is_dir() {
            loader.load_symbol_directory(symbol_dir)?;
        }
        Ok(loader)
    }

    pub fn load_language_directory(&mut self, path: impl AsRef<Path>) -> Result<(), ResourceError> {
        let path = path.as_ref();
        let files = json_files(path)?;
        if files.is_empty() {
            return Err(ResourceError::Validation {
                path: path.to_path_buf(),
                message: "no JSON language packs found".to_string(),
            });
        }
        for file in files {
            self.load_language_file(file)?;
        }
        Ok(())
    }

    pub fn load_symbol_directory(&mut self, path: impl AsRef<Path>) -> Result<(), ResourceError> {
        let path = path.as_ref();
        for file in json_files(path)? {
            self.load_symbol_file(file)?;
        }
        Ok(())
    }

    pub fn load_language_file(&mut self, path: impl AsRef<Path>) -> Result<(), ResourceError> {
        let path = path.as_ref().to_path_buf();
        let source = fs::read_to_string(&path).map_err(|error| ResourceError::Io {
            path: path.clone(),
            message: error.to_string(),
        })?;
        self.load_language_json(&source, path)
    }

    pub fn load_symbol_file(&mut self, path: impl AsRef<Path>) -> Result<(), ResourceError> {
        let path = path.as_ref().to_path_buf();
        let source = fs::read_to_string(&path).map_err(|error| ResourceError::Io {
            path: path.clone(),
            message: error.to_string(),
        })?;
        self.load_symbol_json(&source, path)
    }

    pub fn add_language_pack(
        &mut self,
        pack: LanguagePack,
        source_path: impl AsRef<Path>,
    ) -> Result<(), ResourceError> {
        validate_language_pack(&pack, source_path.as_ref())?;
        self.language_index.add_pack(pack, source_path.as_ref());
        Ok(())
    }

    pub fn add_symbol_pack(
        &mut self,
        pack: SymbolPack,
        source_path: impl AsRef<Path>,
    ) -> Result<(), ResourceError> {
        validate_symbol_pack(&pack, source_path.as_ref())?;
        for symbol in pack.symbols {
            self.symbol_index.merge(symbol);
        }
        Ok(())
    }

    pub fn load_language_json(
        &mut self,
        source: &str,
        path: impl Into<PathBuf>,
    ) -> Result<(), ResourceError> {
        let path = path.into();
        let pack: LanguagePack =
            serde_json::from_str(source).map_err(|error| ResourceError::Parse {
                path: path.clone(),
                message: error.to_string(),
            })?;
        self.add_language_pack(pack, path)
    }

    pub fn load_symbol_json(
        &mut self,
        source: &str,
        path: impl Into<PathBuf>,
    ) -> Result<(), ResourceError> {
        let path = path.into();
        let pack: SymbolPack =
            serde_json::from_str(source).map_err(|error| ResourceError::Parse {
                path: path.clone(),
                message: error.to_string(),
            })?;
        self.add_symbol_pack(pack, path)
    }

    pub fn languages(&self) -> Vec<String> {
        self.language_index.languages()
    }

    pub fn language_count(&self) -> usize {
        self.language_index.language_count()
    }

    pub fn word_count(&self) -> usize {
        self.language_index.word_count()
    }

    pub fn record_count(&self) -> usize {
        self.language_index.record_count()
    }

    pub fn index_keys(&self) -> Vec<String> {
        self.language_index.index_keys()
    }

    pub fn symbol_count(&self) -> usize {
        self.symbol_index.len()
    }

    pub fn language_pack(&self, language: &str) -> Option<&LanguagePack> {
        self.language_index.language_pack(language)
    }

    pub fn language_packs(&self, language: &str) -> Vec<&LanguagePack> {
        self.language_index.language_packs(language)
    }

    pub fn lookup(&self, query: &str) -> LexiconLookup {
        self.language_index.lookup(query)
    }

    pub fn lookup_in_language(&self, query: &str, language: &str) -> LexiconLookup {
        self.language_index.lookup_in_language(query, language)
    }

    pub fn lookup_languages(&self, word: &str) -> Vec<String> {
        self.language_index.lookup_languages(word)
    }

    pub fn lookup_records(&self, word: &str) -> Vec<LexiconRecord> {
        self.lookup(word).matches
    }

    pub fn contains_in_language(&self, word: &str, language: &str) -> bool {
        self.language_index.contains_in_language(word, language)
    }

    pub fn detect_languages(&self, text: &str) -> Vec<crate::core::types::LanguageCandidate> {
        self.language_index.detect_languages(text)
    }
}

fn canonical_language(language: &str) -> String {
    language.trim().to_lowercase().replace('_', "-")
}

fn validate_language_pack(pack: &LanguagePack, path: &Path) -> Result<(), ResourceError> {
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

fn validate_symbol_pack(pack: &SymbolPack, path: &Path) -> Result<(), ResourceError> {
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

fn json_files(path: &Path) -> Result<Vec<PathBuf>, ResourceError> {
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
