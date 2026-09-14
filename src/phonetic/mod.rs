#[cfg(feature = "phonetic-espeak")]
pub mod espeak;
pub mod features;
pub mod g2p;
pub mod ipa;
pub mod similarity;

#[cfg(feature = "phonetic-espeak")]
pub use espeak::{
    parse_espeak_ipa, parse_voices_table, primary_stress_syllables, EspeakNgG2PProvider,
    EspeakVoice, DEFAULT_ESPEAK_TIMEOUT,
};
pub use features::{articulatory_distance, feature_label};
pub use g2p::{G2PProvider, NullG2PProvider, RuleBasedG2PProvider};
pub use ipa::parse_ipa;
pub use similarity::{phoneme_edit_distance, phonetic_similarity, weighted_phoneme_distance};
