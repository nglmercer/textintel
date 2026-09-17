use std::fs;
use std::path::{Path, PathBuf};

use serde_json;

use super::error::ResourceError;
use super::index::{LanguageIndex, LexiconLookup, LexiconRecord, SymbolIndex};
use super::pack::{AbbreviationPack, LanguagePack, ResourcePackInfo, SymbolPack};
use crate::core::types::SymbolReading;

mod validation;

use validation::{
    abbreviation_language_allowed, canonical_language, json_files, validate_abbreviation_pack,
    validate_declared_hash, validate_file_size, validate_language_pack, validate_symbol_pack,
};
include!(concat!(env!("OUT_DIR"), "/embedded_resources.rs"));

pub const DEFAULT_MAX_RESOURCE_BYTES: usize = 64 * 1024 * 1024;
pub const DEFAULT_MAX_PACK_ENTRIES: usize = 1_000_000;
pub const DEFAULT_MAX_SYMBOLS: usize = 100_000;
pub const DEFAULT_MAX_READINGS_PER_SYMBOL: usize = 256;

/// Bounds applied before a pack is parsed or indexed. They are intentionally
/// explicit so applications can tighten them for untrusted downloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceLimits {
    pub max_resource_bytes: usize,
    pub max_pack_entries: usize,
    pub max_symbols: usize,
    pub max_readings_per_symbol: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_resource_bytes: DEFAULT_MAX_RESOURCE_BYTES,
            max_pack_entries: DEFAULT_MAX_PACK_ENTRIES,
            max_symbols: DEFAULT_MAX_SYMBOLS,
            max_readings_per_symbol: DEFAULT_MAX_READINGS_PER_SYMBOL,
        }
    }
}

/// Data-driven loader and index for all language and symbol resources.
#[derive(Debug, Clone, Default)]
pub struct ResourceLoader {
    pub(crate) language_index: LanguageIndex,
    pub(crate) symbol_index: SymbolIndex,
    pub(crate) abbreviation_index: std::collections::BTreeMap<String, Vec<SymbolReading>>,
    pub(crate) manifest: Vec<ResourcePackInfo>,
    pub(crate) limits: ResourceLimits,
}

