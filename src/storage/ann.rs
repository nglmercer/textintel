//! Optional HNSW semantic index for large-store retrieval.
//!
//! [`HnswVectorIndex`] stores whole-text (`default`) embedding vectors and
//! answers top-k nearest-neighbour queries with cosine distance. It is a
//! retrieval accelerator only: candidates still go through full fingerprint
//! comparison, so approximate recall never becomes a false verdict.
//!
//! Removals are tombstones (HNSW graphs are append-only); callers filter
//! results against live records. Vectors from mixed-dimension stores are
//! rejected at insert; fingerprints without a `default` embedding are
//! skipped.
//!
//! # Persistence strategy: deterministic rebuild on startup
//!
//! The HNSW graph itself is **never persisted**. Only `(document id, whole-text
//! vector)` pairs persist, inside the fingerprint store (JSON/redb). On
//! startup the index is rebuilt by re-inserting those pairs in id-sorted
//! order ([`HnswVectorIndex::rebuild`],
//! [`MemoryStore::rebuild_ann`](crate::storage::MemoryStore::rebuild_ann)).
//! There is no background persister, no WAL, and no version skew to migrate.
//!
//! Rationale: HNSW graphs are append-only with tombstoned removals, so a
//! persisted graph would accumulate dead entries and hinge on an exact
//! `hnsw_rs` version. Rebuilds compact tombstones for free and keep one
//! source of truth (the fingerprint store).
//!
//! # Determinism scope
//!
//! `hnsw_rs` draws per-point graph levels from OS entropy with no seed API,
//! so two HNSW graphs built from the same records can differ structurally
//! and large-index search stays approximate across rebuilds (top hits are
//! stable; see the recall anchors in `tests/ann_search.rs`). Small indexes
//! (at most `EXACT_SEARCH_MAX_LIVE` live entries) answer queries by an
//! exhaustive cosine scan over stored vectors instead, which is exact and
//! bit-deterministic across rebuilds — and cheaper than graph traversal at
//! that scale.

use std::sync::RwLock;

use hnsw_rs::prelude::*;

const PROVIDER: &str = "hnsw_ann";
const MAX_CONNECTIONS: usize = 16;
const MAX_LAYER: usize = 16;
const EF_CONSTRUCTION: usize = 200;

/// Live-entry ceiling for the exact brute-force search path. At most this
/// many live entries, [`HnswVectorIndex::search`] scans stored vectors
/// exhaustively (exact, deterministic); beyond it, queries use the HNSW
/// graph. The ceiling sits below the 2K-vector recall benchmark so that
/// benchmark keeps measuring the approximate path.
const EXACT_SEARCH_MAX_LIVE: usize = 1024;

/// Append-only HNSW index over whole-text embedding vectors.
pub struct HnswVectorIndex {
    index: Hnsw<'static, f32, DistCosine>,
    dimensions: usize,
    max_elements: usize,
    /// Embedding model backing the vectors (`model_id@revision`) when the
    /// owning store tracks one. Queries from a different revision skip this
    /// index instead of comparing across revisions.
    model: Option<String>,
    next_id: RwLock<usize>,
    ids: RwLock<Vec<Option<String>>>,
    /// Stored vectors mirroring `ids` positionally (tombstones are `None`
    /// in both) for the exact small-index search path. Lock order is
    /// always `ids`, then `vectors`, then `next_id`.
    vectors: RwLock<Vec<Option<Vec<f32>>>>,
}

impl std::fmt::Debug for HnswVectorIndex {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HnswVectorIndex")
            .field("dimensions", &self.dimensions)
            .field("max_elements", &self.max_elements)
            .field("len", &self.len())
            .finish()
    }
}

