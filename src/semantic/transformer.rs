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

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use candle_core::{DType, Device, Tensor};

use crate::core::capabilities::{CapabilityLevel, ModelMetadata, ProviderCapabilities};
use crate::core::error::ProviderError;
use crate::core::providers::EmbeddingProvider as EmbeddingProviderTrait;

const PROVIDER: &str = "transformer_embedding";
const CONFIG_FILE: &str = "config.json";
const VOCAB_FILE: &str = "vocab.txt";
const WEIGHTS_FILE: &str = "model.safetensors";
/// Additive attention-mask value for padded positions (underflows to zero).
const MASKED_LOGIT: f64 = -1e9;
const TANH_GELU_COEF: f64 = 0.7978845608; // sqrt(2/pi)
const TANH_GELU_CUBIC: f64 = 0.044715;

fn invalid(message: impl std::fmt::Display) -> ProviderError {
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
struct EncoderConfig {
    hidden_size: usize,
    num_layers: usize,
    num_heads: usize,
    intermediate_size: usize,
    max_positions: usize,
    layer_norm_eps: f64,
    vocab_size: usize,
    model_id: String,
    revision: Option<String>,
    languages: Vec<String>,
    do_lower_case: bool,
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

/// BERT WordPiece tokenizer over a `vocab.txt` vocabulary (one token per
/// line, id = line number). Requires `[PAD]`, `[UNK]`, `[CLS]`, `[SEP]`.
#[derive(Debug, Clone)]
struct WordPieceTokenizer {
    vocab: BTreeMap<String, u32>,
    unk_id: u32,
    cls_id: u32,
    sep_id: u32,
    pad_id: u32,
    lowercase: bool,
}

impl WordPieceTokenizer {
    fn from_vocab_txt(source: &str, lowercase: bool) -> Result<Self, ProviderError> {
        let mut vocab = BTreeMap::new();
        for (index, line) in source.lines().enumerate() {
            let token = line.trim_end_matches(['\n', '\r']);
            if token.is_empty() {
                continue;
            }
            let id = u32::try_from(index)
                .map_err(|_| invalid(format!("{VOCAB_FILE}: vocabulary exceeds u32 ids")))?;
            vocab.insert(token.to_string(), id);
        }
        if vocab.is_empty() {
            return Err(invalid(format!("{VOCAB_FILE}: vocabulary is empty")));
        }
        let lookup = |special: &str| {
            vocab
                .get(special)
                .copied()
                .ok_or_else(|| invalid(format!("{VOCAB_FILE}: missing required token `{special}`")))
        };
        Ok(Self {
            unk_id: lookup("[UNK]")?,
            cls_id: lookup("[CLS]")?,
            sep_id: lookup("[SEP]")?,
            pad_id: lookup("[PAD]")?,
            vocab,
            lowercase,
        })
    }

    /// Basic tokenization: whitespace split, punctuation/CJK isolation,
    /// optional uncased folding with accent stripping.
    fn basic_tokens(&self, text: &str) -> Vec<String> {
        let mut spaced = String::with_capacity(text.len() + 8);
        for ch in text.chars() {
            if ch.is_whitespace() {
                spaced.push(' ');
            } else if is_split_char(ch) {
                spaced.push(' ');
                spaced.push(ch);
                spaced.push(' ');
            } else {
                spaced.push(ch);
            }
        }
        spaced
            .split_whitespace()
            .map(|word| {
                if self.lowercase {
                    strip_accents(&word.to_lowercase())
                } else {
                    word.to_string()
                }
            })
            .collect()
    }

    /// Greedy longest-match WordPiece segmentation of one basic token.
    fn word_pieces(&self, word: &str) -> Vec<u32> {
        if word.len() > 100 {
            return vec![self.unk_id];
        }
        let chars: Vec<char> = word.chars().collect();
        let mut pieces = Vec::new();
        let mut start = 0;
        while start < chars.len() {
            let mut end = chars.len();
            let mut found = None;
            while end > start {
                let candidate: String = chars[start..end].iter().collect();
                let key = if start == 0 {
                    candidate
                } else {
                    format!("##{candidate}")
                };
                if let Some(id) = self.vocab.get(&key) {
                    found = Some(*id);
                    break;
                }
                end -= 1;
            }
            match found {
                Some(id) => {
                    pieces.push(id);
                    start = end;
                }
                None => return vec![self.unk_id],
            }
        }
        pieces
    }

    /// Encode to `[CLS] pieces [SEP]`, truncating word pieces to `max_len - 2`.
    /// Returns `(ids, truncated)`.
    fn encode(&self, text: &str, max_len: usize) -> (Vec<u32>, bool) {
        let capacity = max_len.saturating_sub(2).max(1);
        let mut pieces = Vec::new();
        let mut truncated = false;
        'words: for word in self.basic_tokens(text) {
            for piece in self.word_pieces(&word) {
                if pieces.len() >= capacity {
                    truncated = true;
                    break 'words;
                }
                pieces.push(piece);
            }
        }
        let mut ids = Vec::with_capacity(pieces.len() + 2);
        ids.push(self.cls_id);
        ids.extend(pieces);
        ids.push(self.sep_id);
        (ids, truncated)
    }
}

fn is_split_char(ch: char) -> bool {
    if ch.is_ascii_punctuation() {
        return true;
    }
    // Loose CJK/letter-spacing for Han, Hiragana, Katakana, Hangul blocks.
    matches!(ch,
        '\u{2E80}'..='\u{2EFF}' | '\u{3000}'..='\u{303F}' | '\u{3040}'..='\u{309F}'
        | '\u{30A0}'..='\u{30FF}' | '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}'
        | '\u{AC00}'..='\u{D7AF}' | '\u{F900}'..='\u{FAFF}' | '\u{FF00}'..='\u{FFEF}')
}

fn strip_accents(text: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    text.nfd().filter(|ch| !is_combining_mark(*ch)).collect()
}

fn is_combining_mark(ch: char) -> bool {
    matches!(ch, '\u{300}'..='\u{36F}' | '\u{1AB0}'..='\u{1AFF}' | '\u{1DC0}'..='\u{1DFF}'
        | '\u{20D0}'..='\u{20FF}' | '\u{FE20}'..='\u{FE2F}')
}

fn debug_shapes(tensors: &[&Tensor]) -> Vec<Vec<usize>> {
    tensors
        .iter()
        .map(|tensor| tensor.dims().to_vec())
        .collect()
}

/// One encoder layer's validated tensors (all F32, CPU).
struct LayerWeights {
    query_w: Tensor,
    query_b: Tensor,
    key_w: Tensor,
    key_b: Tensor,
    value_w: Tensor,
    value_b: Tensor,
    attn_out_w: Tensor,
    attn_out_b: Tensor,
    attn_ln_w: Tensor,
    attn_ln_b: Tensor,
    inter_w: Tensor,
    inter_b: Tensor,
    out_w: Tensor,
    out_b: Tensor,
    out_ln_w: Tensor,
    out_ln_b: Tensor,
}

struct BertWeights {
    word_emb: Tensor,
    pos_emb: Tensor,
    token_type_emb: Tensor,
    emb_ln_w: Tensor,
    emb_ln_b: Tensor,
    layers: Vec<LayerWeights>,
}

impl std::fmt::Debug for LayerWeights {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LayerWeights")
            .field(
                "shapes",
                &debug_shapes(&[
                    &self.query_w,
                    &self.key_w,
                    &self.value_w,
                    &self.attn_out_w,
                    &self.inter_w,
                    &self.out_w,
                ]),
            )
            .finish()
    }
}

