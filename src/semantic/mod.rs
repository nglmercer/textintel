pub mod embeddings;
pub mod similarity;

pub use embeddings::{EmbeddingProvider, NullEmbeddingProvider, StaticEmbeddingProvider};
pub use similarity::cosine;

