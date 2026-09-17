//! Versioned, data-driven language and symbol resources.
//!
//! The loader indexes every JSON language pack found in a directory. The
//! repository contains a small seed set, while applications can add complete
//! licensed dictionaries without changing the analysis code.

mod error;
mod index;
mod loader;
mod order;
mod pack;
mod providers;

pub use error::ResourceError;
pub use index::{LanguageIndex, LexiconLookup, LexiconRecord, LookupStatus};
pub use loader::{ResourceLimits, ResourceLoader};
pub use order::{IndexKey, normalize_key};
pub use pack::{
    AbbreviationEntry, AbbreviationPack, AbbreviationReading, LanguagePack, LexiconEntry,
    ResourcePackInfo, SUPPORTED_SCHEMA_VERSION, SymbolPack, SymbolResource, canonical_concept_id,
};
pub use providers::{
    DefaultLexiconProvider, embedded as embedded_common, embedded as embedded_resources,
};
