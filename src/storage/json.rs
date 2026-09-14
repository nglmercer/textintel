use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::capabilities::ProviderCapabilities;
use crate::core::providers::VectorStore;
use crate::core::types::{MessageFingerprint, SearchCandidateSet};

use super::MemoryStore;

const STORE_SCHEMA_VERSION: u32 = 1;
const DEFAULT_MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
struct PersistedStore {
    schema_version: u32,
    records: BTreeMap<String, MessageFingerprint>,
}

/// Raw envelope: records stay unparsed JSON so [`migrate`] can inspect each
/// fingerprint version before deserializing.
///
/// [`migrate`]: crate::storage::migrate
#[derive(Debug, Serialize, Deserialize)]
struct RawPersistedStore {
    schema_version: u32,
    #[serde(default)]
    records: BTreeMap<String, serde_json::Value>,
}

/// Local persistent store using a versioned JSON envelope and the same
/// retrieval indexes as [`MemoryStore`]. It never fetches or interprets URLs.
#[derive(Debug, Clone)]
pub struct JsonFileStore {
    path: PathBuf,
    max_file_bytes: u64,
    inner: MemoryStore,
}

impl JsonFileStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        Self::open_with_limit(path, DEFAULT_MAX_FILE_BYTES)
    }

    pub fn open_with_limit(path: impl AsRef<Path>, max_file_bytes: u64) -> Result<Self, String> {
        let path = path.as_ref().to_path_buf();
        let mut store = Self {
            path,
            max_file_bytes,
            inner: MemoryStore::default(),
        };
        if store.path.exists() {
            let metadata = fs::metadata(&store.path).map_err(|error| error.to_string())?;
            if metadata.len() > max_file_bytes {
                return Err(format!(
                    "persistent store exceeds max_file_bytes={max_file_bytes}"
                ));
            }
            let source = fs::read_to_string(&store.path).map_err(|error| error.to_string())?;
            let raw: RawPersistedStore = serde_json::from_str(&source).map_err(|error| {
                format!("invalid persistent store {}: {error}", store.path.display())
            })?;
            if raw.schema_version != STORE_SCHEMA_VERSION {
                return Err(format!(
                    "unsupported persistent store schema_version={}",
                    raw.schema_version
                ));
            }
            for (id, record) in raw.records {
                let bytes = serde_json::to_vec(&record)
                    .map_err(|error| format!("record {id:?} is not JSON: {error}"))?;
                let migrated = crate::storage::migrate::migrate_fingerprint_bytes(&bytes)
                    .map_err(|error| format!("record {id:?}: {error}"))?;
                store.inner.upsert(id, migrated.fingerprint)?;
            }
        }
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn flush(&self) -> Result<(), String> {
        let persisted = PersistedStore {
            schema_version: STORE_SCHEMA_VERSION,
            records: self.inner.records().into_iter().collect::<BTreeMap<_, _>>(),
        };
        let bytes = serde_json::to_vec_pretty(&persisted).map_err(|error| error.to_string())?;
        if bytes.len() as u64 > self.max_file_bytes {
            return Err(format!(
                "serialized store exceeds max_file_bytes={}",
                self.max_file_bytes
            ));
        }
        if let Some(parent) = self
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, &bytes).map_err(|error| error.to_string())?;
        if let Err(error) = fs::rename(&temporary, &self.path) {
            let _ = fs::remove_file(&self.path);
            fs::rename(&temporary, &self.path).map_err(|_| error.to_string())?;
        }
        Ok(())
    }
}

impl VectorStore for JsonFileStore {
    fn upsert(&mut self, id: String, fingerprint: MessageFingerprint) -> Result<(), String> {
        let previous = self.inner.get(&id).cloned();
        self.inner.upsert(id.clone(), fingerprint)?;
        if let Err(error) = self.flush() {
            if let Some(previous) = previous {
                self.inner.upsert(id, previous)?;
            } else {
                self.inner.remove(&id)?;
            }
            return Err(error);
        }
        Ok(())
    }

    fn remove(&mut self, id: &str) -> Result<bool, String> {
        let previous = self.inner.get(id).cloned();
        let removed = self.inner.remove(id)?;
        if removed {
            if let Err(error) = self.flush() {
                if let Some(previous) = previous {
                    self.inner.upsert(id.to_string(), previous)?;
                }
                return Err(error);
            }
        }
        Ok(removed)
    }

    fn len(&self) -> usize {
        self.inner.len()
    }

    fn records(&self) -> Vec<(String, MessageFingerprint)> {
        self.inner.records()
    }

    fn search_candidates(
        &self,
        query: &MessageFingerprint,
        limit: usize,
    ) -> Result<Vec<(String, MessageFingerprint)>, String> {
        Ok(self.inner.search_candidates(query, limit))
    }

    fn search_candidates_with_metadata(
        &self,
        query: &MessageFingerprint,
        limit: usize,
    ) -> Result<SearchCandidateSet, String> {
        Ok(self.inner.search_candidates_with_metadata(query, limit))
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("json_file_store")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EngineConfig, TextIntelligence};

    fn test_path(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "textintel-json-migrate-{name}-{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn write_envelope(path: &Path, record: serde_json::Value) {
        let envelope = serde_json::json!({
            "schema_version": STORE_SCHEMA_VERSION,
            "records": {"doc": record},
        });
        std::fs::write(path, serde_json::to_string_pretty(&envelope).unwrap()).unwrap();
    }

    #[test]
    fn versionless_records_migrate_on_load() {
        let engine = TextIntelligence::new(EngineConfig::default());
        let fingerprint = engine.analyze("compra ahora").unwrap();
        let mut record = serde_json::to_value(&fingerprint).unwrap();
        record.as_object_mut().unwrap().remove("schema_version");
        let path = test_path("v1");
        write_envelope(&path, record);
        let store = JsonFileStore::open(&path).unwrap();
        let records = store.records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, "doc");
        assert_eq!(
            records[0].1.schema_version,
            crate::core::types::FINGERPRINT_SCHEMA_VERSION
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn newer_records_are_rejected_not_guessed() {
        let engine = TextIntelligence::new(EngineConfig::default());
        let fingerprint = engine.analyze("compra ahora").unwrap();
        let mut record = serde_json::to_value(&fingerprint).unwrap();
        record["schema_version"] =
            serde_json::json!(crate::core::types::FINGERPRINT_SCHEMA_VERSION + 1);
        let path = test_path("newer");
        write_envelope(&path, record);
        assert!(JsonFileStore::open(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }
}
