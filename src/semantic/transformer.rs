//! Local transformer sentence embeddings (modern BERT-wiring family) on CPU.
//!
//! [`TransformerEmbeddingProvider`] loads explicit local model files and runs
//! a BERT-compatible encoder with Candle tensors. No network access, no model
//! download: the directory must already exist.
//!
//! Curated modern checkpoints (see [`crate::semantic::catalog`]):
//!
//! - `intfloat/multilingual-e5-small` (118M, 384 dims): multilingual default.
//!   Mean pooling with `query: ` / `passage: ` prefixes per the model card.
//! - `Snowflake/snowflake-arctic-embed-xs` (22M, 384 dims): English CPU pick.
//!   CLS pooling, retrieval queries carry the card's `Represent this ...` prefix.
//! - `mixedbread-ai/mxbai-embed-xsmall-v1` (24M, 384 dims): English CPU pick.
//!   Mean pooling, no prefix.
//!
//! Two tokenizer layouts load: `tokenizer.json` (SentencePiece-Unigram,
//! e5-style, preferred when present) and `vocab.txt` (WordPiece, BERT-style).
//! `model.safetensors` accepts Hugging Face `bert.`-prefixed tensor names as
//! well as bare names, and F16/BF16 weights are cast to F32 once at load.
//! Encoder wiring must be BERT (`model_type: "bert"`); other wirings
//! (XLM-RoBERTa layers, T5, decoder-only models) are rejected with a clear
//! error.
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
use tokenizer::{UnigramTokenizer, WordPieceTokenizer};
use weights::{BertWeights, load_weights};
const PROVIDER: &str = "transformer_embedding";
pub(crate) const CONFIG_FILE: &str = "config.json";
pub(crate) const VOCAB_FILE: &str = "vocab.txt";
pub(crate) const TOKENIZER_FILE: &str = "tokenizer.json";
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
             BERT-wiring encoders only — multilingual-e5-small, \
             snowflake-arctic-embed-xs, and mxbai-embed-xsmall-v1 all qualify \
             (their configs declare model_type `bert`)"
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
    /// True when pieces were dropped to fit `max_token_length`.
    pub truncated: Vec<bool>,
    /// Piece counts actually encoded (including the wrapping pair).
    pub token_counts: Vec<usize>,
}

/// Tokenizer backing an encoder: Unigram (`tokenizer.json`, e5-style) or
/// WordPiece (`vocab.txt`, BERT-style). Both encode to a wrapped id sequence
/// with truncation reporting; only the vocabulary format differs.
#[derive(Debug, Clone)]
enum EncoderTokenizer {
    Unigram(UnigramTokenizer),
    WordPiece(WordPieceTokenizer),
}

impl EncoderTokenizer {
    fn encode(&self, text: &str, max_len: usize) -> (Vec<u32>, bool) {
        match self {
            Self::Unigram(tokenizer) => tokenizer.encode(text, max_len),
            Self::WordPiece(tokenizer) => tokenizer.encode(text, max_len),
        }
    }

    fn vocab_len(&self) -> usize {
        match self {
            Self::Unigram(tokenizer) => tokenizer.vocab_len(),
            Self::WordPiece(tokenizer) => tokenizer.vocab.len(),
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Unigram(_) => "unigram (tokenizer.json)",
            Self::WordPiece(_) => "wordpiece (vocab.txt)",
        }
    }
}

/// Local transformer sentence-embedding provider (modern BERT-wiring family).
///
/// Open an explicit model directory; everything runs offline on the CPU.
/// Use [`Self::encode_detailed`] when truncation must be reported, and match
/// pooling plus text prefix to the checkpoint's model card (see the module
/// documentation): e5-small needs mean pooling with `query: ` / `passage: `
/// prefixes, Arctic-XS needs CLS pooling with its query prefix, MXBAI-XSmall
/// needs mean pooling with no prefix.
#[derive(Debug)]
pub struct TransformerEmbeddingProvider {
    directory: PathBuf,
    config: EncoderConfig,
    tokenizer: EncoderTokenizer,
    weights: BertWeights,
    pooling: TransformerPooling,
    max_batch: usize,
    max_parallel: usize,
    text_prefix: Option<String>,
    model_id: String,
    revision: Option<String>,
    languages: Vec<String>,
}

