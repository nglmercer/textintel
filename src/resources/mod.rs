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
pub use order::{normalize_key, IndexKey};
pub use pack::{
    canonical_concept_id, AbbreviationEntry, AbbreviationPack, AbbreviationReading, LanguagePack,
    LexiconEntry, ResourcePackInfo, SymbolPack, SymbolResource, SUPPORTED_SCHEMA_VERSION,
};
pub use providers::{
    embedded as embedded_common, embedded as embedded_resources, DefaultLexiconProvider,
};
