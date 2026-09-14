use serde::{Deserialize, Serialize};

use crate::core::types::{SymbolConcept, SymbolReading};

pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;

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
    #[serde(default)]
    pub source: Option<String>,
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
    /// Compact form for packs that only have word strings.
    #[serde(default)]
    pub words: Vec<String>,
    /// Compact stop-word form.
    #[serde(default)]
    pub stop_words: Vec<String>,
    /// Examples are indexed as words and retained for phrase-level features.
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
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

fn default_reading_type() -> String {
    "chat_abbreviation".to_string()
}

/// One spoken expansion of a chat abbreviation token.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AbbreviationReading {
    pub text: String,
    #[serde(default = "default_weight")]
    pub probability: f64,
    #[serde(default = "default_reading_type", rename = "type")]
    pub kind: String,
}

/// Versioned chat/slang abbreviation entries for one language. Language
/// specific mappings live here, never in generic core logic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AbbreviationEntry {
    pub token: String,
    #[serde(default)]
    pub readings: Vec<AbbreviationReading>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AbbreviationPack {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub language: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub entries: Vec<AbbreviationEntry>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
}

/// Provenance record for one loaded resource pack, surfaced through
/// `ResourceLoader::manifest` and `TextIntelligence::resource_manifest` so
/// deployments can report exactly which linguistic data backs an analysis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourcePackInfo {
    /// Pack family: `"language"`, `"symbol"`, or `"abbreviation"`.
    pub kind: String,
    pub language: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    /// Where the pack was loaded from (file path or `<embedded:...>` tag).
    pub origin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SymbolPack {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// Optional default language for readings in this pack. A neutral pack
    /// omits it and may contain only concepts/Unicode metadata.
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub symbols: Vec<SymbolResource>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
}
