//! Decision training/evaluation datasets: JSONL examples with gold
//! labels and optional teacher distributions.
//!
//! Layout for `data/decision/`:
//!
//! ```text
//! dataset.json      {"version": "0.1.0"}
//! train.jsonl       one DecisionExample per line
//! validation.jsonl  calibration + threshold fitting (never final test)
//! test.jsonl        held-out evaluation
//! ```
//!
//! ```json
//! {
//!   "state": "I was charged twice. Refund the duplicate payment.",
//!   "question": {
//!     "type": "choice",
//!     "instructions": "Which team should handle this message?",
//!     "criteria": {
//!       "billing": "Payment, invoice and refund problems",
//!       "technical": "Problems operating the software",
//!       "sales": "Questions about purchasing"
//!     }
//!   },
//!   "gold": "billing"
//! }
//! ```

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::core::error::TextIntelError;

use super::scoring::validate_distribution;
use super::types::DecisionQuestion;

/// One labeled decision example. `gold` lives in
/// [`DecisionQuestion::candidate_labels`] space: a criterion id, `"true"` /
/// `"false"`, or a level index as a decimal string.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionExample {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    pub state: String,
    pub question: DecisionQuestion,
    pub gold: String,
    /// Optional soft target from a licensed teacher: label-aligned
    /// distribution over the same labels as `gold`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub teacher_probabilities: Option<BTreeMap<String, f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
}

impl DecisionExample {
    /// Structural validation: the question, the gold label, and the
    /// optional teacher distribution.
    pub fn validate(&self) -> Result<(), String> {
        if self.state.trim().is_empty() {
            return Err(format!("decision example {:?} has an empty state", self.id));
        }
        self.question
            .validate()
            .map_err(|message| format!("decision example {:?}: {message}", self.id))?;
        self.gold_index()?
            .ok_or_else(|| {
                format!(
                    "decision example {:?} has an unknown gold label {:?}",
                    self.id, self.gold
                )
            })
            .map(|_| ())?;
        if let Some(teacher) = &self.teacher_probabilities {
            let labels = self.question.candidate_labels();
            let mut expected = labels.clone();
            expected.sort_unstable();
            let mut observed: Vec<String> = teacher.keys().cloned().collect();
            observed.sort_unstable();
            if expected != observed {
                return Err(format!(
                    "decision example {:?}: teacher probabilities must cover exactly the candidate labels",
                    self.id
                ));
            }
            let values: Vec<f64> = labels.iter().map(|label| teacher[label]).collect();
            validate_distribution(&values, None)
                .map_err(|message| format!("decision example {:?}: teacher {message}", self.id))?;
        }
        Ok(())
    }

    /// Gold label as a candidate index, or `None` when it names no
    /// candidate of the question.
    pub fn gold_index(&self) -> Result<Option<usize>, String> {
        match &self.question {
            DecisionQuestion::Choice { criteria, .. } => {
                let mut ids: Vec<&String> = criteria.keys().collect();
                ids.sort_unstable();
                Ok(ids.iter().position(|id| *id == &self.gold))
            }
            DecisionQuestion::Binary { .. } => match self.gold.as_str() {
                "false" => Ok(Some(0)),
                "true" => Ok(Some(1)),
                _ => Ok(None),
            },
            DecisionQuestion::Score { levels, .. } => match self.gold.parse::<usize>() {
                Ok(index) if index < levels.len() => Ok(Some(index)),
                _ => Ok(None),
            },
        }
    }
}

/// One named split of a decision dataset.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DecisionSplit {
    pub name: String,
    pub examples: Vec<DecisionExample>,
}

/// A versioned decision dataset: `train` / `validation` / `test` splits
/// loaded from a directory.
#[derive(Debug, Clone, PartialEq)]
pub struct DecisionDataset {
    pub version: String,
    pub train: Vec<DecisionExample>,
    pub validation: Vec<DecisionExample>,
    pub test: Vec<DecisionExample>,
}

