//! Evaluation datasets: versioned cases, split filtering, and the held-out
//! spam corpus. Parsing only — scoring lives in `metrics` and the runners.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::core::error::TextIntelError;

fn default_dataset_version() -> String {
    "unversioned".to_string()
}

fn default_split() -> String {
    "test".to_string()
}

fn default_difficulty() -> String {
    "medium".to_string()
}

/// Structured expectations for a single evaluation case.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct ExpectedOutput {
    /// Decoded candidates must contain one of these strings (case-insensitive).
    pub decoded_contains: Vec<String>,
    /// Similar pairs should score at or above this threshold.
    pub min_similarity: Option<f64>,
}

/// Primary evaluation categories. Cases predate the field and are inferred
/// from labels (see [`EvaluationCase::primary_category`]); new cases should
/// set `category` explicitly.
pub const EVALUATION_CATEGORIES: &[&str] = &[
    "semantic",
    "cross_language",
    "transliteration",
    "rebus",
    "phonetic",
    "leetspeak",
    "homoglyph",
    "unicode",
    "code_switching",
    "short_text",
    "spam",
    "hard_negatives",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvaluationCase {
    #[serde(default)]
    pub id: String,
    pub a: String,
    pub b: String,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default = "default_split")]
    pub split: String,
    #[serde(default = "default_difficulty")]
    pub difficulty: String,
    /// Primary category (one of [`EVALUATION_CATEGORIES`] or `general`).
    /// Empty on legacy cases, which are inferred from labels.
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub labels: BTreeMap<String, bool>,
    #[serde(default)]
    pub expected: ExpectedOutput,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl EvaluationCase {
    /// Normalized split name (`train`, `validation`, or `test`).
    pub fn normalized_split(&self) -> &str {
        match self.split.as_str() {
            "train" | "training" => "train",
            "validation" | "valid" | "dev" => "validation",
            _ => "test",
        }
    }

    pub fn is_similar(&self) -> bool {
        self.labels.get("similar").copied().unwrap_or(false)
    }

    /// Primary category for per-category metrics. Explicit `category` wins;
    /// legacy cases without one are inferred from labels so old datasets
    /// still slice meaningfully.
    pub fn primary_category(&self) -> &str {
        if !self.category.is_empty() {
            return &self.category;
        }
        for (label, category) in [
            ("rebus", "rebus"),
            ("homoglyph", "homoglyph"),
            ("phonetic", "phonetic"),
            ("semantic", "semantic"),
            ("short", "short_text"),
            ("spam", "spam"),
            ("visual", "unicode"),
            ("symbolic", "rebus"),
            ("obfuscated", "leetspeak"),
        ] {
            if self.labels.get(label).copied().unwrap_or(false) {
                return category;
            }
        }
        "general"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvaluationDataset {
    #[serde(default = "default_dataset_version")]
    pub version: String,
    pub cases: Vec<EvaluationCase>,
}

/// One labeled spam-corpus message (`spam` or `ham`/`benign`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpamCorpusItem {
    #[serde(default)]
    pub id: String,
    pub text: String,
    pub label: String,
}

impl SpamCorpusItem {
    pub fn is_spam(&self) -> bool {
        self.label.eq_ignore_ascii_case("spam")
    }
}

/// Versioned held-out spam corpus (see `data/spam/README.md`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpamCorpus {
    #[serde(default = "default_dataset_version")]
    pub version: String,
    pub items: Vec<SpamCorpusItem>,
}

impl SpamCorpus {
    pub fn from_json(source: &str) -> Result<Self, TextIntelError> {
        serde_json::from_str(source).map_err(TextIntelError::from)
    }

    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, TextIntelError> {
        let path = path.as_ref();
        let source = std::fs::read_to_string(path).map_err(|error| {
            TextIntelError::InvalidConfiguration(format!("cannot read {}: {error}", path.display()))
        })?;
        let corpus = Self::from_json(&source)?;
        if corpus.items.is_empty() {
            return Err(TextIntelError::InvalidConfiguration(format!(
                "spam corpus {} has no items",
                path.display()
            )));
        }
        Ok(corpus)
    }
}

impl EvaluationDataset {
    pub fn from_json(source: &str) -> Result<Self, TextIntelError> {
        if let Ok(dataset) = serde_json::from_str::<Self>(source) {
            return Ok(dataset);
        }
        let cases = serde_json::from_str(source)?;
        Ok(Self {
            version: "unversioned".to_string(),
            cases,
        })
    }

    /// Load a modular dataset directory (`train.json`, `validation.json`,
    /// `test.json`, each a [`EvaluationDataset`] document). Part versions
    /// must agree and every case must carry its part's split; concatenation
    /// preserves within-split order so split filtering is unaffected.
    pub fn from_dir(path: impl AsRef<std::path::Path>) -> Result<Self, TextIntelError> {
        let root = path.as_ref();
        let mut version: Option<String> = None;
        let mut cases = Vec::new();
        for split in ["train", "validation", "test"] {
            let file = root.join(format!("{split}.json"));
            let source = std::fs::read_to_string(&file).map_err(|error| {
                TextIntelError::InvalidConfiguration(format!(
                    "cannot read {}: {error}",
                    file.display()
                ))
            })?;
            let part = Self::from_json(&source)?;
            match &version {
                None => version = Some(part.version.clone()),
                Some(expected) if expected == &part.version => {}
                Some(expected) => {
                    return Err(TextIntelError::InvalidConfiguration(format!(
                        "dataset part {} has version {}, expected {expected}",
                        file.display(),
                        part.version
                    )));
                }
            }
            for case in part.cases {
                if case.normalized_split() != split {
                    return Err(TextIntelError::InvalidConfiguration(format!(
                        "case {} carries split {:?}, expected {split:?} in {}",
                        case.id,
                        case.split,
                        file.display()
                    )));
                }
                cases.push(case);
            }
        }
        Ok(Self {
            version: version.unwrap_or_else(default_dataset_version),
            cases,
        })
    }

    /// Load a dataset from a directory (see [`EvaluationDataset::from_dir`])
    /// or a single JSON document (see [`EvaluationDataset::from_json`]).
    pub fn load_path(path: impl AsRef<std::path::Path>) -> Result<Self, TextIntelError> {
        let path = path.as_ref();
        if path.is_dir() {
            return Self::from_dir(path);
        }
        let source = std::fs::read_to_string(path).map_err(|error| {
            TextIntelError::InvalidConfiguration(format!("cannot read {}: {error}", path.display()))
        })?;
        Self::from_json(&source)
    }

    /// Cases belonging to `split` (`train`, `validation`, or `test`).
    pub fn filter_split(&self, split: &str) -> Vec<&EvaluationCase> {
        let wanted = match split {
            "train" | "training" => "train",
            "validation" | "valid" | "dev" => "validation",
            _ => "test",
        };
        self.cases
            .iter()
            .filter(|case| case.normalized_split() == wanted)
            .collect()
    }

    pub fn train(&self) -> Vec<&EvaluationCase> {
        self.filter_split("train")
    }

    pub fn validation(&self) -> Vec<&EvaluationCase> {
        self.filter_split("validation")
    }

    pub fn test(&self) -> Vec<&EvaluationCase> {
        self.filter_split("test")
    }

    /// Count cases per split, returned as `(train, validation, test)`.
    pub fn split_counts(&self) -> (usize, usize, usize) {
        (
            self.train().len(),
            self.validation().len(),
            self.test().len(),
        )
    }
}
