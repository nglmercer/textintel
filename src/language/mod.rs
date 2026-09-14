pub mod detector;
pub mod ngram;
pub mod profile;
pub mod segmentation;

pub use ngram::NgramLanguageDetector;
pub use profile::ProfileLanguageDetector;

pub use detector::{detect_languages, DefaultLanguageDetector};
pub use segmentation::segment_message;
