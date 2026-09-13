pub mod detector;
pub mod segmentation;

pub use detector::{detect_languages, DefaultLanguageDetector};
pub use segmentation::segment_message;