impl std::fmt::Debug for BertWeights {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BertWeights")
            .field(
                "embeddings",
                &debug_shapes(&[&self.word_emb, &self.pos_emb, &self.token_type_emb]),
            )
            .field("layers", &self.layers.len())
            .finish()
    }
}

fn take_f32(
    tensors: &mut HashMap<String, Tensor>,
    name: &str,
    shape: &[usize],
) -> Result<Tensor, ProviderError> {
    let tensor = tensors.remove(name).ok_or_else(|| {
        invalid(format!(
            "{WEIGHTS_FILE}: missing tensor `{name}` (not a BERT checkpoint?)"
        ))
    })?;
    if tensor.dtype() != DType::F32 {
        return Err(invalid(format!(
            "{WEIGHTS_FILE}: tensor `{name}` has dtype {:?}, only F32 is supported",
            tensor.dtype()
        )));
    }
    if tensor.dims() != shape {
        return Err(invalid(format!(
            "{WEIGHTS_FILE}: tensor `{name}` has shape {:?}, expected {shape:?} \
             (checkpoint does not match {CONFIG_FILE})",
            tensor.dims()
        )));
    }
    Ok(tensor)
}

fn load_weights(
    tensors: &mut HashMap<String, Tensor>,
    config: &EncoderConfig,
) -> Result<BertWeights, ProviderError> {
    let hidden = config.hidden_size;
    let intermediate = config.intermediate_size;
    let vocab = config.vocab_size;
    let positions = config.max_positions;
    let vec = |size: usize| vec![size];
    let mat = |rows: usize, cols: usize| vec![rows, cols];
    let word_emb = take_f32(
        tensors,
        "embeddings.word_embeddings.weight",
        &mat(vocab, hidden),
    )?;
    let pos_emb = take_f32(
        tensors,
        "embeddings.position_embeddings.weight",
        &mat(positions, hidden),
    )?;
    let token_type_emb = take_f32(
        tensors,
        "embeddings.token_type_embeddings.weight",
        &mat(2, hidden),
    )?;
    let emb_ln_w = take_f32(tensors, "embeddings.LayerNorm.weight", &vec(hidden))?;
    let emb_ln_b = take_f32(tensors, "embeddings.LayerNorm.bias", &vec(hidden))?;
    let mut layers = Vec::with_capacity(config.num_layers);
    for layer in 0..config.num_layers {
        let base = format!("encoder.layer.{layer}");
        let attention = format!("{base}.attention.self");
        let mut linear = |name: &str, shape: &[usize]| take_f32(tensors, name, shape);
        layers.push(LayerWeights {
            query_w: linear(&format!("{attention}.query.weight"), &mat(hidden, hidden))?,
            query_b: linear(&format!("{attention}.query.bias"), &vec(hidden))?,
            key_w: linear(&format!("{attention}.key.weight"), &mat(hidden, hidden))?,
            key_b: linear(&format!("{attention}.key.bias"), &vec(hidden))?,
            value_w: linear(&format!("{attention}.value.weight"), &mat(hidden, hidden))?,
            value_b: linear(&format!("{attention}.value.bias"), &vec(hidden))?,
            attn_out_w: linear(
                &format!("{base}.attention.output.dense.weight"),
                &mat(hidden, hidden),
            )?,
            attn_out_b: linear(&format!("{base}.attention.output.dense.bias"), &vec(hidden))?,
            attn_ln_w: linear(
                &format!("{base}.attention.output.LayerNorm.weight"),
                &vec(hidden),
            )?,
            attn_ln_b: linear(
                &format!("{base}.attention.output.LayerNorm.bias"),
                &vec(hidden),
            )?,
            inter_w: linear(
                &format!("{base}.intermediate.dense.weight"),
                &mat(intermediate, hidden),
            )?,
            inter_b: linear(
                &format!("{base}.intermediate.dense.bias"),
                &vec(intermediate),
            )?,
            out_w: linear(
                &format!("{base}.output.dense.weight"),
                &mat(hidden, intermediate),
            )?,
            out_b: linear(&format!("{base}.output.dense.bias"), &vec(hidden))?,
            out_ln_w: linear(&format!("{base}.output.LayerNorm.weight"), &vec(hidden))?,
            out_ln_b: linear(&format!("{base}.output.LayerNorm.bias"), &vec(hidden))?,
        });
    }
    Ok(BertWeights {
        word_emb,
        pos_emb,
        token_type_emb,
        emb_ln_w,
        emb_ln_b,
        layers,
    })
}

