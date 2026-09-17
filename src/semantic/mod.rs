pub mod catalog;
pub mod embeddings;
#[cfg(feature = "semantic-http")]
pub mod http;
pub mod liquid;
pub mod similarity;
#[cfg(feature = "semantic-transformer")]
pub mod transformer;

pub use catalog::{
    ARCTIC_EMBED_XS, LFM2_5_230M, LFM2_5_350M, MODERN_EMBEDDING_MODELS, MODERN_GENERATIVE_MODELS,
    MULTILINGUAL_E5_SMALL, MXBAI_EMBED_XSMALL, embedding_model, generative_model,
};
pub use embeddings::{
    CachedEmbeddingProvider, EmbeddingProvider, FallbackEmbeddingProvider,
    FeatureHashEmbeddingProvider, NullEmbeddingProvider, StaticEmbeddingProvider,
};
#[cfg(feature = "semantic-http")]
pub use http::HttpEmbeddingProvider;
pub use liquid::{
    DEFAULT_ENDPOINT as LIQUID_DEFAULT_ENDPOINT, DEFAULT_MODEL as LIQUID_DEFAULT_MODEL,
    LiquidInstructProvider,
};
pub use similarity::cosine;
#[cfg(feature = "semantic-transformer")]
pub use transformer::{EncodedBatch, TransformerEmbeddingProvider, TransformerPooling};
