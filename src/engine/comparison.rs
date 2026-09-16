//! Engine comparison: pair scoring plus duplicate judgments.

use std::time::Instant;

use crate::comparison::model::score_fingerprints_with_profile;
use crate::comparison::scorer::score_fingerprints as weighted_score_fingerprints;
use crate::core::error::TextIntelError;
use crate::core::types::{
    ComparisonResult, DuplicateMode, DuplicateResult, MessageFingerprint, StageTimings,
};
use crate::detection::duplicates::{duplicate_result, duplicate_result_with_mode};
use crate::engine::TextIntelligence;

impl TextIntelligence {
    pub(super) fn score_pair(
        &self,
        left: &MessageFingerprint,
        right: &MessageFingerprint,
    ) -> ComparisonResult {
        if let Some(scorer) = &self.similarity_scorer {
            scorer.score(left, right)
        } else if let Some(profile) = &self.similarity_profile {
            score_fingerprints_with_profile(left, right, profile)
        } else {
            weighted_score_fingerprints(left, right, &self.config.similarity_weights)
        }
    }

    pub fn compare_batch(
        &self,
        pairs: &[(String, String)],
    ) -> Result<Vec<ComparisonResult>, TextIntelError> {
        if pairs.len() > self.config.max_batch_size {
            return Err(TextIntelError::InvalidConfiguration(format!(
                "batch size {} exceeds max_batch_size={}",
                pairs.len(),
                self.config.max_batch_size
            )));
        }
        let texts = pairs
            .iter()
            .flat_map(|(left, right)| [left.clone(), right.clone()])
            .collect::<Vec<_>>();
        let fingerprints = self.analyze_batch(&texts)?;
        pairs
            .iter()
            .enumerate()
            .map(|(index, _)| {
                Ok(self.score_pair(&fingerprints[index * 2], &fingerprints[index * 2 + 1]))
            })
            .collect()
    }

    pub fn compare(&self, left: &str, right: &str) -> Result<ComparisonResult, TextIntelError> {
        Ok(self.compare_with_timing(left, right)?.0)
    }

    /// [`compare`](Self::compare) plus per-stage timings. The two `analyze`
    /// stages are summed per stage; `comparison` holds the scoring step and
    /// `total` the whole call. Timings never contain user text.
    pub fn compare_with_timing(
        &self,
        left: &str,
        right: &str,
    ) -> Result<(ComparisonResult, StageTimings), TextIntelError> {
        let total_started = Instant::now();
        let (left_fp, left_timings) = self.analyze_with_timing(left)?;
        let (right_fp, right_timings) = self.analyze_with_timing(right)?;
        let started = Instant::now();
        let result = self.score_pair(&left_fp, &right_fp);
        let mut timings = StageTimings {
            normalization_micros: left_timings.normalization_micros
                + right_timings.normalization_micros,
            language_micros: left_timings.language_micros + right_timings.language_micros,
            symbols_micros: left_timings.symbols_micros + right_timings.symbols_micros,
            rebus_micros: left_timings.rebus_micros + right_timings.rebus_micros,
            semantic_micros: left_timings.semantic_micros + right_timings.semantic_micros,
            phonetic_micros: left_timings.phonetic_micros + right_timings.phonetic_micros,
            comparison_micros: started.elapsed().as_secs_f64() * 1_000_000.0,
            total_micros: 0.0,
        };
        timings.total_micros = total_started.elapsed().as_secs_f64() * 1_000_000.0;
        Ok((result, timings))
    }

    pub fn compare_fingerprints(
        &self,
        left: &MessageFingerprint,
        right: &MessageFingerprint,
    ) -> ComparisonResult {
        self.score_pair(left, right)
    }

    pub fn duplicate(
        &self,
        left: &str,
        right: &str,
        threshold: f64,
    ) -> Result<DuplicateResult, TextIntelError> {
        let left = self.analyze(left)?;
        let right = self.analyze(right)?;
        Ok(duplicate_result(
            &left,
            &right,
            threshold.clamp(0.0, 1.0),
            &self.config.similarity_weights,
        ))
    }

    pub fn duplicate_with_mode(
        &self,
        left: &str,
        right: &str,
        threshold: f64,
        mode: DuplicateMode,
    ) -> Result<DuplicateResult, TextIntelError> {
        let left = self.analyze(left)?;
        let right = self.analyze(right)?;
        Ok(duplicate_result_with_mode(
            &left,
            &right,
            threshold.clamp(0.0, 1.0),
            &self.config.similarity_weights,
            mode,
        ))
    }
}
