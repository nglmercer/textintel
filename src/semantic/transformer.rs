//! Local transformer sentence embeddings (BERT family) on the CPU.
//!
//! [`TransformerEmbeddingProvider`] loads explicit local model files —
//! `config.json`, `vocab.txt` (WordPiece), `model.safetensors` — and runs a
//! BERT-compatible encoder with Candle tensors. No network access, no model
//! download: the directory must already exist.
//!
//! Supported checkpoints are BERT-wiring models with a WordPiece vocabulary
//! (`bert-base-multilingual-cased`, `paraphrase-multilingual-MiniLM-L12-v2`,
//! LaBSE-style encoders). Checkpoints using SentencePiece/BPE tokenizers
//! (multilingual-e5, BGE-M3) are rejected with a clear error: their
//! tokenizer is a different format, not a different quality tier.
//!
//! Sentence vectors are CLS-pooled (or mean-pooled) encoder outputs,
//! L2-normalized. The GELU activation uses the standard tanh approximation.
//!
//! A tiny deterministic fixture for mechanics tests can be generated with
//! `tools/generate_mini_transformer.py`; see
//! `tests/fixtures/mini-transformer/README.md`.

use std::path::{Path, PathBuf};

use candle_core::{Device, Tensor};

use crate::core::capabilities::{CapabilityLevel, ModelMetadata, ProviderCapabilities};
use crate::core::error::ProviderError;
use crate::core::providers::EmbeddingProvider as EmbeddingProviderTrait;

mod forward;
mod tokenizer;
mod weights;

use forward::{encoder_layer, layer_norm};
use tokenizer::WordPieceTokenizer;
use weights::{load_weights, BertWeights};
const PROVIDER: &str = "transformer_embedding";
pub(crate) const CONFIG_FILE: &str = "config.json";
pub(crate) const VOCAB_FILE: &str = "vocab.txt";
pub(crate) const WEIGHTS_FILE: &str = "model.safetensors";
/// Additive attention-mask value for padded positions (underflows to zero).
const MASKED_LOGIT: f64 = -1e9;

pub(crate) fn invalid(message: impl std::fmt::Display) -> ProviderError {
    ProviderError::new(PROVIDER, message.to_string())
}

fn candle(error: candle_core::Error) -> ProviderError {
    invalid(format!("tensor error: {error}"))
}

/// Which encoder output becomes the sentence vector.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TransformerPooling {
    /// First (`[CLS]`) row. Default.
    #[default]
    Cls,
    /// Mean over non-padded rows.
    Mean,
}

/// Encoder geometry and provenance parsed from `config.json`.
#[derive(Debug, Clone)]
pub(crate) struct EncoderConfig {
    pub(crate) hidden_size: usize,
    pub(crate) num_layers: usize,
    pub(crate) num_heads: usize,
    pub(crate) intermediate_size: usize,
    pub(crate) max_positions: usize,
    pub(crate) layer_norm_eps: f64,
    pub(crate) vocab_size: usize,
    pub(crate) model_id: String,
    pub(crate) revision: Option<String>,
    pub(crate) languages: Vec<String>,
    pub(crate) do_lower_case: bool,
}

fn required_usize(config: &serde_json::Value, field: &str) -> Result<usize, ProviderError> {
    config
        .get(field)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| invalid(format!("{CONFIG_FILE}: missing or invalid `{field}`")))
}