impl DecisionDataset {
    /// Parse newline-delimited JSON examples, skipping blank lines. Line
    /// numbers are 1-based in error messages.
    pub fn from_jsonl(source: &str) -> Result<Vec<DecisionExample>, String> {
        let mut examples = Vec::new();
        for (index, line) in source.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let example: DecisionExample = serde_json::from_str(line)
                .map_err(|error| format!("line {}: invalid example: {error}", index + 1))?;
            example
                .validate()
                .map_err(|message| format!("line {}: {message}", index + 1))?;
            examples.push(example);
        }
        Ok(examples)
    }

    /// Load `dataset.json` plus `train.jsonl`, `validation.jsonl`, and
    /// `test.jsonl` from `dir`. Every file must exist and every example
    /// must validate; splits may be empty only when the dataset has no
    /// examples at all (which is rejected).
    pub fn from_dir(dir: impl AsRef<std::path::Path>) -> Result<Self, TextIntelError> {
        let dir = dir.as_ref();
        let read = |file: &str| {
            std::fs::read_to_string(dir.join(file)).map_err(|error| {
                TextIntelError::InvalidConfiguration(format!(
                    "cannot read {}: {error}",
                    dir.join(file).display()
                ))
            })
        };
        let manifest: BTreeMap<String, String> = serde_json::from_str(&read("dataset.json")?)
            .map_err(|error: serde_json::Error| {
                TextIntelError::InvalidConfiguration(format!("invalid dataset.json: {error}"))
            })?;
        let version = manifest.get("version").cloned().unwrap_or_default();
        if version.trim().is_empty() {
            return Err(TextIntelError::InvalidConfiguration(
                "dataset.json carries no version".to_string(),
            ));
        }
        let mut dataset = Self {
            version,
            train: Vec::new(),
            validation: Vec::new(),
            test: Vec::new(),
        };
        for (split, file) in [
            ("train", "train.jsonl"),
            ("validation", "validation.jsonl"),
            ("test", "test.jsonl"),
        ] {
            let examples = Self::from_jsonl(&read(file)?).map_err(|message| {
                TextIntelError::InvalidConfiguration(format!("{file}: {message}"))
            })?;
            match split {
                "train" => dataset.train = examples,
                "validation" => dataset.validation = examples,
                _ => dataset.test = examples,
            }
        }
        if dataset.train.is_empty() && dataset.validation.is_empty() && dataset.test.is_empty() {
            return Err(TextIntelError::InvalidConfiguration(
                "decision dataset has no examples".to_string(),
            ));
        }
        Ok(dataset)
    }

    /// Examples for `split` (`train`, `validation`/`valid`/`dev`, or `test`).
    pub fn split(&self, name: &str) -> &[DecisionExample] {
        match name {
            "train" | "training" => &self.train,
            "validation" | "valid" | "dev" => &self.validation,
            _ => &self.test,
        }
    }

    /// Load a single-file JSON eval set (`{"version": ..., "examples":
    /// [...]}`) as a test-only dataset. This is the shared cross-model
    /// comparison format: small, hand-auditable, identical inputs for
    /// every contestant.
    pub fn from_simple_file(path: impl AsRef<std::path::Path>) -> Result<Self, TextIntelError> {
        let path = path.as_ref();
        let source = std::fs::read_to_string(path).map_err(|error| {
            TextIntelError::InvalidConfiguration(format!("cannot read {}: {error}", path.display()))
        })?;
        Self::from_simple_json(&source).map_err(|message| {
            TextIntelError::InvalidConfiguration(format!("{}: {message}", path.display()))
        })
    }

    pub fn from_simple_json(source: &str) -> Result<Self, String> {
        let document: BTreeMap<String, serde_json::Value> =
            serde_json::from_str(source).map_err(|error| format!("invalid eval set: {error}"))?;
        let version = document
            .get("version")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        if version.trim().is_empty() {
            return Err("eval set carries no version".to_string());
        }
        let raw = document
            .get("examples")
            .ok_or_else(|| "eval set has no examples".to_string())?;
        let examples: Vec<DecisionExample> = serde_json::from_value(raw.clone())
            .map_err(|error| format!("invalid examples: {error}"))?;
        if examples.is_empty() {
            return Err("eval set has no examples".to_string());
        }
        for (index, example) in examples.iter().enumerate() {
            example
                .validate()
                .map_err(|message| format!("example {index}: {message}"))?;
        }
        Ok(Self {
            version,
            train: Vec::new(),
            validation: Vec::new(),
            test: examples,
        })
    }

    /// Load a decision dataset from a directory (see [`Self::from_dir`])
    /// or a single-file JSON eval set (see [`Self::from_simple_file`]).
    pub fn load_path(path: impl AsRef<std::path::Path>) -> Result<Self, TextIntelError> {
        let path = path.as_ref();
        if path.is_dir() {
            return Self::from_dir(path);
        }
        Self::from_simple_file(path)
    }

    pub fn split_counts(&self) -> (usize, usize, usize) {
        (self.train.len(), self.validation.len(), self.test.len())
    }
}
