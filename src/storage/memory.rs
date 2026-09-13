use std::collections::BTreeMap;

use crate::core::providers::VectorStore;
use crate::core::types::MessageFingerprint;

#[derive(Debug, Default, Clone)]
pub struct MemoryStore {
    records: BTreeMap<String, MessageFingerprint>,
}

impl MemoryStore {
    pub fn get(&self, id: &str) -> Option<&MessageFingerprint> {
        self.records.get(id)
    }
}

impl VectorStore for MemoryStore {
    fn upsert(&mut self, id: String, fingerprint: MessageFingerprint) -> Result<(), String> {
        self.records.insert(id, fingerprint);
        Ok(())
    }

    fn remove(&mut self, id: &str) -> Result<bool, String> {
        Ok(self.records.remove(id).is_some())
    }

    fn len(&self) -> usize {
        self.records.len()
    }

    fn records(&self) -> Vec<(String, MessageFingerprint)> {
        self.records
            .iter()
            .map(|(id, fingerprint)| (id.clone(), fingerprint.clone()))
            .collect()
    }
}
