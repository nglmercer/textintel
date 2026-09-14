pub mod embeddings;
#[cfg(feature = "semantic-http")]
pub mod http;
pub mod similarity;

pub use embeddings::{
    CachedEmbeddingProvider, EmbeddingProvider, FeatureHashEmbeddingProvider,
    NullEmbeddingProvider, StaticEmbeddingProvider,
};
#[cfg(feature = "semantic-http")]
pub use http::HttpEmbeddingProvider;
pub use similarity::cosine;
