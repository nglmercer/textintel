//! Production local store backed by redb.
//!
//! [`RedbStore`] mirrors [`crate::storage::JsonFileStore`] semantics (versioned
//! envelope, in-memory indexes rebuilt on open, rollback on failed writes)
//! with transactional persistence: every mutation commits atomically.
//!
//! Layout: a `records` table maps document id to the JSON-serialized
//! fingerprint, and a `meta` table carries `schema_version`,
//! `fingerprint_version`, and free-form metadata. JSON payloads keep stored
//! records inspectable with standard tooling.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};

use crate::core::capabilities::{CapabilityLevel, ProviderCapabilities};
use crate::core::providers::VectorStore;
use crate::core::types::{MessageFingerprint, SearchCandidateSet, FINGERPRINT_SCHEMA_VERSION};

use super::MemoryStore;

const PROVIDER: &str = "redb_store";
const STORE_SCHEMA_VERSION: u32 = 1;
const RECORDS: TableDefinition<&str, &[u8]> = TableDefinition::new("records");
const META: TableDefinition<&str, &str> = TableDefinition::new("meta");
const META_SCHEMA_VERSION: &str = "schema_version";
const META_FINGERPRINT_VERSION: &str = "fingerprint_version";

/// Transactional local store. Write-through: every `upsert`/`remove` commits
/// before returning, so no `flush` step exists.
#[derive(Debug)]
pub struct RedbStore {
    path: PathBuf,
    database: Database,
    inner: MemoryStore,
}

impl RedbStore {
    /// Open (creating) the database at `path`, validating the stored schema
    /// and every fingerprint version. Unknown schema versions are rejected,
    /// never guessed.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let database = Database::create(&path).map_err(|error| format!("{PROVIDER}: {error}"))?;
        let stored_schema = {
            let transaction = database
                .begin_read()
                .map_err(|error| format!("{PROVIDER}: {error}"))?;
            match transaction.open_table(META) {
                Ok(table) => table
                    .get(META_SCHEMA_VERSION)
                    .map_err(|error| format!("{PROVIDER}: {error}"))?
                    .map(|guard| guard.value().to_string()),
                Err(_) => None,
            }
        };
        match stored_schema {
            Some(version) => {
                let parsed: u32 = version.parse().map_err(|_| {
                    format!("{PROVIDER}: stored schema_version {version:?} is not a number")
                })?;
                if parsed != STORE_SCHEMA_VERSION {
                    return Err(format!(
                        "{PROVIDER}: unsupported store schema_version={parsed} (build supports {STORE_SCHEMA_VERSION})"
                    ));
                }
            }
            None => {
                let transaction = database
                    .begin_write()
                    .map_err(|error| format!("{PROVIDER}: {error}"))?;
                {
                    let mut meta = transaction
                        .open_table(META)
                        .map_err(|error| format!("{PROVIDER}: {error}"))?;
                    meta.insert(
                        META_SCHEMA_VERSION,
                        STORE_SCHEMA_VERSION.to_string().as_str(),
                    )
                    .map_err(|error| format!("{PROVIDER}: {error}"))?;
                    meta.insert(
                        META_FINGERPRINT_VERSION,
                        FINGERPRINT_SCHEMA_VERSION.to_string().as_str(),
                    )
                    .map_err(|error| format!("{PROVIDER}: {error}"))?;
                }
                transaction
                    .commit()
                    .map_err(|error| format!("{PROVIDER}: {error}"))?;
            }
        }
        let mut store = Self {
            path,
            database,
            inner: MemoryStore::default(),
        };
        store.load_records()?;
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Stored metadata plus the live record count.
    pub fn metadata(&self) -> Result<BTreeMap<String, String>, String> {
        let transaction = self
            .database
            .begin_read()
            .map_err(|error| format!("{PROVIDER}: {error}"))?;
        let table = transaction
            .open_table(META)
            .map_err(|error| format!("{PROVIDER}: {error}"))?;
        let mut metadata = BTreeMap::new();
        for entry in table
            .iter()
            .map_err(|error| format!("{PROVIDER}: {error}"))?
        {
            let (key, value) = entry.map_err(|error| format!("{PROVIDER}: {error}"))?;
            metadata.insert(key.value().to_string(), value.value().to_string());
        }
        metadata.insert("records".to_string(), self.inner.len().to_string());
        Ok(metadata)
    }

    fn load_records(&mut self) -> Result<(), String> {
        let transaction = self
            .database
            .begin_read()
            .map_err(|error| format!("{PROVIDER}: {error}"))?;
        let records = match transaction.open_table(RECORDS) {
            Ok(table) => table
                .iter()
                .map_err(|error| format!("{PROVIDER}: {error}"))?
                .map(|entry| {
                    entry
                        .map(|(key, value)| (key.value().to_string(), value.value().to_vec()))
                        .map_err(|error| format!("{PROVIDER}: {error}"))
                })
                .collect::<Result<Vec<_>, String>>()?,
            Err(_) => Vec::new(),
        };
        drop(transaction);
        for (id, bytes) in records {
            let fingerprint = load_fingerprint(&id, &bytes)?;
            self.inner.upsert(id, fingerprint)?;
        }
        Ok(())
    }

