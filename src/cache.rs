//! Safe revision-aware caching for expensive provider calls.
//!
//! Every cache in this module is bounded (`max_entries`, oldest-first
//! eviction by key order) and revision-aware: entries are namespaced by the
//! wrapped provider's identity plus its model/data revision, and an observed
//! revision change invalidates the cache instead of serving stale vectors.
//! Caches are deterministic (no TTLs, no wall-clock expiry) and never shared
//! across providers.

mod core;
mod g2p;
mod language;
mod rebus;

pub use core::{resource_revision, CacheDiagnostics, RevisionCache};
pub use g2p::CachedG2PProvider;
pub use language::CachedLanguageDetectionProvider;
pub use rebus::{rebus_cache_key, CachedRebusDecoder};