impl TransformerEmbeddingProvider {
    /// Load `config.json`, a tokenizer (`tokenizer.json` when present,
    /// otherwise `vocab.txt`), and `model.safetensors` from `directory`.
    /// Shapes and dtypes are validated eagerly: incompatible checkpoints
    /// fail here, never with a silent wrong-shaped inference.
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
        let has_unigram = directory.join(TOKENIZER_FILE).is_file();
        let has_wordpiece = directory.join(VOCAB_FILE).is_file();
        if !has_unigram && !has_wordpiece {
            return Err(invalid(format!(
                "no tokenizer found in {}: expected {TOKENIZER_FILE} \
                 (Unigram, e5-style) or {VOCAB_FILE} (WordPiece, BERT-style); \
                 copy the checkpoint's tokenizer file next to {CONFIG_FILE}",
                directory.display()
            )));
        }
        let tokenizer = if has_unigram {
            EncoderTokenizer::Unigram(UnigramTokenizer::from_tokenizer_json(&read(
                TOKENIZER_FILE,
            )?)?)
        } else {
            EncoderTokenizer::WordPiece(WordPieceTokenizer::from_vocab_txt(
                &read(VOCAB_FILE)?,
                config.do_lower_case,
            )?)
        };
        // The tokenizer may cover fewer rows than `vocab_size` (real
        // multilingual-e5-small ships 250002 pieces for 250037 embedding
        // rows; the extra rows stay unused, as in Hugging Face
        // transformers). A larger tokenizer would index out of bounds and
        // is rejected: it does not belong to this checkpoint.
        if tokenizer.vocab_len() > config.vocab_size {
            return Err(invalid(format!(
                "tokenizer ({}): {} entries but {CONFIG_FILE} declares vocab_size {}; \
                 the tokenizer does not match this checkpoint",
                tokenizer.kind(),
                tokenizer.vocab_len(),
                config.vocab_size
            )));
        }
        let mut tensors =
            candle_core::safetensors::load(directory.join(WEIGHTS_FILE), &Device::Cpu).map_err(
                |error| {
                    invalid(format!(
                        "cannot load {WEIGHTS_FILE}: {error} (F32/F16/BF16 safetensors expected)"
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
            max_parallel: 4,
            text_prefix: None,
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

    /// Worker threads for multi-text batches (default 4). Each text runs
    /// its own exact-length forward on a worker and results rejoin in
    /// input order, so vectors are identical to sequential encoding;
    /// single-text calls never spawn threads. Set `1` for strictly
    /// sequential encoding (e.g. servers that already parallelize across
    /// requests and want no fan-out per call).
    pub fn with_max_parallel(mut self, max_parallel: usize) -> Self {
        self.max_parallel = max_parallel.max(1);
        self
    }

    /// Prepend `prefix` to every input before tokenization (e.g. `query: `
    /// or `passage: ` for multilingual-e5, the `Represent this ...` prefix
    /// for Arctic retrieval queries). Empty prefixes are ignored.
    pub fn with_text_prefix(mut self, prefix: impl Into<String>) -> Self {
        let prefix = prefix.into();
        self.text_prefix = if prefix.is_empty() {
            None
        } else {
            Some(prefix)
        };
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

    /// Absolute token cap including the wrapping pair.
    pub fn max_token_length(&self) -> usize {
        self.config.max_positions
    }

    pub fn vocabulary_size(&self) -> usize {
        self.tokenizer.vocab_len()
    }

    /// Tokenizer layout backing this provider (`unigram` or `wordpiece`).
    pub fn tokenizer_kind(&self) -> &'static str {
        match self.tokenizer {
            EncoderTokenizer::Unigram(_) => "unigram",
            EncoderTokenizer::WordPiece(_) => "wordpiece",
        }
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
        // Range gathers are views, not copies: rows 0..tokens in order
        // (positions) and row 0 broadcast (token type 0) hold exactly the
        // gathered values, with no index tensors or copies.
        let pos = self.weights.pos_emb.narrow(0, 0, tokens).map_err(candle)?;
        let token_type = self
            .weights
            .token_type_emb
            .narrow(0, 0, 1)
            .map_err(candle)?
            .broadcast_as((tokens, hidden))
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
        // Unpadded forwards (the only kind `encode_one` runs) would add
        // exact zeros before softmax: skipping the no-op leaves outputs
        // identical (`exp` maps both signed zeros to `1.0`, erasing the
        // only possible -0.0/+0.0 divergence). Padded callers keep the
        // additive mask.
        let mask_add = if mask.iter().all(|value| *value == 1.0) {
            None
        } else {
            let additive: Vec<f32> = mask
                .iter()
                .map(|value| (1.0 - value) * MASKED_LOGIT as f32)
                .collect();
            Some(
                Tensor::new(additive, &device)
                    .map_err(candle)?
                    .reshape((1, 1, tokens))
                    .map_err(candle)?,
            )
        };
        for layer in &self.weights.layers {
            hidden_state = encoder_layer(&hidden_state, mask_add.as_ref(), layer, &self.config)
                .map_err(candle)?;
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

    /// Tokenize plus forward for one text: its own exact width (never
    /// padded), so its vector never depends on batch neighbors.
    /// Tokenizers always emit at least the two boundary specials, so
    /// widths are nonzero.
    fn encode_one(&self, text: &str) -> Result<(Vec<f32>, bool, usize), ProviderError> {
        let prefixed;
        let input = match &self.text_prefix {
            Some(prefix) => {
                prefixed = format!("{prefix}{text}");
                prefixed.as_str()
            }
            None => text,
        };
        let (ids, was_truncated) = self.tokenizer.encode(input, self.config.max_positions);
        let mask = vec![1.0f32; ids.len()];
        let vector = self.forward(&ids, &mask, ids.len())?;
        Ok((vector, was_truncated, ids.len()))
    }

    /// One chunk sequentially (single text, or parallelism disabled).
    fn encode_chunk_sequential(
        &self,
        chunk: &[String],
        vectors: &mut Vec<Vec<f32>>,
        truncated: &mut Vec<bool>,
        token_counts: &mut Vec<usize>,
    ) -> Result<(), ProviderError> {
        for text in chunk {
            let (vector, was_truncated, count) = self.encode_one(text)?;
            vectors.push(vector);
            truncated.push(was_truncated);
            token_counts.push(count);
        }
        Ok(())
    }

    /// One chunk across worker threads. Each text is an independent
    /// forward, so workers share `&self` and rejoin in input order with
    /// the first-in-order error — byte-identical to sequential encoding.
    fn encode_chunk_parallel(
        &self,
        chunk: &[String],
        vectors: &mut Vec<Vec<f32>>,
        truncated: &mut Vec<bool>,
        token_counts: &mut Vec<usize>,
    ) -> Result<(), ProviderError> {
        let degree = self.max_parallel.min(chunk.len()).max(1);
        if degree < 2 {
            return self.encode_chunk_sequential(chunk, vectors, truncated, token_counts);
        }
        let groups: Vec<&[String]> = chunk.chunks(chunk.len().div_ceil(degree)).collect();
        let mut joined = Vec::with_capacity(groups.len());
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(groups.len());
            for group in groups {
                handles.push(scope.spawn(move || {
                    let mut pieces = Vec::with_capacity(group.len());
                    for text in group {
                        pieces.push(self.encode_one(text)?);
                    }
                    Ok::<Vec<(Vec<f32>, bool, usize)>, ProviderError>(pieces)
                }));
            }
            for handle in handles {
                joined.push(handle.join());
            }
        });
        for result in joined {
            match result {
                Ok(Ok(pieces)) => {
                    for (vector, was_truncated, count) in pieces {
                        vectors.push(vector);
                        truncated.push(was_truncated);
                        token_counts.push(count);
                    }
                }
                Ok(Err(error)) => return Err(error),
                Err(_) => {
                    return Err(invalid("worker thread panicked during parallel encode"));
                }
            }
        }
        Ok(())
    }

    /// Encode with per-input truncation reporting. Inputs are chunked to
    /// `max_batch` texts; each text runs at its own width (never padded),
    /// so vectors never depend on batch neighbors. Multi-text chunks run
    /// on up to [`Self::with_max_parallel`] worker threads (identical
    /// vectors, input order); the configured
    /// [`Self::with_text_prefix`] is prepended before tokenizing.
    pub fn encode_detailed(&self, texts: &[String]) -> Result<EncodedBatch, ProviderError> {
        let mut vectors = Vec::with_capacity(texts.len());
        let mut truncated = Vec::with_capacity(texts.len());
        let mut token_counts = Vec::with_capacity(texts.len());
        for chunk in texts.chunks(self.max_batch) {
            // Each text runs at its own width: padding to the chunk
            // longest only burns attention/FFN compute on pad rows that
            // pooling ignores (CLS) or must exclude (mean).
            self.encode_chunk_parallel(chunk, &mut vectors, &mut truncated, &mut token_counts)?;
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
        assert!(
            parse_config(
                r#"{"model_type": "gpt2", "hidden_size": 8, "num_hidden_layers": 1,
                "num_attention_heads": 2, "intermediate_size": 8,
                "max_position_embeddings": 8, "vocab_size": 8}"#,
                Path::new("m")
            )
            .is_err()
        );
        assert!(
            parse_config(
                r#"{"hidden_size": 8, "num_hidden_layers": 1, "num_attention_heads": 3,
                "intermediate_size": 8, "max_position_embeddings": 8, "vocab_size": 8}"#,
                Path::new("m")
            )
            .is_err()
        );
        assert!(WordPieceTokenizer::from_vocab_txt("[PAD]\n[UNK]\n", false).is_err());
    }
}
