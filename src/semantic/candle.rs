//! Local static-embedding backend powered by the Candle tensor runtime.
//!
//! The provider loads a model directory containing:
//!
//! ```text
//! model/
//!   tokenizer.json         WordLevel vocabulary (`tokenizers` format subset)
//!   embeddings.safetensors float32 tensor named `weight` with shape [vocab, dim]
//! ```
//!
//! Sentence vectors are mean-pooled token rows, L2-normalized on the CPU.
//! Multilingual support comes from the vocabulary: any Unicode token present
//! in `tokenizer.json` is embedded, everything else maps to `unk_token`.
//! No network access, no model download: the directory must already exist.
//!
//! A fastText `.vec` file can be converted with e.g.
//! `python -c` emitting one safetensors `weight` tensor plus a matching
//! WordLevel `tokenizer.json`; see the module documentation example in
//! `examples/semantic.rs` once the `semantic-candle` feature is enabled.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use candle_core::{DType, Device, Tensor};

use crate::core::capabilities::{CapabilityLevel, ModelMetadata, ProviderCapabilities};
use crate::core::error::ProviderError;
use crate::core::providers::EmbeddingProvider as EmbeddingProviderTrait;
use crate::normalization::unicode::casefold_text;

const PROVIDER: &str = "candle_embedding";
const TOKENIZER_FILE: &str = "tokenizer.json";
const WEIGHTS_FILE: &str = "embeddings.safetensors";
const WEIGHT_TENSOR: &str = "weight";

/// Minimal WordLevel tokenizer: an explicit vocabulary plus an unknown token.
#[derive(Debug, Clone)]
struct WordLevel {
    vocab: BTreeMap<String, u32>,
    unk_id: u32,
}

impl WordLevel {
    fn from_json(source: &str) -> Result<Self, ProviderError> {
        let value: serde_json::Value =
            serde_json::from_str(source).map_err(|error| invalid("tokenizer.json", error))?;
        let model = value
            .get("model")
            .ok_or_else(|| missing("tokenizer.json", "model"))?;
        let kind = model.get("type").and_then(|value| value.as_str());
        if kind != Some("WordLevel") {
            return Err(ProviderError::new(
                PROVIDER,
                format!("unsupported tokenizer model type {kind:?}: expected WordLevel vocabulary"),
            ));
        }
        let vocab_object = model
            .get("vocab")
            .and_then(|value| value.as_object())
            .ok_or_else(|| missing("tokenizer.json", "model.vocab"))?;
        let mut vocab = BTreeMap::new();
        for (token, id) in vocab_object {
            let id = id.as_u64().ok_or_else(|| {
                ProviderError::new(
                    PROVIDER,
                    format!("tokenizer vocab id for {token:?} is not an integer"),
                )
            })?;
            let id = u32::try_from(id).map_err(|_| {
                ProviderError::new(
                    PROVIDER,
                    format!("tokenizer vocab id for {token:?} exceeds u32 range"),
                )
            })?;
            vocab.insert(token.clone(), id);
        }
        let unk_token = model
            .get("unk_token")
            .and_then(|value| value.as_str())
            .ok_or_else(|| missing("tokenizer.json", "model.unk_token"))?
            .to_string();
        let unk_id = *vocab.get(&unk_token).ok_or_else(|| {
            ProviderError::new(
                PROVIDER,
                format!("unk_token {unk_token:?} is missing from the vocabulary"),
            )
        })?;
        Ok(Self { vocab, unk_id })
    }

    fn encode(&self, text: &str) -> Vec<u32> {
        casefold_text(text)
            .split_whitespace()
            .map(|token| self.vocab.get(token).copied().unwrap_or(self.unk_id))
            .collect()
    }
}

/// Local Candle-backed static embedding provider.
///
/// Mean-pooled, L2-normalized token vectors computed with Candle CPU
/// tensors. Deterministic for a fixed model directory.
#[derive(Debug, Clone)]
pub struct CandleEmbeddingProvider {
    directory: PathBuf,
    tokenizer: WordLevel,
    weight: Tensor,
    dimensions: usize,
    model_id: String,
    revision: Option<String>,
    languages: Vec<String>,
}