impl ResourceLoader {
    /// Load every embedded language pack and embedded symbol pack.
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
        for (name, source) in ABBREVIATION_PACKS {
            loader.load_abbreviation_json(source, PathBuf::from(format!("<embedded:{name}>")))?;
        }
        Ok(loader)
    }

    /// Backward-compatible alias for the embedded seed set.
    pub fn common() -> Result<Self, ResourceError> {
        Self::embedded()
    }

    pub fn with_limits(limits: ResourceLimits) -> Self {
        Self {
            limits,
            ..Self::default()
        }
    }

    pub fn limits(&self) -> &ResourceLimits {
        &self.limits
    }

    /// Load all JSON language packs below `path` recursively.
    pub fn from_directory(path: impl AsRef<Path>) -> Result<Self, ResourceError> {
        let mut loader = Self::default();
        loader.load_language_directory(path)?;
        Ok(loader)
    }

    /// Load `languages/` and, when present, `symbols/` and `abbreviations/`
    /// below a resource root.
    pub fn from_resource_root(path: impl AsRef<Path>) -> Result<Self, ResourceError> {
        let root = path.as_ref();
        let language_dir = root.join("languages");
        let symbol_dir = root.join("symbols");
        let abbreviation_dir = root.join("abbreviations");
        let mut loader = Self::default();
        if language_dir.is_dir() {
            loader.load_language_directory(&language_dir)?;
        } else {
            loader.load_language_directory(root)?;
        }
        if symbol_dir.is_dir() {
            loader.load_symbol_directory(symbol_dir)?;
        }
        if abbreviation_dir.is_dir() {
            loader.load_abbreviation_directory(abbreviation_dir)?;
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
        validate_file_size(&path, self.limits.max_resource_bytes)?;
        let source = fs::read_to_string(&path).map_err(|error| ResourceError::Io {
            path: path.clone(),
            message: error.to_string(),
        })?;
        self.load_language_json(&source, path)
    }

    pub fn load_symbol_file(&mut self, path: impl AsRef<Path>) -> Result<(), ResourceError> {
        let path = path.as_ref().to_path_buf();
        validate_file_size(&path, self.limits.max_resource_bytes)?;
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
        let entry_count =
            pack.entries.len() + pack.words.len() + pack.stop_words.len() + pack.examples.len();
        if entry_count > self.limits.max_pack_entries {
            return Err(ResourceError::Validation {
                path: source_path.as_ref().to_path_buf(),
                message: format!(
                    "language pack contains {entry_count} entries; maximum is {}",
                    self.limits.max_pack_entries
                ),
            });
        }
        self.manifest.push(ResourcePackInfo {
            kind: "language".to_string(),
            language: Some(canonical_language(&pack.language)),
            name: pack.name.clone(),
            source: pack.source.clone(),
            license: pack.license.clone(),
            revision: pack.revision.clone(),
            sha256: pack.sha256.clone(),
            origin: source_path.as_ref().display().to_string(),
        });
        self.language_index.add_pack(pack, source_path.as_ref());
        Ok(())
    }

    pub fn add_symbol_pack(
        &mut self,
        mut pack: SymbolPack,
        source_path: impl AsRef<Path>,
    ) -> Result<(), ResourceError> {
        if let Some(language) = &mut pack.language {
            *language = canonical_language(language);
            if language.is_empty() {
                return Err(ResourceError::Validation {
                    path: source_path.as_ref().to_path_buf(),
                    message: "symbol pack language cannot be empty".to_string(),
                });
            }
        }
        let default_language = pack.language.clone();
        // Provenance stamped onto readings/concepts that do not declare
        // their own: pack name when set, otherwise the load origin.
        let provenance = if pack.name.trim().is_empty() {
            source_path.as_ref().display().to_string()
        } else {
            pack.name.clone()
        };
        for symbol in &mut pack.symbols {
            for concept in &mut symbol.concepts {
                concept.id = super::pack::canonical_concept_id(&concept.id);
                if concept.source.is_none() {
                    concept.source = Some(provenance.clone());
                }
            }
            for reading in &mut symbol.readings {
                if reading.source.is_none() {
                    reading.source = Some(provenance.clone());
                }
                if let Some(language) = &mut reading.language {
                    *language = canonical_language(language);
                    if language.is_empty() {
                        return Err(ResourceError::Validation {
                            path: source_path.as_ref().to_path_buf(),
                            message: format!(
                                "reading language cannot be empty for symbol {:?}",
                                symbol.token
                            ),
                        });
                    }
                    if let Some(default) = default_language.as_deref()
                        && language != default
                    {
                        return Err(ResourceError::Validation {
                            path: source_path.as_ref().to_path_buf(),
                            message: format!(
                                "reading language {language:?} conflicts with symbol pack language {default:?}"
                            ),
                        });
                    }
                } else {
                    reading.language = default_language.clone();
                }
            }
        }
        validate_symbol_pack(&pack, source_path.as_ref())?;
        if pack.symbols.len() > self.limits.max_symbols {
            return Err(ResourceError::Validation {
                path: source_path.as_ref().to_path_buf(),
                message: format!(
                    "symbol pack contains {}; maximum is {}",
                    pack.symbols.len(),
                    self.limits.max_symbols
                ),
            });
        }
        if pack
            .symbols
            .iter()
            .any(|symbol| symbol.readings.len() > self.limits.max_readings_per_symbol)
        {
            return Err(ResourceError::Validation {
                path: source_path.as_ref().to_path_buf(),
                message: format!(
                    "symbol readings exceed maximum {}",
                    self.limits.max_readings_per_symbol
                ),
            });
        }
        self.manifest.push(ResourcePackInfo {
            kind: "symbol".to_string(),
            language: pack.language.clone(),
            name: pack.name.clone(),
            source: pack.source.clone(),
            license: pack.license.clone(),
            revision: pack.revision.clone(),
            sha256: pack.sha256.clone(),
            origin: source_path.as_ref().display().to_string(),
        });
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
        if source.len() > self.limits.max_resource_bytes {
            return Err(ResourceError::Validation {
                path,
                message: format!(
                    "resource is {} bytes; maximum is {}",
                    source.len(),
                    self.limits.max_resource_bytes
                ),
            });
        }
        let parse_path = path.clone();
        let pack: LanguagePack =
            serde_json::from_str(source).map_err(|error| ResourceError::Parse {
                path: parse_path,
                message: error.to_string(),
            })?;
        validate_declared_hash(pack.sha256.as_deref(), source, &path)?;
        self.add_language_pack(pack, path)
    }

    pub fn load_symbol_json(
        &mut self,
        source: &str,
        path: impl Into<PathBuf>,
    ) -> Result<(), ResourceError> {
        let path = path.into();
        if source.len() > self.limits.max_resource_bytes {
            return Err(ResourceError::Validation {
                path,
                message: format!(
                    "resource is {} bytes; maximum is {}",
                    source.len(),
                    self.limits.max_resource_bytes
                ),
            });
        }
        let parse_path = path.clone();
        let pack: SymbolPack =
            serde_json::from_str(source).map_err(|error| ResourceError::Parse {
                path: parse_path,
                message: error.to_string(),
            })?;
        validate_declared_hash(pack.sha256.as_deref(), source, &path)?;
        self.add_symbol_pack(pack, path)
    }

    pub fn load_abbreviation_directory(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<(), ResourceError> {
        let path = path.as_ref();
        let files = json_files(path)?;
        if files.is_empty() {
            return Err(ResourceError::Validation {
                path: path.to_path_buf(),
                message: "no JSON abbreviation packs found".to_string(),
            });
        }
        for file in files {
            self.load_abbreviation_file(file)?;
        }
        Ok(())
    }

    pub fn load_abbreviation_file(&mut self, path: impl AsRef<Path>) -> Result<(), ResourceError> {
        let path = path.as_ref().to_path_buf();
        validate_file_size(&path, self.limits.max_resource_bytes)?;
        let source = fs::read_to_string(&path).map_err(|error| ResourceError::Io {
            path: path.clone(),
            message: error.to_string(),
        })?;
        self.load_abbreviation_json(&source, path)
    }

    pub fn load_abbreviation_json(
        &mut self,
        source: &str,
        path: impl Into<PathBuf>,
    ) -> Result<(), ResourceError> {
        let path = path.into();
        if source.len() > self.limits.max_resource_bytes {
            return Err(ResourceError::Validation {
                path,
                message: format!(
                    "resource is {} bytes; maximum is {}",
                    source.len(),
                    self.limits.max_resource_bytes
                ),
            });
        }
        let parse_path = path.clone();
        let pack: AbbreviationPack =
            serde_json::from_str(source).map_err(|error| ResourceError::Parse {
                path: parse_path,
                message: error.to_string(),
            })?;
        validate_declared_hash(pack.sha256.as_deref(), source, &path)?;
        self.add_abbreviation_pack(pack, path)
    }

    pub fn add_abbreviation_pack(
        &mut self,
        pack: AbbreviationPack,
        source_path: impl AsRef<Path>,
    ) -> Result<(), ResourceError> {
        validate_abbreviation_pack(&pack, source_path.as_ref())?;
        if pack.entries.len() > self.limits.max_pack_entries {
            return Err(ResourceError::Validation {
                path: source_path.as_ref().to_path_buf(),
                message: format!(
                    "abbreviation pack contains {} entries; maximum is {}",
                    pack.entries.len(),
                    self.limits.max_pack_entries
                ),
            });
        }
        let language = canonical_language(&pack.language);
        self.manifest.push(ResourcePackInfo {
            kind: "abbreviation".to_string(),
            language: Some(language.clone()),
            name: pack.name.clone(),
            source: pack.source.clone(),
            license: pack.license.clone(),
            revision: pack.revision.clone(),
            sha256: pack.sha256.clone(),
            origin: source_path.as_ref().display().to_string(),
        });
        let provenance = if pack.name.trim().is_empty() {
            format!("abbreviations:{language}")
        } else {
            pack.name.clone()
        };
        for entry in pack.entries {
            let key = entry.token.to_ascii_lowercase();
            let slot = self.abbreviation_index.entry(key).or_default();
            for reading in entry.readings {
                slot.push(
                    SymbolReading::new(
                        reading.text,
                        Some(language.clone()),
                        reading.probability,
                        reading.kind,
                    )
                    .with_source(provenance.clone()),
                );
            }
        }
        Ok(())
    }

    /// Chat-abbreviation expansions for `token` (case-insensitive), sorted by
    /// descending probability and filtered by `languages` when non-empty.
    pub fn abbreviation_readings(
        &self,
        token: &str,
        languages: Option<&[String]>,
        max_readings: usize,
    ) -> Vec<SymbolReading> {
        let mut readings = self
            .abbreviation_index
            .get(&token.to_ascii_lowercase())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|reading| abbreviation_language_allowed(reading.language.as_deref(), languages))
            .collect::<Vec<_>>();
        readings.sort_by(|left, right| {
            right
                .probability
                .total_cmp(&left.probability)
                .then_with(|| left.text.cmp(&right.text))
        });
        readings.truncate(max_readings.max(1));
        readings
    }

    /// Provenance for every loaded pack: source, license, revision, and hash
    /// when the pack declares them.
    pub fn manifest(&self) -> &[ResourcePackInfo] {
        &self.manifest
    }

    /// Languages with at least one symbol pack (neutral concept packs excluded).
    pub fn symbol_pack_languages(&self) -> Vec<String> {
        self.manifest
            .iter()
            .filter(|info| info.kind == "symbol")
            .filter_map(|info| info.language.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn abbreviation_languages(&self) -> Vec<String> {
        let languages = self
            .abbreviation_index
            .values()
            .flat_map(|readings| readings.iter())
            .filter_map(|reading| reading.language.clone())
            .collect::<std::collections::BTreeSet<_>>();
        languages.into_iter().collect()
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

    pub fn symbol_tokens(&self) -> Vec<String> {
        self.symbol_index.tokens()
    }

    pub fn symbol_languages(&self, token: &str) -> Vec<String> {
        let languages = self
            .symbol_index
            .get(token)
            .into_iter()
            .flat_map(|symbol| symbol.readings.iter())
            .filter_map(|reading| reading.language.clone())
            .collect::<std::collections::BTreeSet<_>>();
        languages.into_iter().collect()
    }

    pub fn language_pack(&self, language: &str) -> Option<&LanguagePack> {
        self.language_index.language_pack(language)
    }

    pub fn language_packs(&self, language: &str) -> Vec<&LanguagePack> {
        self.language_index.language_packs(language)
    }

    pub fn profile_texts(&self) -> std::collections::BTreeMap<String, Vec<String>> {
        self.language_index.profile_texts()
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

    pub fn detect_languages_with_limit(
        &self,
        text: &str,
        max_candidates: usize,
    ) -> Vec<crate::core::types::LanguageCandidate> {
        self.language_index
            .detect_languages_with_limit(text, max_candidates)
    }
}
