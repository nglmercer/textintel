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

/// Weights for rebus candidate scoring. Formerly hardcoded in
/// `rebus::scorer`; now configurable so deployments can tune the blend and
/// trained weights can be loaded later without code changes. Channel weights
/// are renormalized at scoring time, so only relative magnitudes matter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct RebusWeights {
    /// Weight of lexicon plausibility (word known / splittable).
    pub lexical: f64,
    /// Divisor of the `ln(1 + frequency)` bonus (larger = weaker bonus).
    pub frequency_scale: f64,
    /// Cap of the frequency bonus added to the lexical channel.
    pub frequency_cap: f64,
    /// Weight of G2P phonetic similarity between surface and source.
    pub phonetic: f64,
    /// Weight of the beam prior (symbol/reading probabilities).
    pub symbol: f64,
    /// Weight of candidate/requested language agreement.
    pub language: f64,
    /// Weight of boundary/context plausibility.
    pub context: f64,
    /// Blend of whole-text semantic similarity into the total
    /// (`0.0` disables semantic rescoring).
    pub semantic: f64,
    /// Cap of the cumulative transformation penalty (`[0.0, 1.0)`).
    pub transformation_penalty_cap: f64,
    /// Per-kind derivation costs (see `RebusEvidence::transformation_penalty`).
    pub cost_identity: f64,
    pub cost_boundary: f64,
    pub cost_leet: f64,
    pub cost_symbol: f64,
    pub cost_other: f64,
    /// Multiplicative discount per language switch along a beam path.
    /// Small by design: mixed-language inputs stay valid, monolingual
    /// derivations are just slightly preferred.
    pub language_switch_penalty: f64,
    /// Discount for multi-letter number-name readings (`4` → `cuatro`) when
    /// the digit sits inside a word (letter neighbors). In-word digits are
    /// overwhelmingly leet (`4` → `a`); standalone digits keep full names.
    /// Single-letter readings are exempt. Must be in `(0.0, 1.0]`.
    pub in_word_digit_discount: f64,
}

impl Default for RebusWeights {
    fn default() -> Self {
        Self {
            lexical: 0.36,
            frequency_scale: 5.0,
            frequency_cap: 0.25,
            phonetic: 0.22,
            symbol: 0.18,
            language: 0.12,
            context: 0.12,
            semantic: 0.15,
            transformation_penalty_cap: 0.35,
            cost_identity: 0.0,
            cost_boundary: 0.03,
            cost_leet: 0.05,
            cost_symbol: 0.10,
            cost_other: 0.08,
            language_switch_penalty: 0.02,
            in_word_digit_discount: 0.5,
        }
    }
}

