pub mod detector;
pub mod ngram;
pub mod profile;
pub mod segmentation;

pub use ngram::NgramLanguageDetector;
pub use profile::ProfileLanguageDetector;

pub use detector::{DefaultLanguageDetector, detect_languages};
pub use segmentation::segment_message;
