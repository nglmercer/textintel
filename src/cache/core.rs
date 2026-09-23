//! Cache core: diagnostics, revision strings, and the bounded
//! revision-namespaced map every provider cache builds on.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::core::error::ProviderError;

/// Observable cache state. Carries counts only — never keys, values, or user
/// text — so it is safe to log and export by default.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CacheDiagnostics {
    pub enabled: bool,
    pub entries: usize,
    pub capacity: usize,
    pub revision: String,
    pub hits: u64,
    pub misses: u64,
    pub invalidations: u64,
}

impl CacheDiagnostics {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Self::default()
        }
    }
}

/// Stable cache revision for a resource set: every pack's kind, language,
/// name, declared revision, and content hash, sorted. Any pack swap changes
/// the string, so revision-aware caches invalidate instead of serving decodes
/// from stale resources. Pack metadata only — never user text.
pub fn resource_revision(manifest: &[crate::resources::ResourcePackInfo]) -> String {
    let mut parts: Vec<String> = manifest
        .iter()
        .map(|pack| {
            format!(
                "{}:{}:{}@{}#{}",
                pack.kind,
                pack.language.as_deref().unwrap_or("-"),
                pack.name,
                pack.revision.as_deref().unwrap_or("-"),
                pack.sha256.as_deref().unwrap_or("-")
            )
        })
        .collect();
    parts.sort();
    format!("resources:v1:[{}]", parts.join(","))
}

/// Bounded deterministic cache namespaced by a revision string. A revision
/// change (new model weights, new resource packs) clears all entries: stale
/// hits across revisions are a correctness bug, not a performance tradeoff.
#[derive(Debug)]
pub struct RevisionCache<Key, Value> {
    revision: String,
    max_entries: usize,
    entries: BTreeMap<Key, Value>,
    hits: u64,
    misses: u64,
    invalidations: u64,
}

impl<Key, Value> RevisionCache<Key, Value>
where
    Key: Ord + Clone,
    Value: Clone,
{
    pub fn new(revision: impl Into<String>, max_entries: usize) -> Self {
        Self {
            revision: revision.into(),
            max_entries: max_entries.max(1),
            entries: BTreeMap::new(),
            hits: 0,
            misses: 0,
            invalidations: 0,
        }
    }

    pub fn revision(&self) -> &str {
        &self.revision
    }

    pub fn capacity(&self) -> usize {
        self.max_entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Observable state: counts only, never keys or values.
    pub fn diagnostics(&self) -> CacheDiagnostics {
        CacheDiagnostics {
            enabled: true,
            entries: self.entries.len(),
            capacity: self.max_entries,
            revision: self.revision.clone(),
            hits: self.hits,
            misses: self.misses,
            invalidations: self.invalidations,
        }
    }

    pub fn get<Q>(&mut self, key: &Q) -> Option<Value>
    where
        Key: std::borrow::Borrow<Q>,
        Q: Ord + ?Sized,
    {
        let hit = self.entries.get(key).cloned();
        if hit.is_some() {
            self.hits += 1;
        } else {
            self.misses += 1;
        }
        hit
    }

    pub fn put(&mut self, key: Key, value: Value) {
        self.entries.insert(key, value);
        while self.entries.len() > self.max_entries {
            // BTreeMap has no insertion order; evict the first key. Order is
            // deterministic, which is what matters for reproducibility.
            if let Some(first) = self.entries.keys().next().cloned() {
                self.entries.remove(&first);
            } else {
                break;
            }
        }
    }

    pub fn invalidate(&mut self) {
        self.entries.clear();
        self.invalidations += 1;
    }

    /// Adopt `revision`, clearing entries when it differs from the current
    /// one. Returns true when an invalidation happened.
    pub fn set_revision(&mut self, revision: &str) -> bool {
        if self.revision != revision {
            self.revision = revision.to_string();
            self.entries.clear();
            self.invalidations += 1;
            true
        } else {
            false
        }
    }
}

pub(crate) fn lock_error(provider: &str) -> ProviderError {
    ProviderError::new(provider, "cache lock poisoned")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_change_clears_entries() {
        let mut cache = RevisionCache::new("r1", 8);
        cache.put("a".to_string(), 1);
        assert!(!cache.set_revision("r1"));
        assert_eq!(cache.get(&"a".to_string()), Some(1));
        assert!(cache.set_revision("r2"));
        assert!(cache.is_empty());
    }

    #[test]
    fn cache_is_bounded_and_deterministic() {
        let mut cache = RevisionCache::new("r1", 2);
        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);
        cache.put("c".to_string(), 3);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn diagnostics_count_hits_misses_and_invalidations() {
        let mut cache = RevisionCache::new("r1", 8);
        cache.put("a".to_string(), 1);
        assert_eq!(cache.get(&"a".to_string()), Some(1));
        assert_eq!(cache.get(&"missing".to_string()), None);
        assert!(cache.set_revision("r2"));
        cache.invalidate();
        let diagnostics = cache.diagnostics();
        assert!(diagnostics.enabled);
        assert_eq!(diagnostics.entries, 0);
        assert_eq!(diagnostics.capacity, 8);
        assert_eq!(diagnostics.revision, "r2");
        assert_eq!(diagnostics.hits, 1);
        assert_eq!(diagnostics.misses, 1);
        assert_eq!(diagnostics.invalidations, 2);
        // Diagnostics serialize without keys or values.
        let json = serde_json::to_string(&diagnostics).unwrap();
        assert!(!json.contains("\"a\""));
    }
}
