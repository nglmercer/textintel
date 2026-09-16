//! Engine patterns: the supervised pattern registry (match, persist,
//! reload) plus spam detection, which scores fingerprints against the
//! registered patterns.

use std::path::Path;

use crate::core::error::TextIntelError;
use crate::core::types::{MessageFingerprint, Pattern, PatternMatch};
use crate::detection::patterns::match_pattern_fingerprint;
use crate::engine::TextIntelligence;

pub(super) struct RegisteredPattern {
    pub(super) pattern: Pattern,
    pub(super) examples: Vec<(String, MessageFingerprint)>,
}

impl TextIntelligence {
    pub fn add_pattern(
        &self,
        id: impl Into<String>,
        examples: Vec<String>,
    ) -> Result<(), TextIntelError> {
        let id = id.into();
        self.add_pattern_with_options(Pattern {
            id,
            examples,
            negative_examples: Vec::new(),
            threshold: 0.75,
            languages: Vec::new(),
            tags: Vec::new(),
            enabled_channels: Vec::new(),
        })
    }

    pub fn add_pattern_with_options(&self, pattern: Pattern) -> Result<(), TextIntelError> {
        if pattern.id.trim().is_empty() || pattern.examples.is_empty() {
            return Err(TextIntelError::InvalidConfiguration(
                "pattern id and examples are required".to_string(),
            ));
        }
        if !pattern.threshold.is_finite() || !(0.0..=1.0).contains(&pattern.threshold) {
            return Err(TextIntelError::InvalidConfiguration(
                "pattern threshold must be between 0 and 1".to_string(),
            ));
        }
        let id = pattern.id.clone();
        let examples = pattern.examples.clone();
        let mut analyzed = Vec::with_capacity(examples.len());
        for example in &examples {
            analyzed.push((example.clone(), self.analyze(example)?));
        }
        let pattern = RegisteredPattern {
            pattern,
            examples: analyzed,
        };
        self.patterns
            .write()
            .map_err(|_| TextIntelError::Storage("pattern lock poisoned".to_string()))?
            .insert(id, pattern);
        Ok(())
    }

    pub fn remove_pattern(&self, id: &str) -> Result<bool, TextIntelError> {
        Ok(self
            .patterns
            .write()
            .map_err(|_| TextIntelError::Storage("pattern lock poisoned".to_string()))?
            .remove(id)
            .is_some())
    }

    /// Snapshot the registered pattern definitions (without analyzed
    /// fingerprints) for persistence or inspection.
    pub fn pattern_definitions(&self) -> Result<Vec<Pattern>, TextIntelError> {
        let patterns = self
            .patterns
            .read()
            .map_err(|_| TextIntelError::Storage("pattern lock poisoned".to_string()))?;
        Ok(patterns
            .values()
            .map(|registered| registered.pattern.clone())
            .collect())
    }

    /// Persist registered pattern definitions to `path` in a versioned
    /// envelope. Example fingerprints are re-analyzed on load, so the file
    /// stays valid across fingerprint schema upgrades.
    pub fn save_patterns_to(&self, path: impl AsRef<Path>) -> Result<(), TextIntelError> {
        let patterns = self.pattern_definitions()?;
        crate::storage::patterns::save_patterns_to(path.as_ref(), &patterns)
            .map_err(TextIntelError::Storage)
    }

    /// Load pattern definitions from `path`, validating and re-analyzing
    /// every record. Returns the number of patterns registered.
    pub fn load_patterns_from(&self, path: impl AsRef<Path>) -> Result<usize, TextIntelError> {
        let patterns = crate::storage::patterns::load_patterns_from(path.as_ref())
            .map_err(TextIntelError::Storage)?;
        let count = patterns.len();
        for pattern in patterns {
            self.add_pattern_with_options(pattern)?;
        }
        Ok(count)
    }

    pub fn match_patterns(&self, text: &str) -> Result<Vec<PatternMatch>, TextIntelError> {
        let query = self.analyze(text)?;
        let patterns = self
            .patterns
            .read()
            .map_err(|_| TextIntelError::Storage("pattern lock poisoned".to_string()))?;
        let mut matches = patterns
            .values()
            .filter_map(|registered| {
                match_pattern_fingerprint(
                    &query,
                    &registered.pattern,
                    &registered.examples,
                    &self.config.similarity_weights,
                )
            })
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| right.score.total_cmp(&left.score));
        Ok(matches)
    }

    pub fn detect_spam(
        &self,
        text: &str,
    ) -> Result<crate::core::types::SpamResult, TextIntelError> {
        let fingerprint = self.analyze(text)?;
        let patterns = self.match_patterns(text)?;
        self.spam_predictor
            .predict(&fingerprint, &patterns)
            .map_err(TextIntelError::from)
    }
}