impl RebusWeights {
    /// Load weights from JSON (for tuned or trained vectors). Unknown fields
    /// are ignored by serde; validation still applies.
    pub fn from_json(source: &str) -> Result<Self, String> {
        let weights: Self = serde_json::from_str(source)
            .map_err(|error| format!("invalid rebus weights: {error}"))?;
        weights.validate()?;
        Ok(weights)
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|error| error.to_string())
    }

    pub fn as_map(&self) -> std::collections::BTreeMap<String, f64> {
        [
            ("lexical", self.lexical),
            ("frequency_scale", self.frequency_scale),
            ("frequency_cap", self.frequency_cap),
            ("phonetic", self.phonetic),
            ("symbol", self.symbol),
            ("language", self.language),
            ("context", self.context),
            ("semantic", self.semantic),
            (
                "transformation_penalty_cap",
                self.transformation_penalty_cap,
            ),
            ("cost_identity", self.cost_identity),
            ("cost_boundary", self.cost_boundary),
            ("cost_leet", self.cost_leet),
            ("cost_symbol", self.cost_symbol),
            ("cost_other", self.cost_other),
            ("language_switch_penalty", self.language_switch_penalty),
            ("in_word_digit_discount", self.in_word_digit_discount),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
    }

    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in self.as_map() {
            if !value.is_finite() || value < 0.0 {
                return Err(format!(
                    "rebus weight {name} must be finite and non-negative"
                ));
            }
        }
        if self.frequency_scale <= 0.0 {
            return Err("rebus frequency_scale must be positive".to_string());
        }
        if self.transformation_penalty_cap >= 1.0 {
            return Err("rebus transformation_penalty_cap must be below 1.0".to_string());
        }
        if self.semantic > 1.0 {
            return Err("rebus semantic blend must be within [0.0, 1.0]".to_string());
        }
        if self.language_switch_penalty >= 1.0 {
            return Err("rebus language_switch_penalty must be below 1.0".to_string());
        }
        if !self.in_word_digit_discount.is_finite()
            || self.in_word_digit_discount <= 0.0
            || self.in_word_digit_discount > 1.0
        {
            return Err("rebus in_word_digit_discount must be in (0.0, 1.0]".to_string());
        }
        if self.lexical + self.phonetic + self.symbol + self.language + self.context <= 0.0 {
            return Err("at least one rebus channel weight must be positive".to_string());
        }
        Ok(())
    }

    /// Sum of the five blended channel weights (used for renormalization).
    pub fn channel_sum(&self) -> f64 {
        (self.lexical + self.phonetic + self.symbol + self.language + self.context)
            .max(f64::MIN_POSITIVE)
    }
}

/// Bounded revision-aware cache limits (entries per cache). `0` disables
/// that cache. Caches are transparent: hits return clones of what the
/// wrapped provider would compute, keys always include the provider identity
/// and model/resource revision, and an observed revision change invalidates
/// instead of serving stale values.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct CacheLimits {
    pub embeddings: usize,
    pub g2p: usize,
    pub language: usize,
    pub rebus: usize,
}

impl CacheLimits {
    /// Production preset: generous text-keyed caches for the embedding, G2P,
    /// and language providers plus a smaller rebus cache (decoded candidate
    /// lists are the largest values).
    pub fn production() -> Self {
        Self {
            embeddings: 1024,
            g2p: 1024,
            language: 1024,
            rebus: 256,
        }
    }

    /// Fill disabled (`0`) entries with the production defaults, keeping any
    /// explicitly configured limits.
    pub fn with_production_defaults(mut self) -> Self {
        let production = Self::production();
        if self.embeddings == 0 {
            self.embeddings = production.embeddings;
        }
        if self.g2p == 0 {
            self.g2p = production.g2p;
        }
        if self.language == 0 {
            self.language = production.language;
        }
        if self.rebus == 0 {
            self.rebus = production.rebus;
        }
        self
    }

    pub fn any_enabled(&self) -> bool {
        self.embeddings > 0 || self.g2p > 0 || self.language > 0 || self.rebus > 0
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
    /// Rebus scoring blend (see [`RebusWeights`]).
    pub rebus_weights: RebusWeights,
    /// Preferred languages (BCP-47) for decoding and phonetics. Empty means
    /// "use detected languages". Hints never change detection itself, only
    /// which readings the decoder and G2P prefer.
    pub language_hints: Vec<String>,
    /// Bounded revision-aware caches. Disabled by default; the production
    /// preset enables them (see [`CacheLimits::production`]).
    pub cache: CacheLimits,
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
            // Semantic scoring stays opt-in: it only takes effect with an
            // injected embedding provider (e.g. the local feature-hash
            // baseline or a model). The default provider is a null backend.
            semantic: false,
            phonetic: false,
            rebus_weights: RebusWeights::default(),
            language_hints: Vec::new(),
            cache: CacheLimits::default(),
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
        self.similarity_weights.validate()?;
        self.rebus_weights.validate()?;
        if self
            .language_hints
            .iter()
            .any(|hint| hint.trim().is_empty())
        {
            return Err("language_hints must not contain empty codes".to_string());
        }
        Ok(())
    }
}