impl CandleEmbeddingProvider {
    /// Load `tokenizer.json` and `embeddings.safetensors` from `directory`.
    pub fn open(directory: impl AsRef<Path>) -> Result<Self, ProviderError> {
        let directory = directory.as_ref().to_path_buf();
        let tokenizer_source =
            std::fs::read_to_string(directory.join(TOKENIZER_FILE)).map_err(|error| {
                ProviderError::new(
                    PROVIDER,
                    format!(
                        "cannot read {}: {error}",
                        directory.join(TOKENIZER_FILE).display()
                    ),
                )
            })?;
        let tokenizer = WordLevel::from_json(&tokenizer_source)?;
        let tensors = candle_core::safetensors::load(directory.join(WEIGHTS_FILE), &Device::Cpu)
            .map_err(|error| {
                ProviderError::new(
                    PROVIDER,
                    format!(
                        "cannot load {}: {error}",
                        directory.join(WEIGHTS_FILE).display()
                    ),
                )
            })?;
        let weight = tensors.get(WEIGHT_TENSOR).ok_or_else(|| {
            ProviderError::new(
                PROVIDER,
                format!("{WEIGHTS_FILE} is missing the {WEIGHT_TENSOR:?} tensor"),
            )
        })?;
        let shape = weight.shape().dims().to_vec();
        if shape.len() != 2 || shape[0] == 0 || shape[1] == 0 {
            return Err(ProviderError::new(
                PROVIDER,
                format!("{WEIGHT_TENSOR:?} must have shape [vocab, dim], found {shape:?}"),
            ));
        }
        if weight.dtype() != DType::F32 {
            return Err(ProviderError::new(
                PROVIDER,
                format!(
                    "{WEIGHT_TENSOR:?} must be float32, found {:?}",
                    weight.dtype()
                ),
            ));
        }
        if shape[0] != tokenizer.vocab.len() {
            return Err(ProviderError::new(
                PROVIDER,
                format!(
                    "vocabulary size {} does not match {WEIGHT_TENSOR:?} rows {}",
                    tokenizer.vocab.len(),
                    shape[0]
                ),
            ));
        }
        let model_id = directory
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "candle-static".to_string());
        Ok(Self {
            directory,
            tokenizer,
            weight: weight.clone(),
            dimensions: shape[1],
            model_id,
            revision: None,
            languages: vec!["multilingual".to_string()],
        })
    }

    /// Override the reported model identifier (default: directory name).
    pub fn with_model_id(mut self, model_id: impl Into<String>) -> Self {
        self.model_id = model_id.into();
        self
    }

    /// Override the reported model revision.
    pub fn with_revision(mut self, revision: impl Into<String>) -> Self {
        self.revision = Some(revision.into());
        self
    }

    /// Override the reported language coverage.
    pub fn with_languages(mut self, languages: Vec<String>) -> Self {
        self.languages = languages;
        self
    }

    /// Directory the model was loaded from.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Vocabulary size.
    pub fn vocabulary_size(&self) -> usize {
        self.tokenizer.vocab.len()
    }

    fn vectorize(&self, text: &str) -> Result<Vec<f32>, ProviderError> {
        let ids = self.tokenizer.encode(text);
        if ids.is_empty() {
            return Ok(vec![0.0; self.dimensions]);
        }
        let selected = self
            .weight
            .index_select(&Tensor::new(ids, &Device::Cpu).map_err(internal)?, 0)
            .map_err(internal)?;
        let pooled = selected.mean(0).map_err(internal)?;
        let norm = pooled
            .sqr()
            .map_err(internal)?
            .sum_all()
            .map_err(internal)?
            .to_scalar::<f32>()
            .map_err(internal)?
            .sqrt();
        let normalized = if norm > 0.0 {
            pooled
                .broadcast_div(&Tensor::new(norm, &Device::Cpu).map_err(internal)?)
                .map_err(internal)?
        } else {
            pooled
        };
        normalized.to_vec1::<f32>().map_err(internal)
    }
}

impl EmbeddingProviderTrait for CandleEmbeddingProvider {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        texts.iter().map(|text| self.vectorize(text)).collect()
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        // One batch per call; callers chunk to `max_batch_size` and the
        // shared embedding cache absorbs repeated inputs.
        self.embed(texts)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        // Static mean-pooled token embeddings: a Basic local backend, not a
        // contextual transformer encoder.
        let mut capabilities = ProviderCapabilities::new(PROVIDER)
            .with_dimensions(self.dimensions)
            .with_languages(self.languages.clone())
            .with_quality(CapabilityLevel::Basic);
        if let Some(revision) = &self.revision {
            capabilities = capabilities.with_model_revision(revision.clone());
        }
        capabilities
    }

    fn model_metadata(&self) -> Option<ModelMetadata> {
        Some(ModelMetadata {
            model_id: self.model_id.clone(),
            revision: self.revision.clone(),
            dimensions: self.dimensions,
            normalized: true,
            languages: self.languages.clone(),
            source: Some("local candle tensor".to_string()),
            license: None,
        })
    }
}

fn invalid(file: &str, error: serde_json::Error) -> ProviderError {
    ProviderError::new(PROVIDER, format!("{file} is not valid JSON: {error}"))
}

fn missing(file: &str, field: &str) -> ProviderError {
    ProviderError::new(PROVIDER, format!("{file} is missing {field:?}"))
}