/// `x @ w^T + b` with `w: [out, in]`.
fn linear(x: &Tensor, weight: &Tensor, bias: &Tensor) -> Result<Tensor, candle_core::Error> {
    x.matmul(&weight.transpose(0, 1)?)?.broadcast_add(bias)
}

fn layer_norm(
    x: &Tensor,
    weight: &Tensor,
    bias: &Tensor,
    eps: f64,
    dim: usize,
) -> Result<Tensor, candle_core::Error> {
    let mean = x.mean_keepdim(dim)?;
    let centered = x.broadcast_sub(&mean)?;
    let variance = centered.sqr()?.mean_keepdim(dim)?;
    let normalized = centered.broadcast_div(&variance.affine(1.0, eps)?.sqrt()?)?;
    normalized.broadcast_mul(weight)?.broadcast_add(bias)
}

/// GELU with the standard tanh approximation.
fn gelu_tanh(x: &Tensor) -> Result<Tensor, candle_core::Error> {
    let cubed = x.sqr()?.mul(x)?;
    let inner = cubed
        .affine(TANH_GELU_CUBIC, 0.0)?
        .add(x)?
        .affine(TANH_GELU_COEF, 0.0)?;
    inner.tanh()?.affine(1.0, 1.0)?.mul(x)?.affine(0.5, 0.0)
}

fn softmax_last_dim(scores: &Tensor) -> Result<Tensor, candle_core::Error> {
    let last = scores.rank().checked_sub(1).ok_or(candle_core::Error::Msg(
        "softmax needs at least one dimension".to_string(),
    ))?;
    let shifted = scores.broadcast_sub(&scores.max_keepdim(last)?)?;
    let exp = shifted.exp()?;
    exp.broadcast_div(&exp.sum_keepdim(last)?)
}

