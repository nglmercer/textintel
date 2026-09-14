//! Production multilingual G2P backed by the `espeak-ng` command-line tool.
//!
//! The provider shells out to a local `espeak-ng` binary (no network, no
//! model download) and parses `--ipa` output with the crate's [`parse_ipa`]
//! tokenizer. Language coverage follows the voices shipped by the installed
//! espeak-ng build; [`EspeakNgG2PProvider::supported_languages`] lists the
//! curated default mapping and any installed voice can be forced with
//! [`EspeakNgG2PProvider::for_voice`].
//!
//! [`parse_ipa`]: crate::phonetic::parse_ipa

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::core::capabilities::{CapabilityLevel, ProviderCapabilities};
use crate::core::error::ProviderError;
use crate::core::providers::G2PProvider as G2PProviderTrait;
use crate::core::types::PhoneticCandidate;
use crate::phonetic::ipa::parse_ipa;

pub use crate::core::providers::G2PProvider;

const PROVIDER: &str = "espeak_ng_g2p";
const DEFAULT_BINARY: &str = "espeak-ng";
const DEFAULT_VOICE: &str = "en";

/// Fixed tool confidence: espeak-ng emits no score, so this is a constant
/// marker of tool output, not a calibrated probability.
const TOOL_CONFIDENCE: f64 = 0.8;

/// Curated language code to espeak-ng voice mapping. Coverage depends on the
/// installed espeak-ng build; unknown languages fall back to the default
/// voice and any voice can be forced explicitly.
const VOICES: &[(&str, &str)] = &[
    ("af", "af"),
    ("am", "am"),
    ("ar", "ar"),
    ("az", "az"),
    ("be", "be"),
    ("bg", "bg"),
    ("bn", "bn"),
    ("bs", "bs"),
    ("ca", "ca"),
    ("cs", "cs"),
    ("cy", "cy"),
    ("da", "da"),
    ("de", "de"),
    ("el", "el"),
    ("en", "en"),
    ("es", "es"),
    ("et", "et"),
    ("eu", "eu"),
    ("fa", "fa"),
    ("fi", "fi"),
    ("fr", "fr"),
    ("ga", "ga"),
    ("gl", "gl"),
    ("gu", "gu"),
    ("he", "he"),
    ("hi", "hi"),
    ("hr", "hr"),
    ("hu", "hu"),
    ("hy", "hy"),
    ("id", "id"),
    ("is", "is"),
    ("it", "it"),
    ("ja", "ja"),
    ("ka", "ka"),
    ("kk", "kk"),
    ("km", "km"),
    ("kn", "kn"),
    ("ko", "ko"),
    ("ky", "ky"),
    ("lo", "lo"),
    ("lt", "lt"),
    ("lv", "lv"),
    ("mk", "mk"),
    ("ml", "ml"),
    ("mn", "mn"),
    ("mr", "mr"),
    ("ms", "ms"),
    ("mt", "mt"),
    ("nb", "nb"),
    ("ne", "ne"),
    ("nl", "nl"),
    ("pa", "pa"),
    ("pl", "pl"),
    ("pt", "pt"),
    ("ro", "ro"),
    ("ru", "ru"),
    ("si", "si"),
    ("sk", "sk"),
    ("sl", "sl"),
    ("sq", "sq"),
    ("sr", "sr"),
    ("sv", "sv"),
    ("sw", "sw"),
    ("ta", "ta"),
    ("te", "te"),
    ("tg", "tg"),
    ("th", "th"),
    ("tr", "tr"),
    ("uk", "uk"),
    ("ur", "ur"),
    ("uz", "uz"),
    ("vi", "vi"),
    ("yue", "yue"),
    ("zh", "zh"),
];

/// Vowel characters for syllable-nucleus counting (same set used by the
/// articulatory feature classifier).
const VOWELS: &str = "aeiouəɛɪɔʊɑɒɨʉɯyøœæʌäɵɞɘɜɐɨ";

/// Production G2P provider delegating to a local `espeak-ng` binary.
#[derive(Debug, Clone)]
pub struct EspeakNgG2PProvider {
    binary: PathBuf,
    default_voice: String,
}

