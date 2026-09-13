//! Versioned, data-driven language and symbol resources.
//!
//! The loader indexes every JSON language pack found in a directory. The
//! repository ships a deliberately small set of common-language seed packs;
//! applications can add larger licensed dictionaries without changing the
//! analysis code.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::core::error::ProviderError;
use crate::core::providers::{LanguageDetectionProvider, LexiconProvider, SymbolKnowledgeProvider};
use crate::core::types::{LanguageCandidate, SymbolConcept, SymbolReading};
use crate::normalization::unicode::casefold_text;

pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;

const EMBEDDED_LANGUAGE_PACKS: &[(&str, &str)] = &[
    ("en.json", include_str!("../resources/languages/en.json")),
    ("es.json", include_str!("../resources/languages/es.json")),
    ("pt.json", include_str!("../resources/languages/pt.json")),
    ("fr.json", include_str!("../resources/languages/fr.json")),
    ("de.json", include_str!("../resources/languages/de.json")),
    ("it.json", include_str!("../resources/languages/it.json")),
];

const EMBEDDED_SYMBOL_PACK: (&str, &str) = (
    "common.json",
    include_str!("../resources/symbols/common.json"),
);

#[derive(Debug)]
pub enum ResourceError {
    Io { path: PathBuf, message: String },
    Parse { path: PathBuf, message: String },
    Validation { path: PathBuf, message: String },
}

impl Display for ResourceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, message } => {
                write!(f, "resource I/O error at {}: {message}", path.display())
            }
            Self::Parse { path, message } => {
                write!(f, "resource parse error at {}: {message}", path.display())
            }
            Self::Validation { path, message } => {
                write!(f, "invalid resource at {}: {message}", path.display())
            }
        }
    }
}

impl std::error::Error for ResourceError {}

fn default_schema_version() -> u32 {
    SUPPORTED_SCHEMA_VERSION
}

