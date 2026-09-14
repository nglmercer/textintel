pub mod beam_search;
pub mod decoder;
pub mod scorer;
pub mod tokenizer;

pub use decoder::RebusDecoder;
pub use scorer::RebusEvidence;

/// Whole-text semantic evidence callback: maps `(surface, source)` to a
/// similarity in `[0.0, 1.0]`, or `None` when unavailable.
pub type SemanticEvidence = dyn Fn(&str, &str) -> Option<f64>;