impl Default for EspeakNgG2PProvider {
    fn default() -> Self {
        Self {
            binary: PathBuf::from(DEFAULT_BINARY),
            default_voice: DEFAULT_VOICE.to_string(),
        }
    }
}

impl EspeakNgG2PProvider {
    /// Use the `espeak-ng` binary found on `PATH` with the default voice.
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the binary path (useful for tests and custom installs).
    pub fn with_binary(mut self, binary: impl AsRef<Path>) -> Self {
        self.binary = binary.as_ref().to_path_buf();
        self
    }

    /// Force a specific espeak-ng voice (also reported as the dialect).
    /// Accepts forms like `"en-US"`, `"pt-BR"`, `"es-419"`.
    pub fn for_voice(mut self, voice: impl Into<String>) -> Self {
        self.default_voice = voice.into();
        self
    }

    /// Detect an `espeak-ng` binary on `PATH` without executing anything.
    /// Use this for the production preset; when it fails, fall back to
    /// [`RuleBasedG2PProvider`](crate::phonetic::RuleBasedG2PProvider) and
    /// report degraded phonetic quality instead of failing the analysis.
    pub fn auto_detect() -> Result<Self, ProviderError> {
        match find_binary(DEFAULT_BINARY) {
            Some(binary) => Ok(Self {
                binary,
                default_voice: DEFAULT_VOICE.to_string(),
            }),
            None => Err(ProviderError::new(
                PROVIDER,
                "espeak-ng not found on PATH (install espeak-ng for production G2P; \
                 falling back to the rule-based provider with Basic quality)",
            )),
        }
    }

    /// True when [`Self::auto_detect`] would succeed.
    pub fn is_available() -> bool {
        find_binary(DEFAULT_BINARY).is_some()
    }

    /// Languages covered by the curated default voice mapping.
    pub fn supported_languages() -> Vec<String> {
        let mut languages: Vec<String> = VOICES
            .iter()
            .map(|(language, _)| (*language).to_string())
            .collect();
        languages.sort();
        languages.dedup();
        languages
    }

    /// Voice used for `language`, or the configured default voice.
    pub fn voice_for(&self, language: &str) -> &str {
        let requested = language.to_lowercase();
        for (code, voice) in VOICES {
            if *code == requested {
                return voice;
            }
        }
        &self.default_voice
    }

