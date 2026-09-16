//! Engine search: the document store (add, remove, count) plus
//! similarity search and duplicate listing over indexed fingerprints.

use std::collections::BTreeMap;

use crate::core::error::TextIntelError;
use crate::core::types::{ComparisonResult, DuplicateMode, DuplicateResult, SearchResult};
use crate::detection::duplicates::duplicate_result_with_mode;
use crate::engine::TextIntelligence;

impl TextIntelligence {
    pub fn add_document(&self, id: impl Into<String>, text: &str) -> Result<(), TextIntelError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(TextIntelError::InvalidConfiguration(
                "document id cannot be empty".to_string(),
            ));
        }
        let fingerprint = self.analyze(text)?;
        let mut store = self
            .store
            .write()
            .map_err(|_| TextIntelError::Storage("store lock poisoned".to_string()))?;
        if store.len() >= self.config.max_documents
            && store.records().iter().all(|(current, _)| current != &id)
        {
            return Err(TextIntelError::Storage(format!(
                "max_documents={} reached",
                self.config.max_documents
            )));
        }
        store
            .upsert(id, fingerprint)
            .map_err(TextIntelError::Storage)
    }

    pub fn remove_document(&self, id: &str) -> Result<bool, TextIntelError> {
        self.store
            .write()
            .map_err(|_| TextIntelError::Storage("store lock poisoned".to_string()))?
            .remove(id)
            .map_err(TextIntelError::Storage)
    }

    pub fn document_count(&self) -> Result<usize, TextIntelError> {
        Ok(self
            .store
            .read()
            .map_err(|_| TextIntelError::Storage("store lock poisoned".to_string()))?
            .len())
    }

    pub fn find_similar(
        &self,
        text: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, TextIntelError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let query = self.analyze(text)?;
        let retrieved = self
            .store
            .read()
            .map_err(|_| TextIntelError::Storage("store lock poisoned".to_string()))?
            .search_candidates_with_metadata(&query, self.config.max_search_candidates.max(limit))
            .map_err(TextIntelError::Storage)?;
        let retrieval_channels = retrieved.channels;
        let records = retrieved.records;
        let mut candidates = records
            .into_iter()
            .map(|(id, fingerprint)| {
                let comparison = self.score_pair(&query, &fingerprint);
                (id, fingerprint, comparison)
            })
            .collect::<Vec<_>>();
        let candidate_count = candidates.len();
        candidates.sort_by(|left, right| right.2.score.total_cmp(&left.2.score));
        let results = if let Some(reranker) = &self.reranker_provider {
            let input = candidates
                .iter()
                .map(|(id, fingerprint, comparison)| {
                    (id.clone(), fingerprint.clone(), comparison.score)
                })
                .collect();
            let reranked = reranker
                .rerank(&query, input)
                .map_err(TextIntelError::from)?;
            let mut by_id = candidates
                .into_iter()
                .map(|(id, _, comparison)| (id, comparison))
                .collect::<BTreeMap<_, _>>();
            let mut reranked_results: Vec<(String, ComparisonResult)> = reranked
                .into_iter()
                .filter_map(|(id, _, score)| {
                    by_id.remove(&id).map(|mut comparison| {
                        comparison.score = score.clamp(0.0, 1.0);
                        (id, comparison)
                    })
                })
                .collect();
            reranked_results.sort_by(|left, right| right.1.score.total_cmp(&left.1.score));
            reranked_results
        } else {
            candidates
                .into_iter()
                .map(|(id, _, comparison)| (id, comparison))
                .collect()
        };
        Ok(results
            .into_iter()
            .take(limit)
            .map(|(id, comparison)| SearchResult {
                id,
                score: comparison.score,
                comparison,
                candidate_count,
                retrieval_channels: retrieval_channels.clone(),
            })
            .collect())
    }

    pub fn find_duplicates(
        &self,
        text: &str,
        threshold: f64,
    ) -> Result<Vec<(String, DuplicateResult)>, TextIntelError> {
        self.find_duplicates_with_mode(text, threshold, DuplicateMode::Combined)
    }

    pub fn find_duplicates_with_mode(
        &self,
        text: &str,
        threshold: f64,
        mode: DuplicateMode,
    ) -> Result<Vec<(String, DuplicateResult)>, TextIntelError> {
        let query = self.analyze(text)?;
        let records = self
            .store
            .read()
            .map_err(|_| TextIntelError::Storage("store lock poisoned".to_string()))?
            .search_candidates(&query, self.config.max_search_candidates)
            .map_err(TextIntelError::Storage)?;
        Ok(records
            .into_iter()
            .map(|(id, fingerprint)| {
                (
                    id,
                    duplicate_result_with_mode(
                        &query,
                        &fingerprint,
                        threshold.clamp(0.0, 1.0),
                        &self.config.similarity_weights,
                        mode,
                    ),
                )
            })
            .filter(|(_, result)| result.duplicate)
            .collect())
    }
}
