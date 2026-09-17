//! Curated modern model catalog: CPU-friendly checkpoints that replace the
//! legacy MiniLM/LaBSE-era defaults.
//!
//! The catalog is data only — no downloads, no network access. Embedding
//! entries load through [`TransformerEmbeddingProvider`](crate::semantic::TransformerEmbeddingProvider)
//! from an explicit local directory; generative entries run through
//! [`LiquidInstructProvider`](crate::semantic::LiquidInstructProvider) against
//! an explicit local OpenAI-compatible server (llama-server, Ollama, vLLM).
//! See `models/README.md` for manual download commands.

use serde::{Deserialize, Serialize};

/// One modern sentence-embedding checkpoint (local safetensors directory).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModernEmbeddingModel {
    /// Hugging Face repository id, e.g. `intfloat/multilingual-e5-small`.
    pub id: &'static str,
    /// Approximate parameter count in millions.
    pub params_millions: u32,
    /// Sentence-vector dimensions.
    pub dimensions: usize,
    /// Tokenizer layout the provider must find (`vocab.txt` or
    /// `tokenizer.json`).
    pub tokenizer_file: &'static str,
    /// Recommended pooling for this checkpoint.
    pub pooling: &'static str,
    /// Prefix guidance from the model card (empty when the card needs none).
    pub prefix_note: &'static str,
    /// Language coverage in one line.
    pub languages: &'static str,
}

/// One modern generative checkpoint (served locally, e.g. as GGUF).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModernGenerativeModel {
    /// Hugging Face repository id, e.g. `LiquidAI/LFM2.5-230M`.
    pub id: &'static str,
    /// Approximate parameter count in millions.
    pub params_millions: u32,
    /// How to serve it on CPU in one line.
    pub cpu_runtime: &'static str,
    /// Language coverage in one line.
    pub languages: &'static str,
}

/// Modern multilingual default: E5-small (118M, 384 dims, mean pooling).
/// Every input carries a `query: ` / `passage: ` prefix per the model card.
pub const MULTILINGUAL_E5_SMALL: ModernEmbeddingModel = ModernEmbeddingModel {
    id: "intfloat/multilingual-e5-small",
    params_millions: 118,
    dimensions: 384,
    tokenizer_file: "tokenizer.json",
    pooling: "mean",
    prefix_note: "prefix queries with `query: ` and passages with `passage: `; \
                  symmetric tasks use `query: ` on both sides",
    languages: "multilingual (100 languages)",
};

/// Modern English CPU pick: Arctic Embed XS (22M, 384 dims, CLS pooling).
/// Retrieval queries carry the card's `Represent this sentence ...` prefix.
pub const ARCTIC_EMBED_XS: ModernEmbeddingModel = ModernEmbeddingModel {
    id: "Snowflake/snowflake-arctic-embed-xs",
    params_millions: 22,
    dimensions: 384,
    tokenizer_file: "vocab.txt",
    pooling: "cls",
    prefix_note: "prefix retrieval queries with \
                  `Represent this sentence for searching relevant passages: `",
    languages: "en",
};

/// Modern English CPU pick: MXBAI XSmall (24M, 384 dims, mean pooling).
pub const MXBAI_EMBED_XSMALL: ModernEmbeddingModel = ModernEmbeddingModel {
    id: "mixedbread-ai/mxbai-embed-xsmall-v1",
    params_millions: 24,
    dimensions: 384,
    tokenizer_file: "vocab.txt",
    pooling: "mean",
    prefix_note: "no prefix required",
    languages: "en",
};

/// All curated modern embedding checkpoints, multilingual default first.
pub const MODERN_EMBEDDING_MODELS: &[ModernEmbeddingModel] =
    &[MULTILINGUAL_E5_SMALL, ARCTIC_EMBED_XS, MXBAI_EMBED_XSMALL];

/// Smallest Liquid LFM2.5 instruct model: fastest CPU generation.
pub const LFM2_5_230M: ModernGenerativeModel = ModernGenerativeModel {
    id: "LiquidAI/LFM2.5-230M",
    params_millions: 230,
    cpu_runtime: "llama-server with an LFM2.5-230M GGUF (`/v1/chat/completions`)",
    languages: "multilingual (10 languages)",
};

/// Larger Liquid LFM2.5 instruct model: better quality, still CPU-runnable.
pub const LFM2_5_350M: ModernGenerativeModel = ModernGenerativeModel {
    id: "LiquidAI/LFM2.5-350M",
    params_millions: 350,
    cpu_runtime: "llama-server with an LFM2.5-350M GGUF (`/v1/chat/completions`)",
    languages: "multilingual (9 languages)",
};

/// All curated modern generative checkpoints, smallest first.
pub const MODERN_GENERATIVE_MODELS: &[ModernGenerativeModel] = &[LFM2_5_230M, LFM2_5_350M];

/// Look up a curated embedding checkpoint by repository id.
pub fn embedding_model(id: &str) -> Option<&'static ModernEmbeddingModel> {
    MODERN_EMBEDDING_MODELS.iter().find(|model| model.id == id)
}

/// Look up a curated generative checkpoint by repository id.
pub fn generative_model(id: &str) -> Option<&'static ModernGenerativeModel> {
    MODERN_GENERATIVE_MODELS.iter().find(|model| model.id == id)
}

/// Default modern checkpoints: multilingual E5-small embeddings and the
/// smallest Liquid LFM2.5 instruct model.
pub fn default_models() -> (
    &'static ModernEmbeddingModel,
    &'static ModernGenerativeModel,
) {
    (&MULTILINGUAL_E5_SMALL, &LFM2_5_230M)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn catalog_has_no_duplicate_ids() {
        let mut ids = BTreeSet::new();
        for model in MODERN_EMBEDDING_MODELS
            .iter()
            .map(|model| model.id)
            .chain(MODERN_GENERATIVE_MODELS.iter().map(|model| model.id))
        {
            assert!(ids.insert(model), "duplicate catalog id {model}");
            assert!(model.contains('/'), "expected a repository id, got {model}");
        }
    }

    #[test]
    fn lookups_cover_every_entry() {
        for model in MODERN_EMBEDDING_MODELS {
            assert_eq!(
                embedding_model(model.id).map(|found| found.id),
                Some(model.id)
            );
        }
        assert!(
            embedding_model("sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2")
                .is_none()
        );
        for model in MODERN_GENERATIVE_MODELS {
            assert_eq!(
                generative_model(model.id).map(|found| found.id),
                Some(model.id)
            );
        }
        assert!(generative_model("unknown/model").is_none());
        let (embedding, generative) = default_models();
        assert_eq!(embedding.id, MULTILINGUAL_E5_SMALL.id);
        assert_eq!(generative.id, LFM2_5_230M.id);
    }

    #[test]
    fn catalog_serializes_for_diagnostics() {
        let payload = serde_json::to_string(MODERN_EMBEDDING_MODELS).unwrap();
        assert!(payload.contains("multilingual-e5-small"));
        let payload = serde_json::to_string(MODERN_GENERATIVE_MODELS).unwrap();
        assert!(payload.contains("LFM2.5-230M"));
    }
}
