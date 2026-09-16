use std::collections::BTreeMap;

use crate::core::capabilities::{CapabilityLevel, ProviderCapabilities};
use crate::core::providers::{VectorStore, VectorStoreCapabilities};
use crate::core::types::{MessageFingerprint, SearchCandidateSet};
use crate::lexical::minhash::minhash_similarity;
use crate::normalization::unicode::casefold_text;
use std::collections::BTreeSet;
#[cfg(feature = "ann-hnsw")]
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct MemoryStore {
    records: BTreeMap<String, MessageFingerprint>,
    lexical_index: BTreeMap<String, BTreeSet<String>>,
    symbol_index: BTreeMap<String, BTreeSet<String>>,
    normalized_index: BTreeMap<String, BTreeSet<String>>,
    minhash_index: BTreeMap<String, BTreeSet<String>>,
    semantic_index: BTreeMap<String, BTreeSet<String>>,
    phonetic_index: BTreeMap<String, BTreeSet<String>>,
    transliteration_index: BTreeMap<String, BTreeSet<String>>,
    decoded_index: BTreeMap<String, BTreeSet<String>>,
    concept_index: BTreeMap<String, BTreeSet<String>>,
    char_index: BTreeMap<String, BTreeSet<String>>,
    /// Per-channel candidate bound before the union is ranked and cut.
    per_channel: usize,
    /// Semantic-ANN channel bound (capped further by the query limit).
    ann_candidates: usize,
    /// Optional HNSW accelerator over whole-text embeddings. Best-effort
    /// retrieval only; ranking always uses full fingerprint comparison.
    #[cfg(feature = "ann-hnsw")]
    ann: Option<Arc<crate::storage::ann::HnswVectorIndex>>,
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self {
            records: BTreeMap::new(),
            lexical_index: BTreeMap::new(),
            symbol_index: BTreeMap::new(),
            normalized_index: BTreeMap::new(),
            minhash_index: BTreeMap::new(),
            semantic_index: BTreeMap::new(),
            phonetic_index: BTreeMap::new(),
            transliteration_index: BTreeMap::new(),
            decoded_index: BTreeMap::new(),
            concept_index: BTreeMap::new(),
            char_index: BTreeMap::new(),
            per_channel: DEFAULT_PER_CHANNEL_CANDIDATES,
            ann_candidates: DEFAULT_ANN_CANDIDATES,
            #[cfg(feature = "ann-hnsw")]
            ann: None,
        }
    }
}

/// Default per-channel candidate bound (see
/// [`EngineConfig::max_per_channel_candidates`](crate::core::config::EngineConfig)).
pub const DEFAULT_PER_CHANNEL_CANDIDATES: usize = 200;
/// Default ANN channel bound (see
/// [`EngineConfig::max_ann_candidates`](crate::core::config::EngineConfig)).
pub const DEFAULT_ANN_CANDIDATES: usize = 100;
/// Character trigram window: only the first bytes of casefolded text are
/// indexed, keeping the trigram channel bounded for long documents.
const CHAR_INDEX_PREFIX: usize = 256;

impl MemoryStore {
    pub fn get(&self, id: &str) -> Option<&MessageFingerprint> {
        self.records.get(id)
    }

    /// Bound the candidate union: each retrieval channel contributes at most
    /// `per_channel` IDs and the semantic-ANN channel at most
    /// `ann_candidates` (capped further by the query limit). No unbounded
    /// candidate expansion.
    pub fn with_retrieval_limits(mut self, per_channel: usize, ann_candidates: usize) -> Self {
        self.per_channel = per_channel.max(1);
        self.ann_candidates = ann_candidates.max(1);
        self
    }

    /// Apply retrieval limits in place (used by persistent stores built from
    /// [`EngineConfig`](crate::core::config::EngineConfig)).
    pub fn set_retrieval_limits(&mut self, per_channel: usize, ann_candidates: usize) {
        self.per_channel = per_channel.max(1);
        self.ann_candidates = ann_candidates.max(1);
    }

