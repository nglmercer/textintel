use std::collections::BTreeMap;

use crate::core::capabilities::ProviderCapabilities;
use crate::core::providers::VectorStore;
use crate::core::types::{MessageFingerprint, SearchCandidateSet};
use crate::lexical::minhash::minhash_similarity;
use crate::normalization::unicode::casefold_text;
use std::collections::BTreeSet;

#[derive(Debug, Default, Clone)]
pub struct MemoryStore {
    records: BTreeMap<String, MessageFingerprint>,
    lexical_index: BTreeMap<String, BTreeSet<String>>,
    symbol_index: BTreeMap<String, BTreeSet<String>>,
    normalized_index: BTreeMap<String, BTreeSet<String>>,
    minhash_index: BTreeMap<String, BTreeSet<String>>,
    semantic_index: BTreeMap<String, BTreeSet<String>>,
    phonetic_index: BTreeMap<String, BTreeSet<String>>,
}

impl MemoryStore {
    pub fn get(&self, id: &str) -> Option<&MessageFingerprint> {
        self.records.get(id)
    }

    fn index_record(&mut self, id: &str, fingerprint: &MessageFingerprint) {
        for value in fingerprint
            .tokens
            .iter()
            .chain(fingerprint.lemmas.iter())
            .map(|value| casefold_text(value))
        {
            if !value.is_empty() {
                self.lexical_index
                    .entry(value)
                    .or_default()
                    .insert(id.to_string());
            }
        }
        for symbol in &fingerprint.symbols {
            self.symbol_index
                .entry(symbol.raw.clone())
                .or_default()
                .insert(id.to_string());
        }
        if let Some(normalized) = &fingerprint.normalized {
            self.normalized_index
                .entry(casefold_text(normalized))
                .or_default()
                .insert(id.to_string());
        }
        for (position, value) in fingerprint.lexical_features.minhash.iter().enumerate() {
            self.minhash_index
                .entry(format!("{position}:{value}"))
                .or_default()
                .insert(id.to_string());
        }
        for embedding in fingerprint.semantic_embeddings.values() {
            for bucket in semantic_buckets(embedding) {
                self.semantic_index
                    .entry(bucket)
                    .or_default()
                    .insert(id.to_string());
            }
        }
        for candidate in &fingerprint.phonetic_candidates {
            for phoneme in &candidate.phonemes {
                self.phonetic_index
                    .entry(format!("{}:{phoneme}", candidate.language))
                    .or_default()
                    .insert(id.to_string());
            }
        }
    }

    fn deindex_record(&mut self, id: &str, fingerprint: &MessageFingerprint) {
        for value in fingerprint
            .tokens
            .iter()
            .chain(fingerprint.lemmas.iter())
            .map(|value| casefold_text(value))
        {
            remove_index_value(&mut self.lexical_index, &value, id);
        }
        for symbol in &fingerprint.symbols {
            remove_index_value(&mut self.symbol_index, &symbol.raw, id);
        }
        if let Some(normalized) = &fingerprint.normalized {
            remove_index_value(&mut self.normalized_index, &casefold_text(normalized), id);
        }
        for (position, value) in fingerprint.lexical_features.minhash.iter().enumerate() {
            remove_index_value(&mut self.minhash_index, &format!("{position}:{value}"), id);
        }
        for embedding in fingerprint.semantic_embeddings.values() {
            for bucket in semantic_buckets(embedding) {
                remove_index_value(&mut self.semantic_index, &bucket, id);
            }
        }
        for candidate in &fingerprint.phonetic_candidates {
            for phoneme in &candidate.phonemes {
                remove_index_value(
                    &mut self.phonetic_index,
                    &format!("{}:{phoneme}", candidate.language),
                    id,
                );
            }
        }
    }

    pub fn search_candidates(
        &self,
        query: &MessageFingerprint,
        limit: usize,
    ) -> Vec<(String, MessageFingerprint)> {
        self.search_candidates_with_metadata(query, limit).records
    }