    fn run(&self, text: &str, voice: &str) -> Result<String, ProviderError> {
        let output = Command::new(&self.binary)
            .args(["-q", "--ipa=3", "-v", voice, text])
            .output()
            .map_err(|error| {
                ProviderError::new(
                    PROVIDER,
                    format!(
                        "cannot execute {}: {error} (install espeak-ng for production G2P)",
                        self.binary.display()
                    ),
                )
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let clipped: String = stderr.chars().take(300).collect();
            return Err(ProviderError::new(
                PROVIDER,
                format!(
                    "espeak-ng exited with {status}: {clipped}",
                    status = output.status
                ),
            ));
        }
        String::from_utf8(output.stdout).map_err(|error| {
            ProviderError::new(PROVIDER, format!("espeak-ng output is not UTF-8: {error}"))
        })
    }
}

/// Locate an executable `name` on `PATH` without running it, so detection is
/// bounded and side-effect free.
fn find_binary(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&paths) {
        let candidate = directory.join(name);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    path.is_file() && check_exec_bit(path)
}

#[cfg(unix)]
fn check_exec_bit(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// Parse raw `--ipa` stdout into display IPA, phoneme tokens, and a syllable
/// estimate (vowel-nucleus groups, at least one per non-empty output).
pub fn parse_espeak_ipa(stdout: &str) -> (String, Vec<String>, usize) {
    let ipa = stdout.trim().to_string();
    let phonemes = parse_ipa(&ipa);
    (ipa, phonemes.clone(), syllable_nuclei(&phonemes))
}

fn syllable_nuclei(phonemes: &[String]) -> usize {
    if phonemes.is_empty() {
        return 0;
    }
    let mut nuclei = 0;
    let mut previous_vowel = false;
    for phoneme in phonemes {
        let vowel = phoneme.chars().any(|ch| VOWELS.contains(ch));
        if vowel && !previous_vowel {
            nuclei += 1;
        }
        previous_vowel = vowel;
    }
    nuclei.max(1)
}

impl G2PProviderTrait for EspeakNgG2PProvider {
    fn phonemize(&self, text: &str, language: &str) -> Result<PhoneticCandidate, ProviderError> {
        let voice = self.voice_for(language).to_string();
        let stdout = self.run(text, &voice)?;
        let (ipa, phonemes, syllables) = parse_espeak_ipa(&stdout);
        Ok(PhoneticCandidate {
            source: text.to_string(),
            language: language.to_string(),
            dialect: Some(voice),
            ipa: Some(ipa),
            phonemes,
            stress: None,
            syllables,
            articulatory_features: Vec::new(),
            confidence: if text.is_empty() {
                0.0
            } else {
                TOOL_CONFIDENCE
            },
        })
    }

    fn phonemize_batch(
        &self,
        texts: &[String],
        language: &str,
    ) -> Result<Vec<PhoneticCandidate>, ProviderError> {
        // One bounded subprocess per text; callers chunk to `max_batch_size`.
        texts
            .iter()
            .map(|text| self.phonemize(text, language))
            .collect()
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new(PROVIDER)
            .with_languages(Self::supported_languages())
            .with_quality(CapabilityLevel::Production)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_espeak_ipa_output() {
        let (ipa, phonemes, syllables) = parse_espeak_ipa("olˈa\n");
        assert_eq!(ipa, "olˈa");
        assert_eq!(phonemes, parse_ipa("olˈa"));
        assert_eq!(syllables, 2);
        let (_, phonemes, syllables) = parse_espeak_ipa("");
        assert!(phonemes.is_empty());
        assert_eq!(syllables, 0);
    }

    #[test]
    fn resolves_voices_and_lists_languages() {
        let provider = EspeakNgG2PProvider::new();
        assert_eq!(provider.voice_for("es"), "es");
        assert_eq!(provider.voice_for("ES"), "es");
        assert_eq!(provider.voice_for("xx-unknown"), "en");
        let forced = EspeakNgG2PProvider::new().for_voice("en-US");
        assert_eq!(forced.voice_for("xx-unknown"), "en-US");
        let languages = EspeakNgG2PProvider::supported_languages();
        assert!(languages.contains(&"es".to_string()));
        assert!(languages.contains(&"ja".to_string()));
        assert!(languages.contains(&"zh".to_string()));
        assert!(languages.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn reports_missing_binary() {
        let provider = EspeakNgG2PProvider::new().with_binary("/nonexistent-espeak-ng-binary-xyz");
        let error = provider.phonemize("hola", "es").unwrap_err();
        assert!(error.to_string().contains("install espeak-ng"));
    }

    #[cfg(unix)]
    #[test]
    fn phonemizes_through_mock_binary() {
        use std::os::unix::fs::PermissionsExt;

        let directory =
            std::env::temp_dir().join(format!("textintel-espeak-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let script = directory.join("espeak-ng");
        // Mock records its argv and emits canned IPA for the last argument.
        std::fs::write(
            &script,
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$ESPEAK_MOCK_LOG\"\nprintf 'olˈa\\n'\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let log = directory.join("argv.log");
        std::env::set_var("ESPEAK_MOCK_LOG", &log);
        let provider = EspeakNgG2PProvider::new()
            .with_binary(&script)
            .for_voice("es");
        let candidate = provider.phonemize("hola", "es").unwrap();
        assert_eq!(candidate.ipa.as_deref(), Some("olˈa"));
        assert_eq!(candidate.phonemes, parse_ipa("olˈa"));
        assert_eq!(candidate.syllables, 2);
        assert_eq!(candidate.language, "es");
        assert_eq!(candidate.dialect.as_deref(), Some("es"));
        assert_eq!(candidate.confidence, TOOL_CONFIDENCE);
        let argv = std::fs::read_to_string(&log).unwrap();
        assert!(argv.contains("-v"), "voice flag forwarded: {argv}");
        assert!(argv.contains("\nes\n"), "mapped voice forwarded: {argv}");
        assert!(argv.contains("--ipa=3"), "ipa flag forwarded: {argv}");
        std::env::remove_var("ESPEAK_MOCK_LOG");
        let _ = std::fs::remove_dir_all(&directory);
    }
}