    pub fn per_channel_limit(&self) -> usize {
        self.per_channel
    }

    pub fn ann_candidate_limit(&self) -> usize {
        self.ann_candidates
    }

    /// Build a store with an HNSW semantic accelerator for `dimensions`-wide
    /// whole-text embeddings holding up to `max_elements` entries.
    #[cfg(feature = "ann-hnsw")]
    pub fn with_ann(dimensions: usize, max_elements: usize) -> Result<Self, String> {
        Ok(Self {
            ann: Some(Arc::new(crate::storage::ann::HnswVectorIndex::new(
                dimensions,
                max_elements,
            )?)),
            ..Self::default()
        })
    }

    /// The HNSW accelerator, if configured.
    #[cfg(feature = "ann-hnsw")]
    pub fn ann_index(&self) -> Option<&crate::storage::ann::HnswVectorIndex> {
        self.ann.as_deref()
    }

    /// Attach an HNSW accelerator to a store that was loaded without one by
    /// deterministically rebuilding the graph from the live records (the
    /// documented persistence strategy). Records without a usable
    /// whole-text embedding are skipped, as are records whose embedding
    /// model differs from the store's majority model (comparing vectors
    /// across model revisions is meaningless); returns the live entries
    /// indexed. Replaces any previously configured accelerator.
    #[cfg(feature = "ann-hnsw")]
    pub fn enable_ann(&mut self, dimensions: usize, max_elements: usize) -> Result<usize, String> {
        let snapshot = crate::storage::ann::HnswVectorIndex::snapshot_store_with_models(self);
        let rebuilt = crate::storage::ann::HnswVectorIndex::rebuild_with_models(
            dimensions,
            max_elements,
            &snapshot,
        )
        .map_err(|error| format!("ann rebuild failed for {} records: {error}", snapshot.len()))?;
        let live = rebuilt.len();
        self.ann = Some(Arc::new(rebuilt));
        Ok(live)
    }

    /// Embedding model backing the ANN vectors (`model_id@revision`) when
    /// the index tracks one; `None` without ANN or without tracked revision.
    fn ann_model(&self) -> Option<String> {
        #[cfg(feature = "ann-hnsw")]
        {
            self.ann
                .as_ref()
                .and_then(|index| index.model().map(str::to_string))
        }
        #[cfg(not(feature = "ann-hnsw"))]
        {
            None
        }
    }

    /// Retrieval channels this store serves, including `semantic_ann` when an
    /// ANN index is actually configured.
    pub fn indexed_channels(&self) -> Vec<String> {
        // Without `ann-hnsw` nothing is pushed; the `mut` is only needed
        // when the feature can extend the list.
        #[cfg_attr(not(feature = "ann-hnsw"), allow(unused_mut))]
        let mut channels = [
            "lexical",
            "symbol",
            "normalized",
            "minhash",
            "semantic",
            "phonetic",
            "transliteration",
            "decoded",
            "concept",
            "character",
        ]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
        #[cfg(feature = "ann-hnsw")]
        if self.ann.is_some() {
            channels.push("semantic_ann".to_string());
        }
        channels
    }

    /// Deterministic ANN rebuild from this store's live records (the
    /// documented persistence strategy: the graph is never persisted, only
    /// rebuilt on startup). Keeps the configured dimensions/capacity,
    /// compacts tombstoned removals, re-validates embedding dimensions and
    /// model revisions (incompatible vectors are excluded, never mixed),
    /// and returns the live entries indexed. Fails when no ANN accelerator
    /// is configured.
    #[cfg(feature = "ann-hnsw")]
    pub fn rebuild_ann(&mut self) -> Result<usize, String> {
        let Some(ann) = self.ann.as_ref() else {
            return Err("hnsw_ann: no ANN accelerator configured".to_string());
        };
        let snapshot = crate::storage::ann::HnswVectorIndex::snapshot_store_with_models(self);
        let rebuilt = crate::storage::ann::HnswVectorIndex::rebuild_with_models(
            ann.dimensions(),
            ann.max_elements(),
            &snapshot,
        )?;
        let live = rebuilt.len();
        self.ann = Some(Arc::new(rebuilt));
        Ok(live)
    }

