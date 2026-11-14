//! Vector utilities and application-supplied embedding provider boundaries.
pub mod embeddings;
pub mod similarity;

pub use embeddings::{
    CachedEmbeddingProvider, EmbeddingProvider, FallbackEmbeddingProvider,
    FeatureHashEmbeddingProvider, NullEmbeddingProvider, StaticEmbeddingProvider,
};
pub use similarity::cosine;
