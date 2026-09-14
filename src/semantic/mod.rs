#[cfg(feature = "semantic-candle")]
pub mod candle;
pub mod embeddings;
#[cfg(feature = "semantic-http")]
pub mod http;
pub mod similarity;

#[cfg(feature = "semantic-candle")]
pub use candle::CandleEmbeddingProvider;
pub use embeddings::{
    CachedEmbeddingProvider, EmbeddingProvider, FeatureHashEmbeddingProvider,
    NullEmbeddingProvider, StaticEmbeddingProvider,
};
#[cfg(feature = "semantic-http")]
pub use http::HttpEmbeddingProvider;
pub use similarity::cosine;