fn parse_config(source: &str, directory: &Path) -> Result<EncoderConfig, ProviderError> {
    let config: serde_json::Value =
        serde_json::from_str(source).map_err(|error| invalid(format!("{CONFIG_FILE}: {error}")))?;
    let model_type = config
        .get("model_type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("bert");
    if model_type != "bert" {
        return Err(invalid(format!(
            "{CONFIG_FILE}: unsupported model_type `{model_type}`: this provider runs \
             BERT-wiring WordPiece encoders only (multilingual-e5 and BGE-M3 use \
             SentencePiece tokenizers and are not loadable here)"
        )));
    }
    let hidden_act = config
        .get("hidden_act")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("gelu");
    if !matches!(hidden_act, "gelu" | "gelu_new") {
        return Err(invalid(format!(
            "{CONFIG_FILE}: unsupported hidden_act `{hidden_act}` (expected `gelu` or `gelu_new`)"
        )));
    }
    let hidden_size = required_usize(&config, "hidden_size")?;
    let num_layers = required_usize(&config, "num_hidden_layers")?;
    let num_heads = required_usize(&config, "num_attention_heads")?;
    let intermediate_size = required_usize(&config, "intermediate_size")?;
    let max_positions = required_usize(&config, "max_position_embeddings")?;
    let vocab_size = required_usize(&config, "vocab_size")?;
    if hidden_size % num_heads != 0 {
        return Err(invalid(format!(
            "{CONFIG_FILE}: hidden_size {hidden_size} is not divisible by \
             num_attention_heads {num_heads}"
        )));
    }
    let layer_norm_eps = config
        .get("layer_norm_eps")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(1e-12);
    if !layer_norm_eps.is_finite() || layer_norm_eps <= 0.0 {
        return Err(invalid(format!(
            "{CONFIG_FILE}: invalid layer_norm_eps `{layer_norm_eps}`"
        )));
    }
    let fallback_id = directory
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| PROVIDER.to_string());
    Ok(EncoderConfig {
        hidden_size,
        num_layers,
        num_heads,
        intermediate_size,
        max_positions,
        layer_norm_eps,
        vocab_size,
        model_id: config
            .get("model_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(&fallback_id)
            .to_string(),
        revision: config
            .get("revision")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        languages: config
            .get("languages")
            .and_then(serde_json::Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_else(|| vec!["multilingual".to_string()]),
        do_lower_case: config
            .get("do_lower_case")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    })
}

/// One encoded batch with truncation reporting per input.
#[derive(Debug, Clone)]
pub struct EncodedBatch {
    pub vectors: Vec<Vec<f32>>,
    /// True when word pieces were dropped to fit `max_token_length`.
    pub truncated: Vec<bool>,
    /// Word-piece counts actually encoded (including `[CLS]`/`[SEP]`).
    pub token_counts: Vec<usize>,
}

/// Local transformer sentence-embedding provider (BERT family).
///
/// Open an explicit model directory; everything runs offline on the CPU.
/// Use [`Self::encode_detailed`] when truncation must be reported.
#[derive(Debug)]
pub struct TransformerEmbeddingProvider {
    directory: PathBuf,
    config: EncoderConfig,
    tokenizer: WordPieceTokenizer,
    weights: BertWeights,
    pooling: TransformerPooling,
    max_batch: usize,
    model_id: String,
    revision: Option<String>,
    languages: Vec<String>,
}

impl TransformerEmbeddingProvider {
    /// Load `config.json`, `vocab.txt`, and `model.safetensors` from
    /// `directory`. Shapes and dtypes are validated eagerly: incompatible
    /// checkpoints fail here, never with a silent wrong-shaped inference.
    pub fn open(directory: impl AsRef<Path>) -> Result<Self, ProviderError> {
        let directory = directory.as_ref().to_path_buf();
        let read = |file: &str| {
            std::fs::read_to_string(directory.join(file)).map_err(|error| {
                invalid(format!(
                    "cannot read {}: {error} (expected a local model directory, no download)",
                    directory.join(file).display()
                ))
            })
        };
        let config = parse_config(&read(CONFIG_FILE)?, &directory)?;
        let tokenizer =
            WordPieceTokenizer::from_vocab_txt(&read(VOCAB_FILE)?, config.do_lower_case)?;
        if tokenizer.vocab.len() != config.vocab_size {
            return Err(invalid(format!(
                "{VOCAB_FILE}: {} entries but {CONFIG_FILE} declares vocab_size {}; \
                 the tokenizer does not match this checkpoint",
                tokenizer.vocab.len(),
                config.vocab_size
            )));
        }
        let mut tensors =
            candle_core::safetensors::load(directory.join(WEIGHTS_FILE), &Device::Cpu).map_err(
                |error| {
                    invalid(format!(
                        "cannot load {WEIGHTS_FILE}: {error} (F32 safetensors expected)"
                    ))
                },
            )?;
        let weights = load_weights(&mut tensors, &config)?;
        Ok(Self {
            model_id: config.model_id.clone(),
            revision: config.revision.clone(),
            languages: config.languages.clone(),
            directory,
            config,
            tokenizer,
            weights,
            pooling: TransformerPooling::default(),
            max_batch: 32,
        })
    }

    pub fn with_pooling(mut self, pooling: TransformerPooling) -> Self {
        self.pooling = pooling;
        self
    }

    pub fn with_max_batch(mut self, max_batch: usize) -> Self {
        self.max_batch = max_batch.max(1);
        self
    }

    pub fn with_model_id(mut self, model_id: impl Into<String>) -> Self {
        self.model_id = model_id.into();
        self
    }

    pub fn with_revision(mut self, revision: impl Into<String>) -> Self {
        self.revision = Some(revision.into());
        self
    }

    pub fn with_languages(mut self, languages: Vec<String>) -> Self {
        self.languages = languages;
        self
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn dimensions(&self) -> usize {
        self.config.hidden_size
    }

    /// Absolute token cap including `[CLS]`/`[SEP]`.
    pub fn max_token_length(&self) -> usize {
        self.config.max_positions
    }

    pub fn vocabulary_size(&self) -> usize {
        self.tokenizer.vocab.len()
    }

    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    fn forward(&self, ids: &[u32], mask: &[f32], tokens: usize) -> Result<Vec<f32>, ProviderError> {
        let device = Device::Cpu;
        let hidden = self.config.hidden_size;
        let id_tensor = Tensor::new(ids.to_vec(), &device).map_err(candle)?;
        let word = self
            .weights
            .word_emb
            .index_select(&id_tensor, 0)
            .map_err(candle)?;
        let positions: Vec<u32> = (0..tokens as u32).collect();
        let pos = self
            .weights
            .pos_emb
            .index_select(&Tensor::new(positions, &device).map_err(candle)?, 0)
            .map_err(candle)?;
        let zeros = vec![0u32; tokens];
        let token_type = self
            .weights
            .token_type_emb
            .index_select(&Tensor::new(zeros, &device).map_err(candle)?, 0)
            .map_err(candle)?;
        let embedded = word
            .add(&pos)
            .map_err(candle)?
            .add(&token_type)
            .map_err(candle)?;
        let mut hidden_state = layer_norm(
            &embedded,
            &self.weights.emb_ln_w,
            &self.weights.emb_ln_b,
            self.config.layer_norm_eps,
            1,
        )
        .map_err(candle)?;
        let additive: Vec<f32> = mask
            .iter()
            .map(|value| (1.0 - value) * MASKED_LOGIT as f32)
            .collect();
        let mask_add = Tensor::new(additive, &device)
            .map_err(candle)?
            .reshape((1, 1, tokens))
            .map_err(candle)?;
        for layer in &self.weights.layers {
            hidden_state =
                encoder_layer(&hidden_state, &mask_add, layer, &self.config).map_err(candle)?;
        }
        let pooled = match self.pooling {
            TransformerPooling::Cls => hidden_state
                .narrow(0, 0, 1)
                .map_err(candle)?
                .reshape((hidden,))
                .map_err(candle)?,
            TransformerPooling::Mean => {
                let kept: f32 = mask.iter().sum();
                let summed = hidden_state.sum_keepdim(0).map_err(candle)?;
                summed
                    .broadcast_div(&Tensor::new(kept.max(1.0), &device).map_err(candle)?)
                    .map_err(candle)?
                    .reshape((hidden,))
                    .map_err(candle)?
            }
        };
        let norm = pooled
            .sqr()
            .map_err(candle)?
            .sum_all()
            .map_err(candle)?
            .to_scalar::<f32>()
            .map_err(candle)?
            .sqrt();
        let normalized = if norm > 0.0 {
            pooled
                .broadcast_div(&Tensor::new(norm, &device).map_err(candle)?)
                .map_err(candle)?
        } else {
            pooled
        };
        let vector = normalized.to_vec1::<f32>().map_err(candle)?;
        if vector.len() != hidden || vector.iter().any(|value| !value.is_finite()) {
            return Err(invalid("encoder produced an invalid vector"));
        }
        Ok(vector)
    }

    /// Encode with per-input truncation reporting. Inputs are chunked to
    /// `max_batch` texts; every chunk pads to its own longest sequence.
    pub fn encode_detailed(&self, texts: &[String]) -> Result<EncodedBatch, ProviderError> {
        let mut vectors = Vec::with_capacity(texts.len());
        let mut truncated = Vec::with_capacity(texts.len());
        let mut token_counts = Vec::with_capacity(texts.len());
        for chunk in texts.chunks(self.max_batch) {
            let encoded: Vec<(Vec<u32>, bool)> = chunk
                .iter()
                .map(|text| self.tokenizer.encode(text, self.config.max_positions))
                .collect();
            let width = encoded.iter().map(|(ids, _)| ids.len()).max().unwrap_or(0);
            for (ids, was_truncated) in &encoded {
                let mut padded = ids.clone();
                let mut mask = vec![1.0f32; ids.len()];
                padded.resize(width, self.tokenizer.pad_id);
                mask.resize(width, 0.0);
                vectors.push(self.forward(&padded, &mask, width)?);
                truncated.push(*was_truncated);
                token_counts.push(ids.len());
            }
        }
        Ok(EncodedBatch {
            vectors,
            truncated,
            token_counts,
        })
    }
}

impl EmbeddingProviderTrait for TransformerEmbeddingProvider {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        Ok(self.encode_detailed(texts)?.vectors)
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        self.embed(texts)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        let mut capabilities = ProviderCapabilities::new(PROVIDER)
            .with_dimensions(self.config.hidden_size)
            .with_languages(self.languages.clone())
            .with_quality(CapabilityLevel::Production);
        if let Some(revision) = &self.revision {
            capabilities = capabilities.with_model_revision(revision.clone());
        }
        capabilities
    }

    fn model_metadata(&self) -> Option<ModelMetadata> {
        Some(ModelMetadata {
            model_id: self.model_id.clone(),
            revision: self.revision.clone(),
            dimensions: self.config.hidden_size,
            normalized: true,
            languages: self.languages.clone(),
            source: Some(self.directory.display().to_string()),
            license: None,
        })
    }

    fn health_check(&self) -> Result<(), ProviderError> {
        let batch = self.encode_detailed(&["health".to_string()])?;
        let vector = batch
            .vectors
            .first()
            .ok_or_else(|| invalid("health check produced no vector"))?;
        if vector.len() != self.config.hidden_size {
            return Err(invalid("health check dimension mismatch"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bad_configs_fail_loudly() {
        assert!(parse_config(r#"{"hidden_size": 0}"#, Path::new("m")).is_err());
        assert!(parse_config(
            r#"{"model_type": "gpt2", "hidden_size": 8, "num_hidden_layers": 1,
                "num_attention_heads": 2, "intermediate_size": 8,
                "max_position_embeddings": 8, "vocab_size": 8}"#,
            Path::new("m")
        )
        .is_err());
        assert!(parse_config(
            r#"{"hidden_size": 8, "num_hidden_layers": 1, "num_attention_heads": 3,
                "intermediate_size": 8, "max_position_embeddings": 8, "vocab_size": 8}"#,
            Path::new("m")
        )
        .is_err());
        assert!(WordPieceTokenizer::from_vocab_txt("[PAD]\n[UNK]\n", false).is_err());
    }
}