fn default_weight() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LexiconEntry {
    pub word: String,
    #[serde(default)]
    pub lemma: Option<String>,
    #[serde(default)]
    pub stop_word: bool,
    #[serde(default = "default_weight")]
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LanguagePack {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub language: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub entries: Vec<LexiconEntry>,
    /// Optional compact form for packs that only have word strings.
    #[serde(default)]
    pub words: Vec<String>,
    /// Optional compact stop-word form.
    #[serde(default)]
    pub stop_words: Vec<String>,
    /// Examples are indexed as words as well as retained for provenance and
    /// future phrase-level scoring.
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SymbolResource {
    pub token: String,
    #[serde(default)]
    pub unicode_name: Option<String>,
    #[serde(default)]
    pub concepts: Vec<SymbolConcept>,
    #[serde(default)]
    pub readings: Vec<SymbolReading>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SymbolPack {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub symbols: Vec<SymbolResource>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
}

#[derive(Debug, Clone)]
struct IndexedWord {
    language: String,
    lemma: String,
    stop_word: bool,
    weight: f64,
}

/// In-memory index for all loaded language and symbol packs.
#[derive(Debug, Clone, Default)]
pub struct ResourceLoader {
    language_packs: BTreeMap<String, LanguagePack>,
    word_index: BTreeMap<String, Vec<IndexedWord>>,
    symbol_index: BTreeMap<String, SymbolResource>,
}

impl ResourceLoader {
    /// Load the repository's small, common-language seed set.
    pub fn common() -> Result<Self, ResourceError> {
        let mut loader = Self::default();
        for (name, source) in EMBEDDED_LANGUAGE_PACKS {
            loader.load_language_json(source, PathBuf::from(format!("<embedded:{name}>")))?;
        }
        let (name, source) = EMBEDDED_SYMBOL_PACK;
        loader.load_symbol_json(source, PathBuf::from(format!("<embedded:{name}>")))?;
        Ok(loader)
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
        let files = json_files(path)?;
        for file in files {
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

    pub fn load_language_json(
        &mut self,
        source: &str,
        path: impl Into<PathBuf>,
    ) -> Result<(), ResourceError> {
        let path = path.into();
        let mut pack: LanguagePack =
            serde_json::from_str(source).map_err(|error| ResourceError::Parse {
                path: path.clone(),
                message: error.to_string(),
            })?;
        pack.language = canonical_language(&pack.language);
        validate_language_pack(&pack, &path)?;
        if self.language_packs.contains_key(&pack.language) {
            return Err(ResourceError::Validation {
                path,
                message: format!("duplicate language pack: {}", pack.language),
            });
        }
        self.language_packs.insert(pack.language.clone(), pack);
        self.rebuild_word_index();
        Ok(())
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
        validate_symbol_pack(&pack, &path)?;
        for symbol in pack.symbols {
            self.symbol_index
                .entry(symbol.token.clone())
                .and_modify(|existing| merge_symbol(existing, &symbol))
                .or_insert(symbol);
        }
        Ok(())
    }

    pub fn languages(&self) -> Vec<String> {
        self.language_packs.keys().cloned().collect()
    }

    pub fn language_count(&self) -> usize {
        self.language_packs.len()
    }

    pub fn word_count(&self) -> usize {
        self.word_index.len()
    }

    pub fn symbol_count(&self) -> usize {
        self.symbol_index.len()
    }

    pub fn language_pack(&self, language: &str) -> Option<&LanguagePack> {
        self.language_packs.get(&canonical_language(language))
    }

    pub fn lookup_languages(&self, word: &str) -> Vec<String> {
        let folded = fold_word(word);
        let mut languages = self
            .word_index
            .get(&folded)
            .into_iter()
            .flat_map(|entries| entries.iter().map(|entry| entry.language.clone()))
            .collect::<BTreeSet<_>>();
        languages.remove("unknown");
        languages.into_iter().collect()
    }

    pub fn contains_in_language(&self, word: &str, language: &str) -> bool {
        self.contains(word, Some(&[canonical_language(language)]))
    }

    pub fn detect_languages(&self, text: &str) -> Vec<LanguageCandidate> {
        let words = lexical_words(text);
        if words.is_empty() {
            return vec![LanguageCandidate::new("unknown", 1.0)];
        }
        let mut scores = BTreeMap::<String, f64>::new();
        for word in words {
            if let Some(entries) = self.word_index.get(&word) {
                for entry in entries {
                    *scores.entry(entry.language.clone()).or_default() += entry.weight;
                }
            }
        }
        if scores.is_empty() {
            return vec![LanguageCandidate::new("unknown", 1.0)];
        }
        let total = scores.values().sum::<f64>().max(f64::EPSILON);
        let mut candidates = scores
            .into_iter()
            .map(|(language, score)| LanguageCandidate::new(language, score / total))
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| right.probability.total_cmp(&left.probability));
        candidates
    }

    fn rebuild_word_index(&mut self) {
        self.word_index.clear();
        let packs = self.language_packs.values().cloned().collect::<Vec<_>>();
        for pack in packs {
            for entry in &pack.entries {
                self.add_indexed_word(
                    &entry.word,
                    &pack.language,
                    entry.lemma.as_deref(),
                    entry.stop_word,
                    entry.weight,
                );
            }
            for word in &pack.words {
                self.add_indexed_word(word, &pack.language, None, false, 1.0);
            }
            for word in &pack.stop_words {
                self.add_indexed_word(word, &pack.language, None, true, 1.0);
            }
            for example in &pack.examples {
                for word in lexical_words(example) {
                    self.add_indexed_word(&word, &pack.language, None, false, 1.0);
                }
            }
        }
    }

    fn add_indexed_word(
        &mut self,
        word: &str,
        language: &str,
        lemma: Option<&str>,
        stop_word: bool,
        weight: f64,
    ) {
        let folded = fold_word(word);
        if folded.is_empty() {
            return;
        }
        let entries = self.word_index.entry(folded).or_default();
        if let Some(existing) = entries.iter_mut().find(|entry| entry.language == language) {
            existing.stop_word |= stop_word;
            if let Some(lemma) = lemma {
                existing.lemma = fold_word(lemma);
            }
            existing.weight = existing.weight.max(weight);
            return;
        }
        entries.push(IndexedWord {
            language: language.to_string(),
            lemma: fold_word(lemma.unwrap_or(word)),
            stop_word,
            weight,
        });
    }
}

impl LexiconProvider for ResourceLoader {
    fn contains(&self, word: &str, languages: Option<&[String]>) -> bool {
        self.word_index
            .get(&fold_word(word))
            .is_some_and(|entries| {
                entries
                    .iter()
                    .any(|entry| language_allowed(&entry.language, languages))
            })
    }

    fn starts_with(&self, prefix: &str, languages: Option<&[String]>) -> bool {
        let prefix = fold_word(prefix);
        !prefix.is_empty()
            && self.word_index.iter().any(|(word, entries)| {
                word.starts_with(&prefix)
                    && entries
                        .iter()
                        .any(|entry| language_allowed(&entry.language, languages))
            })
    }

    fn is_stop_word(&self, word: &str, languages: Option<&[String]>) -> bool {
        self.word_index
            .get(&fold_word(word))
            .is_some_and(|entries| {
                entries
                    .iter()
                    .any(|entry| entry.stop_word && language_allowed(&entry.language, languages))
            })
    }

    fn lemma(&self, word: &str, languages: Option<&[String]>) -> Option<String> {
        let entries = self.word_index.get(&fold_word(word))?;
        if let Some(languages) = languages.filter(|values| !values.is_empty()) {
            for language in languages {
                let language = canonical_language(language);
                if matches!(language.as_str(), "unknown" | "und") {
                    continue;
                }
                if let Some(entry) = entries.iter().find(|entry| entry.language == language) {
                    return Some(entry.lemma.clone());
                }
            }
            entries.first().map(|entry| entry.lemma.clone())
        } else {
            entries.first().map(|entry| entry.lemma.clone())
        }
    }
}

impl LanguageDetectionProvider for ResourceLoader {
    fn detect(&self, text: &str) -> Result<Vec<LanguageCandidate>, ProviderError> {
        Ok(self.detect_languages(text))
    }
}

impl SymbolKnowledgeProvider for ResourceLoader {
    fn readings(&self, token: &str, max_readings: usize) -> Vec<SymbolReading> {
        self.symbol_index
            .get(token)
            .map(|symbol| symbol.readings.iter().take(max_readings).cloned().collect())
            .unwrap_or_default()
    }

    fn concepts(&self, token: &str) -> Vec<SymbolConcept> {
        self.symbol_index
            .get(token)
            .map(|symbol| symbol.concepts.clone())
            .unwrap_or_default()
    }

    fn unicode_name(&self, token: &str) -> Option<String> {
        self.symbol_index
            .get(token)
            .and_then(|symbol| symbol.unicode_name.clone())
    }
}

/// Default lexicon provider backed by the embedded common-language packs.
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultLexiconProvider;

impl LexiconProvider for DefaultLexiconProvider {
    fn contains(&self, word: &str, languages: Option<&[String]>) -> bool {
        embedded_common().contains(word, languages)
    }

    fn starts_with(&self, prefix: &str, languages: Option<&[String]>) -> bool {
        embedded_common().starts_with(prefix, languages)
    }

    fn is_stop_word(&self, word: &str, languages: Option<&[String]>) -> bool {
        embedded_common().is_stop_word(word, languages)
    }

    fn lemma(&self, word: &str, languages: Option<&[String]>) -> Option<String> {
        embedded_common().lemma(word, languages)
    }
}

pub fn embedded_common() -> &'static ResourceLoader {
    static COMMON: OnceLock<ResourceLoader> = OnceLock::new();
    COMMON.get_or_init(|| ResourceLoader::common().expect("embedded resources must be valid"))
}

fn canonical_language(language: &str) -> String {
    language.trim().to_lowercase().replace('_', "-")
}

fn fold_word(word: &str) -> String {
    casefold_text(word.trim())
}

fn lexical_words(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_alphabetic())
        .filter(|word| !word.is_empty())
        .map(fold_word)
        .filter(|word| !word.is_empty())
        .collect()
}

fn language_allowed(language: &str, languages: Option<&[String]>) -> bool {
    match languages {
        None => true,
        Some([]) => true,
        Some(values) => values
            .iter()
            .map(|value| canonical_language(value))
            .any(|value| matches!(value.as_str(), "unknown" | "und") || value == language),
    }
}

fn validate_language_pack(pack: &LanguagePack, path: &Path) -> Result<(), ResourceError> {
    if pack.schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(ResourceError::Validation {
            path: path.to_path_buf(),
            message: format!("unsupported schema_version={}", pack.schema_version),
        });
    }
    if pack.language.is_empty() {
        return Err(ResourceError::Validation {
            path: path.to_path_buf(),
            message: "language cannot be empty".to_string(),
        });
    }
    for entry in &pack.entries {
        if fold_word(&entry.word).is_empty() {
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
        if fold_word(word).is_empty() {
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
    }
    Ok(())
}

fn merge_symbol(existing: &mut SymbolResource, incoming: &SymbolResource) {
    if existing.unicode_name.is_none() {
        existing.unicode_name = incoming.unicode_name.clone();
    }
    for concept in &incoming.concepts {
        if !existing.concepts.iter().any(|value| value.id == concept.id) {
            existing.concepts.push(concept.clone());
        }
    }
    for reading in &incoming.readings {
        if !existing.readings.iter().any(|value| value == reading) {
            existing.readings.push(reading.clone());
        }
    }
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