    fn write_record(&self, id: &str, fingerprint: &MessageFingerprint) -> Result<(), String> {
        let bytes =
            serde_json::to_vec(fingerprint).map_err(|error| format!("{PROVIDER}: {error}"))?;
        let transaction = self
            .database
            .begin_write()
            .map_err(|error| format!("{PROVIDER}: {error}"))?;
        {
            let mut records = transaction
                .open_table(RECORDS)
                .map_err(|error| format!("{PROVIDER}: {error}"))?;
            records
                .insert(id, bytes.as_slice())
                .map_err(|error| format!("{PROVIDER}: {error}"))?;
        }
        transaction
            .commit()
            .map_err(|error| format!("{PROVIDER}: {error}"))?;
        Ok(())
    }

    fn delete_record(&self, id: &str) -> Result<(), String> {
        let transaction = self
            .database
            .begin_write()
            .map_err(|error| format!("{PROVIDER}: {error}"))?;
        {
            let mut records = transaction
                .open_table(RECORDS)
                .map_err(|error| format!("{PROVIDER}: {error}"))?;
            records
                .remove(id)
                .map_err(|error| format!("{PROVIDER}: {error}"))?;
        }
        transaction
            .commit()
            .map_err(|error| format!("{PROVIDER}: {error}"))?;
        Ok(())
    }
}

/// Deserialize one stored fingerprint through schema migration: current
/// payloads pass through, supported older ones upgrade explicitly, and
/// anything else is rejected with the record id attached.
fn load_fingerprint(id: &str, bytes: &[u8]) -> Result<MessageFingerprint, String> {
    crate::storage::migrate::migrate_fingerprint_bytes(bytes)
        .map(|migrated| migrated.fingerprint)
        .map_err(|error| format!("{PROVIDER}: record {id:?}: {error}"))
}

impl VectorStore for RedbStore {
    fn upsert(&mut self, id: String, fingerprint: MessageFingerprint) -> Result<(), String> {
        if fingerprint.schema_version != FINGERPRINT_SCHEMA_VERSION {
            return Err(format!(
                "{PROVIDER}: refusing fingerprint schema_version={} (build supports {FINGERPRINT_SCHEMA_VERSION})",
                fingerprint.schema_version
            ));
        }
        let previous = self.inner.get(&id).cloned();
        self.inner.upsert(id.clone(), fingerprint.clone())?;
        if let Err(error) = self.write_record(&id, &fingerprint) {
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
            if let Err(error) = self.delete_record(id) {
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
        ProviderCapabilities::new(PROVIDER).with_quality(CapabilityLevel::Production)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_path(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("textintel-redb-{name}-{}.redb", std::process::id()));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn round_trips_records_and_metadata() {
        use crate::{EngineConfig, TextIntelligence};

        let path = test_path("roundtrip");
        {
            let engine = TextIntelligence::new(EngineConfig::default())
                .with_store(RedbStore::open(&path).unwrap());
            engine.add_document("one", "compra ahora").unwrap();
            engine.add_document("two", "see you tomorrow").unwrap();
            assert_eq!(engine.document_count().unwrap(), 2);
        }
        let metadata = engine_metadata(&path);
        assert_eq!(metadata["schema_version"], STORE_SCHEMA_VERSION.to_string());
        assert_eq!(
            metadata["fingerprint_version"],
            FINGERPRINT_SCHEMA_VERSION.to_string()
        );

        let reopened = TextIntelligence::new(EngineConfig::default())
            .with_store(RedbStore::open(&path).unwrap());
        assert_eq!(reopened.document_count().unwrap(), 2);
        assert_eq!(
            reopened.find_similar("compra ahora", 1).unwrap()[0].id,
            "one"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rejects_foreign_schema_versions() {
        use crate::{EngineConfig, TextIntelligence};

        let path = test_path("schema");
        {
            let database = Database::create(&path).unwrap();
            let transaction = database.begin_write().unwrap();
            {
                let mut meta = transaction.open_table(META).unwrap();
                meta.insert(META_SCHEMA_VERSION, "999").unwrap();
            }
            transaction.commit().unwrap();
        }
        assert!(RedbStore::open(&path).is_err());
        let _ = std::fs::remove_file(&path);

        // Fingerprint payloads from newer schemas are rejected, never read.
        let path = test_path("fingerprint");
        let fingerprint = {
            let engine = TextIntelligence::new(EngineConfig::default())
                .with_store(RedbStore::open(&path).unwrap());
            let mut fingerprint = engine.analyze("hello").unwrap();
            fingerprint.schema_version = FINGERPRINT_SCHEMA_VERSION + 1;
            fingerprint
        };
        let database = Database::create(&path).unwrap();
        let transaction = database.begin_write().unwrap();
        {
            let mut records = transaction.open_table(RECORDS).unwrap();
            records
                .insert("bad", serde_json::to_vec(&fingerprint).unwrap().as_slice())
                .unwrap();
        }
        transaction.commit().unwrap();
        assert!(RedbStore::open(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }

    fn engine_metadata(path: &Path) -> BTreeMap<String, String> {
        RedbStore::open(path).unwrap().metadata().unwrap()
    }
}
