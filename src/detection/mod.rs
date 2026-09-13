pub mod duplicates;
pub mod patterns;
pub mod spam;

pub use duplicates::duplicate_result;
pub use patterns::match_pattern;
pub use spam::predict_spam;