impl HnswVectorIndex {
    /// Create an index for `dimensions`-wide vectors holding up to
    /// `max_elements` entries.
    pub fn new(dimensions: usize, max_elements: usize) -> Result<Self, String> {
        if dimensions == 0 {
            return Err(format!("{PROVIDER}: dimensions must be positive"));
        }
        if max_elements == 0 {
            return Err(format!("{PROVIDER}: max_elements must be positive"));
        }
        Ok(Self {
            index: Hnsw::new(
                MAX_CONNECTIONS,
                max_elements,
                MAX_LAYER,
                EF_CONSTRUCTION,
                DistCosine {},
            ),
            dimensions,
            max_elements,
            model: None,
            next_id: RwLock::new(0),
            ids: RwLock::new(Vec::new()),
            vectors: RwLock::new(Vec::new()),
        })
    }

    pub fn dimensions(&self) -> usize {
        self.dimensions
    }

    pub fn max_elements(&self) -> usize {
        self.max_elements
    }

    /// Embedding model backing the vectors, when tracked.
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    /// Adopt an embedding-model identity (`model_id@revision`). Vectors
    /// indexed afterwards are expected to come from this model; the owning
    /// store skips records and queries from other revisions.
    pub fn set_model(&mut self, model: Option<String>) {
        self.model = model;
    }