fn attention(
    hidden: &Tensor,
    mask_add: &Tensor,
    layer: &LayerWeights,
    config: &EncoderConfig,
) -> Result<Tensor, candle_core::Error> {
    let tokens = hidden.dim(0)?;
    let heads = config.num_heads;
    let head_dim = config.hidden_size / heads;
    let split = |projected: Tensor| {
        projected
            .reshape((tokens, heads, head_dim))?
            .transpose(0, 1)?
            .contiguous()
    };
    let query = split(linear(hidden, &layer.query_w, &layer.query_b)?)?;
    let key = split(linear(hidden, &layer.key_w, &layer.key_b)?)?;
    let value = split(linear(hidden, &layer.value_w, &layer.value_b)?)?;
    let scale = 1.0 / (head_dim as f64).sqrt();
    let scores = query
        .matmul(&key.transpose(1, 2)?.contiguous()?)?
        .affine(scale, 0.0)?
        .broadcast_add(mask_add)?;
    let context = softmax_last_dim(&scores)?
        .matmul(&value)?
        .transpose(0, 1)?
        .contiguous()?
        .reshape((tokens, config.hidden_size))?;
    let projected = linear(&context, &layer.attn_out_w, &layer.attn_out_b)?;
    layer_norm(
        &projected.add(hidden)?,
        &layer.attn_ln_w,
        &layer.attn_ln_b,
        config.layer_norm_eps,
        1,
    )
}

fn encoder_layer(
    hidden: &Tensor,
    mask_add: &Tensor,
    layer: &LayerWeights,
    config: &EncoderConfig,
) -> Result<Tensor, candle_core::Error> {
    let attended = attention(hidden, mask_add, layer, config)?;
    let activated = gelu_tanh(&linear(&attended, &layer.inter_w, &layer.inter_b)?)?;
    let projected = linear(&activated, &layer.out_w, &layer.out_b)?;
    layer_norm(
        &projected.add(&attended)?,
        &layer.out_ln_w,
        &layer.out_ln_b,
        config.layer_norm_eps,
        1,
    )
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

    fn tensor(rows: &[Vec<f32>]) -> Tensor {
        let flat: Vec<f32> = rows.iter().flatten().copied().collect();
        Tensor::new(flat, &Device::Cpu)
            .unwrap()
            .reshape((rows.len(), rows[0].len()))
            .unwrap()
    }

    #[test]
    fn softmax_rows_sum_to_one() {
        let scores = tensor(&[vec![1.0, 2.0, 3.0], vec![-1000.0, 0.0, 1000.0]]);
        let probs = softmax_last_dim(&scores).unwrap().to_vec2::<f32>().unwrap();
        for row in &probs {
            let sum: f32 = row.iter().sum();
            assert!((sum - 1.0).abs() < 1e-5, "row sums to {sum}");
            assert!(row.iter().all(|value| (0.0..=1.0).contains(value)));
        }
        assert!(probs[0][2] > probs[0][1] && probs[0][1] > probs[0][0]);
        assert!(probs[1][2] > 0.999);
    }

    #[test]
    fn layer_norm_centers_scales_and_shifts() {
        let values = tensor(&[vec![1.0, 2.0, 3.0, 4.0]]);
        let weight = Tensor::new(vec![2.0f32; 4], &Device::Cpu).unwrap();
        let bias = Tensor::new(vec![0.5f32; 4], &Device::Cpu).unwrap();
        let normed = layer_norm(&values, &weight, &bias, 1e-12, 1)
            .unwrap()
            .to_vec2::<f32>()
            .unwrap();
        let row = &normed[0];
        let mean = row.iter().sum::<f32>() / row.len() as f32;
        assert!((mean - 0.5).abs() < 1e-4, "bias shift, mean={mean}");
        // Unit variance scaled by weight 2: values are +/- fixed multiples.
        assert!((row[0] + 2.1836).abs() < 1e-3, "unexpected {row:?}");
    }

    #[test]
    fn gelu_matches_tanh_reference() {
        let values = Tensor::new(vec![0.0f32, 1.0, -1.0], &Device::Cpu).unwrap();
        let out = gelu_tanh(&values).unwrap().to_vec1::<f32>().unwrap();
        assert!(out[0].abs() < 1e-6);
        assert!((out[1] - 0.8412).abs() < 1e-3, "gelu(1)={}", out[1]);
        assert!((out[2] + 0.1588).abs() < 1e-3, "gelu(-1)={}", out[2]);
    }

    #[test]
    fn wordpiece_handles_unknown_and_truncation() {
        let source = "[PAD]\n[UNK]\n[CLS]\n[SEP]\n[MASK]\nhello\n##s\nworld\n";
        let tokenizer = WordPieceTokenizer::from_vocab_txt(source, false).unwrap();
        let (ids, truncated) = tokenizer.encode("hello worlds", 32);
        assert!(!truncated);
        // "worlds" -> "world" + "##s".
        assert_eq!(ids.len(), 5, "CLS hello world ##s SEP, got {ids:?}");
        let (unk, _) = tokenizer.encode("xyzzy", 32);
        assert!(unk.contains(&tokenizer.unk_id));
        let (_, long) = tokenizer.encode("hello ".repeat(100).as_str(), 8);
        assert!(long);
    }

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
