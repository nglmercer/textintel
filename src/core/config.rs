use serde::{Deserialize, Serialize};

/// Weights used by the final score.  Missing channels are omitted and the
/// remaining weights are renormalized, so disabling a provider is safe.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SimilarityWeights {
    pub semantic: f64,
    pub lexical: f64,
    pub character: f64,
    pub visual: f64,
    pub phonetic: f64,
    pub symbolic: f64,
    pub decoded: f64,
    pub obfuscation: f64,
}

impl Default for SimilarityWeights {
    fn default() -> Self {
        // Provider channels are weighted by default, but missing channels are
        // skipped and the remaining weights are renormalized. This keeps the
        // base engine model-free while making an injected semantic/G2P
        // provider effective without a second score configuration.
        Self {
            semantic: 0.20,
            lexical: 0.10,
            character: 0.10,
            visual: 0.10,
            phonetic: 0.15,
            symbolic: 0.10,
            decoded: 0.20,
            obfuscation: 0.05,
        }
    }
}

impl SimilarityWeights {
    pub fn as_map(&self) -> std::collections::BTreeMap<String, f64> {
        [
            ("semantic", self.semantic),
            ("lexical", self.lexical),
            ("character", self.character),
            ("visual", self.visual),
            ("phonetic", self.phonetic),
            ("symbolic", self.symbolic),
            ("decoded", self.decoded),
            ("obfuscation", self.obfuscation),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
    }

    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("semantic", self.semantic),
            ("lexical", self.lexical),
            ("character", self.character),
            ("visual", self.visual),
            ("phonetic", self.phonetic),
            ("symbolic", self.symbolic),
            ("decoded", self.decoded),
            ("obfuscation", self.obfuscation),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(format!("{name} weight must be finite and non-negative"));
            }
        }
        if self.as_map().values().all(|value| *value == 0.0) {
            return Err("at least one similarity weight must be positive".to_string());
        }
        Ok(())
    }
}

/// Resource and provider limits.  These bounds protect candidate generation
/// from untrusted input and make runtime behavior predictable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct EngineConfig {
    pub max_input_length: usize,
    pub max_segments: usize,
    pub beam_width: usize,
    pub max_candidates: usize,
    pub max_symbol_readings: usize,
    pub max_recursion: usize,
    pub max_documents: usize,
    pub repetition_keep: usize,
    pub max_batch_size: usize,
    pub max_search_candidates: usize,
    pub max_decoded_branches: usize,
    pub strong_confidence_gap: f64,
    pub similarity_weights: SimilarityWeights,
    pub semantic: bool,
    pub phonetic: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_input_length: 8_192,
            max_segments: 256,
            beam_width: 20,
            max_candidates: 10,
            max_symbol_readings: 8,
            max_recursion: 8,
            max_documents: 100_000,
            repetition_keep: 1,
            max_batch_size: 256,
            max_search_candidates: 500,
            max_decoded_branches: 20_000,
            strong_confidence_gap: 0.15,
            similarity_weights: SimilarityWeights::default(),
            semantic: false,
            phonetic: false,
        }
    }
}

impl EngineConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.max_input_length == 0 {
            return Err("max_input_length must be positive".to_string());
        }
        for (name, value) in [
            ("max_segments", self.max_segments),
            ("beam_width", self.beam_width),
            ("max_candidates", self.max_candidates),
            ("max_symbol_readings", self.max_symbol_readings),
            ("max_recursion", self.max_recursion),
            ("max_documents", self.max_documents),
            ("repetition_keep", self.repetition_keep),
            ("max_batch_size", self.max_batch_size),
            ("max_search_candidates", self.max_search_candidates),
            ("max_decoded_branches", self.max_decoded_branches),
        ] {
            if value == 0 {
                return Err(format!("{name} must be positive"));
            }
        }
        if !self.strong_confidence_gap.is_finite()
            || !(0.0..=1.0).contains(&self.strong_confidence_gap)
        {
            return Err("strong_confidence_gap must be between 0 and 1".to_string());
        }
        self.similarity_weights.validate()
    }
}