fn internal(error: candle_core::Error) -> ProviderError {
    ProviderError::new(PROVIDER, format!("tensor error: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::providers::EmbeddingProvider as EmbeddingProviderTrait;
    use crate::semantic::embeddings::CachedEmbeddingProvider;
    use crate::EngineConfig;
    use crate::TextIntelligence;

    const DIM: usize = 8;

    fn write_test_model(directory: &Path, tokens: &[&str]) {
        let mut vocab = BTreeMap::new();
        for (index, token) in tokens.iter().enumerate() {
            vocab.insert((*token).to_string(), index as u32);
        }
        let tokenizer = serde_json::json!({
            "version": "1.0",
            "model": {
                "type": "WordLevel",
                "vocab": vocab,
                "unk_token": "[UNK]",
            },
            "pre_tokenizer": {"type": "Whitespace"},
        });
        std::fs::write(
            directory.join(TOKENIZER_FILE),
            serde_json::to_string(&tokenizer).unwrap(),
        )
        .unwrap();
        let weight = Tensor::rand(0f32, 1f32, (tokens.len(), DIM), &Device::Cpu).unwrap();
        let mut tensors = std::collections::HashMap::new();
        tensors.insert(WEIGHT_TENSOR.to_string(), weight);
        candle_core::safetensors::save(&tensors, directory.join(WEIGHTS_FILE)).unwrap();
    }

    fn test_dir(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("textintel-candle-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn embeds_batches_deterministically() {
        let directory = test_dir("basic");
        write_test_model(&directory, &["[UNK]", "hola", "mundo", "hello", "world"]);
        let provider = CandleEmbeddingProvider::open(&directory).unwrap();
        assert_eq!(provider.vocabulary_size(), 5);
        let metadata = provider.model_metadata().unwrap();
        assert_eq!(metadata.dimensions, DIM);
        assert!(metadata.normalized);
        assert_eq!(provider.capabilities().dimensions, Some(DIM));

        let texts = vec!["hola mundo".to_string(), "hello world".to_string()];
        let first = EmbeddingProviderTrait::embed(&provider, &texts).unwrap();
        let second = EmbeddingProviderTrait::embed_batch(&provider, &texts).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 2);
        for vector in &first {
            assert_eq!(vector.len(), DIM);
            assert!(vector.iter().all(|value| value.is_finite()));
            let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
            assert!((norm - 1.0).abs() < 1e-5, "vector is L2-normalized");
        }
        // Unknown words fall back to [UNK].
        let unknown = EmbeddingProviderTrait::embed(&provider, &["zzqqxx".to_string()]).unwrap();
        let unk = EmbeddingProviderTrait::embed(&provider, &["[UNK]".to_string()]).unwrap();
        assert_eq!(unknown, unk);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn validates_model_files() {
        assert!(CandleEmbeddingProvider::open("/nonexistent-model-dir-xyz").is_err());

        let directory = test_dir("noweights");
        write_test_model(&directory, &["[UNK]", "hola"]);
        std::fs::remove_file(directory.join(WEIGHTS_FILE)).unwrap();
        assert!(CandleEmbeddingProvider::open(&directory).is_err());
        let _ = std::fs::remove_dir_all(&directory);

        let directory = test_dir("mismatch");
        write_test_model(&directory, &["[UNK]", "hola", "mundo"]);
        let bad = Tensor::rand(0f32, 1f32, (2, DIM), &Device::Cpu).unwrap();
        let mut tensors = std::collections::HashMap::new();
        tensors.insert(WEIGHT_TENSOR.to_string(), bad);
        candle_core::safetensors::save(&tensors, directory.join(WEIGHTS_FILE)).unwrap();
        let error = CandleEmbeddingProvider::open(&directory).unwrap_err();
        assert!(error.to_string().contains("does not match"));
        let _ = std::fs::remove_dir_all(&directory);

        let directory = test_dir("tokenizer");
        std::fs::write(
            directory.join(TOKENIZER_FILE),
            r#"{"model": {"type": "BPE", "vocab": {}, "unk_token": "[UNK]"}}"#,
        )
        .unwrap();
        let error = CandleEmbeddingProvider::open(&directory).unwrap_err();
        assert!(error.to_string().contains("WordLevel"));
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn powers_engine_semantic_channel() {
        let directory = test_dir("engine");
        write_test_model(&directory, &["[UNK]", "compra", "ahora"]);
        let engine = TextIntelligence::new(EngineConfig {
            semantic: true,
            ..EngineConfig::default()
        })
        .with_embedding_provider(
            CandleEmbeddingProvider::open(&directory)
                .unwrap()
                .with_model_id("test-candle")
                .with_languages(vec!["es".to_string()]),
        );
        let fingerprint = engine.analyze("compra ahora").unwrap();
        assert!(fingerprint.semantic_embeddings.contains_key("default"));
        assert_eq!(
            fingerprint.metadata.get("semantic_model"),
            Some(&"test-candle".to_string())
        );
        assert_eq!(
            fingerprint.metadata.get("semantic_dimensions"),
            Some(&DIM.to_string())
        );
        let comparison = engine.compare("compra ahora", "compra ahora").unwrap();
        assert!(comparison.semantic.unwrap_or(0.0) > 0.99);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn composes_with_embedding_cache() {
        let directory = test_dir("cached");
        write_test_model(&directory, &["[UNK]", "hola"]);
        let cached =
            CachedEmbeddingProvider::new(CandleEmbeddingProvider::open(&directory).unwrap(), 16);
        let texts = vec!["hola".to_string()];
        assert_eq!(
            EmbeddingProviderTrait::embed(&cached, &texts).unwrap(),
            EmbeddingProviderTrait::embed(&cached, &texts).unwrap()
        );
        let _ = std::fs::remove_dir_all(&directory);
    }
}
