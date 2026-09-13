pub mod character;
pub mod minhash;
pub mod ngrams;
pub mod similarity;
pub mod tokenizer;

pub use character::{character_similarity, combined_character_similarity};
pub use similarity::lexical_similarity;
pub use tokenizer::{simple_lemmas, stop_words, tokenize};