    pub fn search_candidates_with_metadata(
        &self,
        query: &MessageFingerprint,
        limit: usize,
    ) -> SearchCandidateSet {
        let limit = limit.max(1);
        if self.records.len() <= limit {
            return SearchCandidateSet {
                records: self.records(),
                channels: vec!["full_small_store".to_string()],
            };
        }
        let mut candidate_ids = BTreeSet::new();
        let mut channels = BTreeSet::new();
        for value in query
            .tokens
            .iter()
            .chain(query.lemmas.iter())
            .map(|value| casefold_text(value))
        {
            if let Some(ids) = self.lexical_index.get(&value) {
                candidate_ids.extend(ids.iter().cloned());
                channels.insert("lexical".to_string());
            }
        }
        for symbol in &query.symbols {
            if let Some(ids) = self.symbol_index.get(&symbol.raw) {
                candidate_ids.extend(ids.iter().cloned());
                channels.insert("symbol".to_string());
            }
        }
        if let Some(normalized) = &query.normalized {
            if let Some(ids) = self.normalized_index.get(&casefold_text(normalized)) {
                candidate_ids.extend(ids.iter().cloned());
                channels.insert("normalized".to_string());
            }
        }
        for (position, value) in query.lexical_features.minhash.iter().enumerate() {
            if let Some(ids) = self.minhash_index.get(&format!("{position}:{value}")) {
                candidate_ids.extend(ids.iter().cloned());
                channels.insert("minhash".to_string());
            }
        }
        for embedding in query.semantic_embeddings.values() {
            for bucket in semantic_buckets(embedding) {
                if let Some(ids) = self.semantic_index.get(&bucket) {
                    candidate_ids.extend(ids.iter().cloned());
                    channels.insert("semantic".to_string());
                }
            }
        }
        for candidate in &query.phonetic_candidates {
            for phoneme in &candidate.phonemes {
                if let Some(ids) = self
                    .phonetic_index
                    .get(&format!("{}:{phoneme}", candidate.language))
                {
                    candidate_ids.extend(ids.iter().cloned());
                    channels.insert("phonetic".to_string());
                }
            }
        }
        let query_terms = query
            .tokens
            .iter()
            .chain(query.lemmas.iter())
            .map(|value| casefold_text(value))
            .collect::<BTreeSet<_>>();
        let mut ranked = candidate_ids
            .into_iter()
            .filter_map(|id| {
                self.records.get(&id).map(|fingerprint| {
                    let terms = fingerprint
                        .tokens
                        .iter()
                        .chain(fingerprint.lemmas.iter())
                        .map(|value| casefold_text(value))
                        .collect::<BTreeSet<_>>();
                    let overlap = if query_terms.is_empty() {
                        0.0
                    } else {
                        query_terms.intersection(&terms).count() as f64 / query_terms.len() as f64
                    };
                    let minhash = minhash_similarity(
                        &query.lexical_features.minhash,
                        &fingerprint.lexical_features.minhash,
                    );
                    let exact = usize::from(query.normalized == fingerprint.normalized) as f64;
                    (
                        0.55 * overlap + 0.35 * minhash + 0.10 * exact,
                        id,
                        fingerprint.clone(),
                    )
                })
            })
            .collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .0
                .total_cmp(&left.0)
                .then_with(|| left.1.cmp(&right.1))
        });
        let output = ranked
            .into_iter()
            .take(limit)
            .map(|(_, id, fingerprint)| (id, fingerprint))
            .collect::<Vec<_>>();
        SearchCandidateSet {
            records: output,
            channels: channels.into_iter().collect(),
        }
    }
}

impl VectorStore for MemoryStore {
    fn upsert(&mut self, id: String, fingerprint: MessageFingerprint) -> Result<(), String> {
        if let Some(previous) = self.records.remove(&id) {
            self.deindex_record(&id, &previous);
        }
        self.index_record(&id, &fingerprint);
        self.records.insert(id, fingerprint);
        Ok(())
    }

    fn remove(&mut self, id: &str) -> Result<bool, String> {
        if let Some(previous) = self.records.remove(id) {
            self.deindex_record(id, &previous);
            Ok(true)
        } else {
            Ok(false)
        }
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

    fn search_candidates(
        &self,
        query: &MessageFingerprint,
        limit: usize,
    ) -> Result<Vec<(String, MessageFingerprint)>, String> {
        Ok(MemoryStore::search_candidates(self, query, limit))
    }

    fn search_candidates_with_metadata(
        &self,
        query: &MessageFingerprint,
        limit: usize,
    ) -> Result<SearchCandidateSet, String> {
        Ok(MemoryStore::search_candidates_with_metadata(
            self, query, limit,
        ))
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("memory_store")
    }
}

fn remove_index_value(index: &mut BTreeMap<String, BTreeSet<String>>, key: &str, id: &str) {
    let mut remove_key = false;
    if let Some(ids) = index.get_mut(key) {
        ids.remove(id);
        remove_key = ids.is_empty();
    }
    if remove_key {
        index.remove(key);
    }
}

fn semantic_buckets(vector: &[f32]) -> Vec<String> {
    vector
        .iter()
        .take(8)
        .enumerate()
        .map(|(index, value)| format!("{index}:{}", (value * 10.0).round() as i32))
        .collect()
}