    #[cfg(feature = "ann-hnsw")]
    fn index_ann(&self, id: &str, fingerprint: &MessageFingerprint) -> Result<(), String> {
        let Some(ann) = self.ann.as_ref() else {
            return Ok(());
        };
        // Fingerprints without embeddings (e.g. null backend) simply do not
        // participate in ANN retrieval.
        let Some(vector) = fingerprint.semantic_embeddings.get("default") else {
            return Ok(());
        };
        // Revision validation: vectors from a different embedding model than
        // the index stay out (they remain searchable through the exact
        // channels). Unknown revisions on either side serve (backward
        // compatibility with pre-revision payloads).
        if let (Some(index_model), Some(record_model)) =
            (ann.model(), embedding_model_of(fingerprint))
        {
            if index_model != record_model {
                return Ok(());
            }
        }
        ann.insert(id, vector)
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
        // Transliteration retrieval: converted-view tokens let cross-script
        // queries meet documents through the shared script.
        for (view, _) in fingerprint.transliteration_views() {
            for token in casefold_text(view).split_whitespace() {
                if !token.is_empty() {
                    self.transliteration_index
                        .entry(token.to_string())
                        .or_default()
                        .insert(id.to_string());
                }
            }
        }
        // Rebus/decoded retrieval: decoded readings let obfuscated queries
        // meet their plain documents.
        for candidate in &fingerprint.rebus_candidates {
            for token in casefold_text(&candidate.text).split_whitespace() {
                if !token.is_empty() {
                    self.decoded_index
                        .entry(token.to_string())
                        .or_default()
                        .insert(id.to_string());
                }
            }
        }
        // Symbol/concept retrieval: concept IDs plus reading texts.
        for symbol in &fingerprint.symbols {
            for concept in &symbol.concepts {
                if !concept.id.is_empty() {
                    self.concept_index
                        .entry(concept.id.clone())
                        .or_default()
                        .insert(id.to_string());
                }
            }
            for reading in &symbol.readings {
                let folded = casefold_text(&reading.text);
                if !folded.is_empty() {
                    self.concept_index
                        .entry(folded)
                        .or_default()
                        .insert(id.to_string());
                }
            }
        }
        // Character retrieval: bounded trigram prefix for typo resilience.
        for gram in char_trigrams(&fingerprint.raw) {
            self.char_index
                .entry(gram)
                .or_default()
                .insert(id.to_string());
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
        for (view, _) in fingerprint.transliteration_views() {
            for token in casefold_text(view).split_whitespace() {
                remove_index_value(&mut self.transliteration_index, token, id);
            }
        }
        for candidate in &fingerprint.rebus_candidates {
            for token in casefold_text(&candidate.text).split_whitespace() {
                remove_index_value(&mut self.decoded_index, token, id);
            }
        }
        for symbol in &fingerprint.symbols {
            for concept in &symbol.concepts {
                remove_index_value(&mut self.concept_index, &concept.id, id);
            }
            for reading in &symbol.readings {
                remove_index_value(&mut self.concept_index, &casefold_text(&reading.text), id);
            }
        }
        for gram in char_trigrams(&fingerprint.raw) {
            remove_index_value(&mut self.char_index, &gram, id);
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
        // Candidate union across retrieval channels. Each channel contributes
        // at most `per_channel` IDs (deterministic sorted order) into a
        // deduplicated set; the union is ranked heuristically and cut to
        // `limit`. Every channel is retrieval-only: the engine scores the
        // union with full fingerprint comparison afterwards, so no channel
        // bypasses comparison.
        let mut candidate_ids = BTreeSet::new();
        let mut channels = BTreeSet::new();
        let cap = self.per_channel;
        for value in query
            .tokens
            .iter()
            .chain(query.lemmas.iter())
            .map(|value| casefold_text(value))
        {
            if let Some(ids) = self.lexical_index.get(&value) {
                extend_capped(&mut candidate_ids, &mut channels, ids, "lexical", cap);
            }
        }
        for symbol in &query.symbols {
            if let Some(ids) = self.symbol_index.get(&symbol.raw) {
                extend_capped(&mut candidate_ids, &mut channels, ids, "symbol", cap);
            }
            for concept in &symbol.concepts {
                if let Some(ids) = self.concept_index.get(&concept.id) {
                    extend_capped(&mut candidate_ids, &mut channels, ids, "concept", cap);
                }
            }
            for reading in &symbol.readings {
                if let Some(ids) = self.concept_index.get(&casefold_text(&reading.text)) {
                    extend_capped(&mut candidate_ids, &mut channels, ids, "concept", cap);
                }
            }
        }
        if let Some(normalized) = &query.normalized {
            if let Some(ids) = self.normalized_index.get(&casefold_text(normalized)) {
                extend_capped(&mut candidate_ids, &mut channels, ids, "normalized", cap);
            }
            for token in casefold_text(normalized).split_whitespace() {
                if let Some(ids) = self.lexical_index.get(token) {
                    extend_capped(&mut candidate_ids, &mut channels, ids, "lexical", cap);
                }
            }
        }
        for (position, value) in query.lexical_features.minhash.iter().enumerate() {
            if let Some(ids) = self.minhash_index.get(&format!("{position}:{value}")) {
                extend_capped(&mut candidate_ids, &mut channels, ids, "minhash", cap);
            }
        }
        for gram in char_trigrams(&query.raw) {
            if let Some(ids) = self.char_index.get(&gram) {
                extend_capped(&mut candidate_ids, &mut channels, ids, "character", cap);
            }
        }
        for embedding in query.semantic_embeddings.values() {
            for bucket in semantic_buckets(embedding) {
                if let Some(ids) = self.semantic_index.get(&bucket) {
                    extend_capped(&mut candidate_ids, &mut channels, ids, "semantic", cap);
                }
            }
        }
        #[cfg(feature = "ann-hnsw")]
        if let (Some(ann), Some(query_vector)) =
            (self.ann.as_ref(), query.semantic_embeddings.get("default"))
        {
            // Revision validation: a query from a different embedding model
            // than the index skips ANN instead of comparing across revisions.
            let revision_ok = match (ann.model(), embedding_model_of(query)) {
                (Some(index_model), Some(query_model)) => index_model == query_model,
                _ => true,
            };
            if revision_ok {
                let k = limit.min(self.ann_candidates);
                if let Ok(hits) = ann.search(query_vector, k) {
                    for (id, _) in hits {
                        if self.records.contains_key(&id) {
                            candidate_ids.insert(id);
                            channels.insert("semantic_ann".to_string());
                        }
                    }
                }
            }
        }
        for candidate in &query.phonetic_candidates {
            for phoneme in &candidate.phonemes {
                if let Some(ids) = self
                    .phonetic_index
                    .get(&format!("{}:{phoneme}", candidate.language))
                {
                    extend_capped(&mut candidate_ids, &mut channels, ids, "phonetic", cap);
                }
            }
        }
        // Transliteration retrieval, both directions: query raw tokens meet
        // documents through their converted views, and query views meet
        // documents through their raw tokens.
        let mut transliteration_tokens = BTreeSet::new();
        for value in query
            .tokens
            .iter()
            .chain(query.lemmas.iter())
            .map(|value| casefold_text(value))
        {
            if !value.is_empty() {
                transliteration_tokens.insert(value);
            }
        }
        for (view, _) in query.transliteration_views() {
            for token in casefold_text(view).split_whitespace() {
                if !token.is_empty() {
                    transliteration_tokens.insert(token.to_string());
                    if let Some(ids) = self.lexical_index.get(token) {
                        extend_capped(
                            &mut candidate_ids,
                            &mut channels,
                            ids,
                            "transliteration",
                            cap,
                        );
                    }
                }
            }
        }
        for token in &transliteration_tokens {
            if let Some(ids) = self.transliteration_index.get(token) {
                extend_capped(
                    &mut candidate_ids,
                    &mut channels,
                    ids,
                    "transliteration",
                    cap,
                );
            }
        }
        // Rebus/decoded retrieval, both directions: query readings meet
        // documents through raw tokens and vice versa.
        let mut decoded_tokens = BTreeSet::new();
        for candidate in &query.rebus_candidates {
            for token in casefold_text(&candidate.text).split_whitespace() {
                if !token.is_empty() {
                    decoded_tokens.insert(token.to_string());
                    if let Some(ids) = self.lexical_index.get(token) {
                        extend_capped(&mut candidate_ids, &mut channels, ids, "decoded", cap);
                    }
                }
            }
        }
        for token in transliteration_tokens.union(&decoded_tokens) {
            if let Some(ids) = self.decoded_index.get(token) {
                extend_capped(&mut candidate_ids, &mut channels, ids, "decoded", cap);
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
        #[cfg(feature = "ann-hnsw")]
        self.index_ann(&id, &fingerprint)?;
        self.records.insert(id, fingerprint);
        Ok(())
    }

    fn remove(&mut self, id: &str) -> Result<bool, String> {
        if let Some(previous) = self.records.remove(id) {
            self.deindex_record(id, &previous);
            #[cfg(feature = "ann-hnsw")]
            if let Some(ann) = self.ann.as_ref() {
                ann.remove(id);
            }
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
        ProviderCapabilities::new("memory_store").with_quality(CapabilityLevel::Basic)
    }

    fn store_capabilities(&self) -> VectorStoreCapabilities {
        #[cfg(feature = "ann-hnsw")]
        let (ann_enabled, ann_dimensions, ann_entries): (bool, Option<usize>, usize) =
            match self.ann.as_ref() {
                Some(index) => (true, Some(index.dimensions()), index.len()),
                None => (false, None, 0),
            };
        #[cfg(not(feature = "ann-hnsw"))]
        let (ann_enabled, ann_dimensions, ann_entries): (bool, Option<usize>, usize) =
            (false, None, 0);
        VectorStoreCapabilities {
            store_type: "memory".to_string(),
            persistent: false,
            ann_enabled,
            ann_dimensions,
            ann_entries,
            ann_model: self.ann_model(),
            indexed_channels: self.indexed_channels(),
        }
    }
}

/// Add at most `cap` IDs from one channel posting list (deterministic
/// sorted order) into the deduplicated union, recording the channel.
fn extend_capped(
    candidate_ids: &mut BTreeSet<String>,
    channels: &mut BTreeSet<String>,
    ids: &BTreeSet<String>,
    channel: &str,
    cap: usize,
) {
    candidate_ids.extend(ids.iter().take(cap.max(1)).cloned());
    channels.insert(channel.to_string());
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

/// Character trigrams of the casefolded prefix (deduplicated, deterministic
/// order). Short-text typo resilience without indexing whole documents.
fn char_trigrams(raw: &str) -> Vec<String> {
    let folded = casefold_text(raw);
    let prefix: String = folded.chars().take(CHAR_INDEX_PREFIX).collect();
    let chars: Vec<char> = prefix.chars().collect();
    if chars.len() < 3 {
        return Vec::new();
    }
    let mut grams = BTreeSet::new();
    for window in chars.windows(3) {
        grams.insert(window.iter().collect::<String>());
    }
    grams.into_iter().collect()
}

/// Embedding model behind a fingerprint's vectors (`model_id@revision` or
/// `model_id`), from analyzer-recorded metadata. `None` for payloads that
/// predate model recording.
#[cfg(feature = "ann-hnsw")]
fn embedding_model_of(fingerprint: &MessageFingerprint) -> Option<String> {
    let model = fingerprint.metadata.get("semantic_model")?;
    match fingerprint.metadata.get("semantic_revision") {
        Some(revision) => Some(format!("{model}@{revision}")),
        None => Some(model.clone()),
    }
}
