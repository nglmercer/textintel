pub mod duplicates;
pub mod patterns;
pub mod spam;

pub use duplicates::{duplicate_result, duplicate_result_with_mode};
pub use patterns::match_pattern;
pub use spam::{predict_spam, spam_features, HeuristicSpamPredictor};