    /// Live entries (tombstoned removals excluded).
    pub fn len(&self) -> usize {
        self.ids
            .read()
            .map(|ids| ids.iter().filter(|entry| entry.is_some()).count())
            .unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Rough memory footprint in bytes: two stored vector copies (graph +
    /// exact-scan mirror) plus a constant per-entry graph overhead estimate.
    /// Documented as an estimate, not a measurement.
    pub fn estimate_bytes(&self) -> usize {
        self.len() * (self.dimensions * 8 + 64)
    }

    /// Insert `vector` under `id`. Rejects wrong dimensions and non-finite
    /// values; re-inserting an `id` tombstones the old entry first.
    pub fn insert(&self, id: &str, vector: &[f32]) -> Result<(), String> {
        if vector.len() != self.dimensions {
            return Err(format!(
                "{PROVIDER}: dimension {} does not match index dimension {}",
                vector.len(),
                self.dimensions
            ));
        }
        if vector.iter().any(|value| !value.is_finite()) {
            return Err(format!("{PROVIDER}: vector for {id:?} is not finite"));
        }
        // Fixed lock order (`ids`, `vectors`, `next_id`) everywhere.
        let mut ids = self
            .ids
            .write()
            .map_err(|_| format!("{PROVIDER}: id lock poisoned"))?;
        let mut vectors = self
            .vectors
            .write()
            .map_err(|_| format!("{PROVIDER}: id lock poisoned"))?;
        let mut next = self
            .next_id
            .write()
            .map_err(|_| format!("{PROVIDER}: id lock poisoned"))?;
        for (entry, stored) in ids.iter_mut().zip(vectors.iter_mut()) {
            if entry.as_deref() == Some(id) {
                *entry = None;
                *stored = None;
            }
        }
        if *next >= self.max_elements {
            return Err(format!(
                "{PROVIDER}: index is full ({max} elements)",
                max = self.max_elements
            ));
        }
        let point = *next;
        *next += 1;
        let owned = vector.to_vec();
        ids.push(Some(id.to_string()));
        vectors.push(Some(owned.clone()));
        drop(ids);
        drop(vectors);
        drop(next);
        self.index.insert((&owned, point));
        Ok(())
    }

    /// Rebuild from `(id, vector)` pairs: entries are inserted in id-sorted
    /// order so the same records always replay the same insertion sequence
    /// regardless of input order. Small-index search over the rebuilt index
    /// is exact and deterministic; large-index search stays approximate (see
    /// the module determinism notes). Pairs with the wrong dimension or
    /// non-finite values are skipped (mixed-dimension stores stay loadable);
    /// everything else that fails to insert aborts the rebuild with an error.
    pub fn rebuild(
        dimensions: usize,
        max_elements: usize,
        documents: &[(String, Vec<f32>)],
    ) -> Result<Self, String> {
        let mut sorted: Vec<&(String, Vec<f32>)> = documents.iter().collect();
        sorted.sort_by(|left, right| left.0.cmp(&right.0));
        let index = Self::new(dimensions, max_elements)?;
        for (id, vector) in sorted {
            if vector.len() != dimensions || vector.iter().any(|value| !value.is_finite()) {
                continue;
            }
            index.insert(id, vector)?;
        }
        Ok(index)
    }

    /// Snapshot the indexable pairs of any [`VectorStore`](crate::core::providers::VectorStore):
    /// `(id, default-embedding)` for records that carry one, in store order.
    /// Feed the result to [`Self::rebuild`] after a restart.
    pub fn snapshot_store(
        store: &dyn crate::core::providers::VectorStore,
    ) -> Vec<(String, Vec<f32>)> {
        Self::snapshot_store_with_models(store)
            .into_iter()
            .map(|(id, vector, _)| (id, vector))
            .collect()
    }

    /// Snapshot with embedding-model identity per record
    /// (`model_id@revision` from analyzer metadata, `None` for payloads that
    /// predate model recording). Feed the result to
    /// [`Self::rebuild_with_models`] so the rebuilt index validates
    /// revisions instead of mixing vectors across models.
    pub fn snapshot_store_with_models(
        store: &dyn crate::core::providers::VectorStore,
    ) -> Vec<(String, Vec<f32>, Option<String>)> {
        store
            .records()
            .into_iter()
            .filter_map(|(id, fingerprint)| {
                fingerprint
                    .semantic_embeddings
                    .get("default")
                    .cloned()
                    .map(|vector| {
                        let model =
                            fingerprint.metadata.get("semantic_model").map(
                                |model| match fingerprint.metadata.get("semantic_revision") {
                                    Some(revision) => format!("{model}@{revision}"),
                                    None => model.clone(),
                                },
                            );
                        (id, vector, model)
                    })
            })
            .collect()
    }

    /// Rebuild like [`Self::rebuild`], additionally validating embedding
    /// revisions: only vectors from the majority known model are indexed
    /// (ties break toward the lexicographically smallest model for
    /// determinism); unknown-model vectors join only when no record carries
    /// a known model. The rebuilt index adopts the majority model so later
    /// inserts and queries validate against it; incompatible vectors are
    /// excluded from the index, never mixed in. They remain searchable
    /// through the exact channels.
    pub fn rebuild_with_models(
        dimensions: usize,
        max_elements: usize,
        documents: &[(String, Vec<f32>, Option<String>)],
    ) -> Result<Self, String> {
        use std::collections::BTreeMap;
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for (_, _, model) in documents {
            if let Some(model) = model {
                *counts.entry(model.as_str()).or_default() += 1;
            }
        }
        let majority = counts
            .iter()
            .max_by(|left, right| left.1.cmp(right.1).then_with(|| right.0.cmp(left.0)))
            .map(|(model, _)| (*model).to_string());
        let mut sorted: Vec<&(String, Vec<f32>, Option<String>)> = documents.iter().collect();
        sorted.sort_by(|left, right| left.0.cmp(&right.0));
        let mut index = Self::new(dimensions, max_elements)?;
        index.set_model(majority.clone());
        for (id, vector, model) in sorted {
            if vector.len() != dimensions || vector.iter().any(|value| !value.is_finite()) {
                continue;
            }
            match (&majority, model) {
                (Some(expected), Some(actual)) if expected != actual => continue,
                (Some(_), None) => continue,
                _ => {}
            }
            index.insert(id, vector)?;
        }
        Ok(index)
    }

    /// Tombstone `id`. Returns false when the id was never indexed.
    pub fn remove(&self, id: &str) -> bool {
        let Ok(mut ids) = self.ids.write() else {
            return false;
        };
        let mut found = false;
        for entry in ids.iter_mut() {
            if entry.as_deref() == Some(id) {
                *entry = None;
                found = true;
            }
        }
        found
    }

    /// Top-`k` nearest `(id, cosine_distance)` pairs, lower distance first.
    /// Tombstones are filtered out. Small indexes scan exhaustively (exact,
    /// deterministic); larger ones use the HNSW graph (approximate).
    pub fn search(&self, vector: &[f32], k: usize) -> Result<Vec<(String, f64)>, String> {
        if vector.len() != self.dimensions {
            return Err(format!(
                "{PROVIDER}: query dimension {} does not match index dimension {}",
                vector.len(),
                self.dimensions
            ));
        }
        if k == 0 {
            return Ok(Vec::new());
        }
        if self.len() <= EXACT_SEARCH_MAX_LIVE {
            return self.search_exact(vector, k);
        }
        let ef = (k * 4).clamp(50, 500);
        let neighbours = self.index.search(vector, k, ef);
        let ids = self
            .ids
            .read()
            .map_err(|_| format!("{PROVIDER}: id lock poisoned"))?;
        Ok(neighbours
            .into_iter()
            .filter_map(|neighbour| {
                ids.get(neighbour.d_id)
                    .and_then(|entry| entry.clone())
                    .map(|id| (id, neighbour.distance as f64))
            })
            .collect())
    }

    /// Exhaustive top-`k` cosine scan over live vectors. Total ordering by
    /// `(distance, id)` keeps ties deterministic; zero vectors score the
    /// maximum distance so they sort last, deterministically.
    fn search_exact(&self, vector: &[f32], k: usize) -> Result<Vec<(String, f64)>, String> {
        let ids = self
            .ids
            .read()
            .map_err(|_| format!("{PROVIDER}: id lock poisoned"))?;
        let vectors = self
            .vectors
            .read()
            .map_err(|_| format!("{PROVIDER}: id lock poisoned"))?;
        let mut scored: Vec<(String, f64)> = ids
            .iter()
            .zip(vectors.iter())
            .filter_map(|(entry, stored)| match (entry, stored) {
                (Some(id), Some(stored)) => Some((id.clone(), cosine_distance_f64(vector, stored))),
                _ => None,
            })
            .collect();
        scored.sort_by(|left, right| {
            left.1
                .total_cmp(&right.1)
                .then_with(|| left.0.cmp(&right.0))
        });
        scored.truncate(k);
        Ok(scored)
    }
}

/// Cosine distance `1 - cos(a, b)` in `f64`. Zero vectors score `1.0`.
fn cosine_distance_f64(left: &[f32], right: &[f32]) -> f64 {
    let mut dot = 0.0f64;
    let mut left_norm = 0.0f64;
    let mut right_norm = 0.0f64;
    for (a, b) in left.iter().zip(right.iter()) {
        let (a, b) = (*a as f64, *b as f64);
        dot += a * b;
        left_norm += a * a;
        right_norm += b * b;
    }
    if left_norm == 0.0 || right_norm == 0.0 {
        return 1.0;
    }
    1.0 - dot / (left_norm.sqrt() * right_norm.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Injective test vectors: the first three components encode the seed
    /// in base 7, so distinct seeds give distinct directions.
    fn unit_vector(seed: usize, dimensions: usize) -> Vec<f32> {
        let mut vector = vec![0.0; dimensions];
        vector[0] = (seed % 7 + 1) as f32;
        if dimensions > 1 {
            vector[1] = ((seed / 7) % 7 + 1) as f32;
        }
        if dimensions > 2 {
            vector[2] = ((seed / 49) % 7 + 1) as f32;
        }
        let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
        vector.iter().map(|value| value / norm).collect()
    }

    #[test]
    fn rejects_invalid_construction_and_vectors() {
        assert!(HnswVectorIndex::new(0, 10).is_err());
        assert!(HnswVectorIndex::new(8, 0).is_err());
        let index = HnswVectorIndex::new(8, 10).unwrap();
        assert!(index.is_empty());
        assert!(index.insert("a", &[0.0; 7]).is_err());
        assert!(index.insert("a", &[f32::NAN; 8]).is_err());
        assert!(index.search(&[0.0; 7], 3).is_err());
        assert!(index.search(&[0.0; 8], 0).unwrap().is_empty());
        assert!(!index.remove("missing"));
    }

    #[test]
    fn finds_nearest_and_honors_removals() {
        let index = HnswVectorIndex::new(8, 100).unwrap();
        for point in 0..20 {
            index
                .insert(&format!("doc-{point}"), &unit_vector(point, 8))
                .unwrap();
        }
        assert_eq!(index.len(), 20);
        let query = unit_vector(3, 8);
        let hits = index.search(&query, 3).unwrap();
        assert_eq!(hits[0].0, "doc-3");
        assert!(hits[0].1 <= hits[1].1);
        assert!(index.remove("doc-3"));
        assert_eq!(index.len(), 19);
        let after = index.search(&query, 3).unwrap();
        assert!(after.iter().all(|(id, _)| id != "doc-3"));
        // Re-inserting replaces the tombstone without growing live count.
        index.insert("doc-3", &unit_vector(3, 8)).unwrap();
        assert_eq!(index.len(), 20);
    }

    #[test]
    fn rebuild_is_deterministic_and_compacts_removals() {
        let documents: Vec<(String, Vec<f32>)> = (0..20)
            .map(|point| (format!("doc-{point:02}"), unit_vector(point * 7 + 1, 8)))
            .collect();
        // Insertion order must not matter: rebuild sorts by id.
        let mut shuffled = documents.clone();
        shuffled.reverse();
        let first = HnswVectorIndex::rebuild(8, 100, &documents).unwrap();
        let second = HnswVectorIndex::rebuild(8, 100, &shuffled).unwrap();
        assert_eq!(first.len(), 20);
        assert_eq!(second.len(), 20);
        let query = unit_vector(3 * 7 + 1, 8);
        let hits_first = first.search(&query, 5).unwrap();
        let hits_second = second.search(&query, 5).unwrap();
        assert_eq!(hits_first, hits_second);
        assert_eq!(hits_first[0].0, "doc-03");
        // Wrong-dimension and non-finite pairs are skipped, never fatal.
        let mut mixed = documents;
        mixed.push(("bad-dim".to_string(), vec![0.0; 4]));
        mixed.push(("bad-finite".to_string(), vec![f32::NAN; 8]));
        let rebuilt = HnswVectorIndex::rebuild(8, 100, &mixed).unwrap();
        assert_eq!(rebuilt.len(), 20);
    }

    #[test]
    fn recall_is_high_on_small_exact_sets() {
        // Sanity anchor for the benchmark methodology: brute-force top-10
        // must nearly match ANN top-10 on a tiny seeded set. The index is
        // approximate, so perfection is asserted as recall >= 0.9, never
        // exact equality.
        let dimensions = 16;
        let index = HnswVectorIndex::new(dimensions, 500).unwrap();
        let mut vectors = Vec::new();
        for point in 0..200 {
            let vector = unit_vector(point * 13 + 5, dimensions);
            index.insert(&format!("doc-{point}"), &vector).unwrap();
            vectors.push(vector);
        }
        let query = unit_vector(999, dimensions);
        let mut brute: Vec<(usize, f32)> = vectors
            .iter()
            .enumerate()
            .map(|(position, vector)| {
                let dot: f32 = vector.iter().zip(query.iter()).map(|(a, b)| a * b).sum();
                (position, 1.0 - dot)
            })
            .collect();
        brute.sort_by(|left, right| left.1.total_cmp(&right.1));
        let expected: Vec<String> = brute
            .iter()
            .take(10)
            .map(|(position, _)| format!("doc-{position}"))
            .collect();
        let hits = index.search(&query, 10).unwrap();
        let found: Vec<String> = hits.into_iter().map(|(id, _)| id).collect();
        let expected_set: std::collections::BTreeSet<&str> =
            expected.iter().map(String::as_str).collect();
        let found_set: std::collections::BTreeSet<&str> =
            found.iter().map(String::as_str).collect();
        let recall =
            expected_set.intersection(&found_set).count() as f64 / expected_set.len() as f64;
        assert!(
            recall >= 0.9,
            "recall@10 on seeded set: {recall:.2} (expected {expected:?}, found {found:?})"
        );
    }
}
