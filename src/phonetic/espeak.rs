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

/// One voice reported by `espeak-ng --voices`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EspeakVoice {
    /// BCP-style language code from the voices table (e.g. `"es"`, `"en-US"`).
    pub language: String,
    /// Voice name (e.g. `"spanish"`, `"english"`).
    pub name: String,
    /// Gender marker from the table (`"M"`, `"F"`, or `"-"`).
    pub gender: String,
    /// Voice file backing this voice.
    pub file: String,
}

/// Production G2P provider delegating to a local `espeak-ng` binary.
#[derive(Debug, Clone)]
pub struct EspeakNgG2PProvider {
    binary: PathBuf,
    default_voice: String,
    timeout: std::time::Duration,
}

/// Subprocess bound for a single espeak-ng invocation.
pub const DEFAULT_ESPEAK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

impl Default for EspeakNgG2PProvider {
    fn default() -> Self {
        Self {
            binary: PathBuf::from(DEFAULT_BINARY),
            default_voice: DEFAULT_VOICE.to_string(),
            timeout: DEFAULT_ESPEAK_TIMEOUT,
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

    /// Bound for a single espeak-ng subprocess call. Exceeding it kills the
    /// child and reports a timeout error instead of hanging the analysis.
    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn timeout(&self) -> std::time::Duration {
        self.timeout
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
                timeout: DEFAULT_ESPEAK_TIMEOUT,
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

    /// True when `language` has a curated voice mapping. Anything else falls
    /// back to the default voice at reduced confidence (see
    /// [`G2PProviderTrait::phonemize`]) instead of pretending full coverage.
    pub fn is_supported_language(language: &str) -> bool {
        let requested = language.to_lowercase();
        VOICES.iter().any(|(code, _)| *code == requested)
    }

    /// Voices installed with this espeak-ng binary (`espeak-ng --voices`).
    /// Returns an error when the binary is missing or the table is unreadable.
    pub fn installed_voices(&self) -> Result<Vec<EspeakVoice>, ProviderError> {
        let stdout = self.run_command(&["--voices"])?;
        Ok(parse_voices_table(&stdout))
    }

    fn run(&self, text: &str, voice: &str) -> Result<String, ProviderError> {
        // Text travels as one argv element; no shell is ever involved.
        self.run_command(&["-q", "--ipa=3", "-v", voice, text])
    }

    /// Run the binary with argv (no shell) under the configured timeout.
    fn run_command(&self, args: &[&str]) -> Result<String, ProviderError> {
        let mut child = Command::new(&self.binary)
            .args(args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|error| {
                ProviderError::new(
                    PROVIDER,
                    format!(
                        "cannot execute {}: {error} (install espeak-ng for production G2P)",
                        self.binary.display()
                    ),
                )
            })?;
        let deadline = std::time::Instant::now() + self.timeout;
        loop {
            match child.try_wait().map_err(|error| {
                ProviderError::new(PROVIDER, format!("cannot poll espeak-ng: {error}"))
            })? {
                Some(status) => {
                    let mut stdout = Vec::new();
                    let mut stderr = Vec::new();
                    if let Some(mut pipe) = child.stdout.take() {
                        use std::io::Read;
                        let _ = pipe.read_to_end(&mut stdout);
                    }
                    if let Some(mut pipe) = child.stderr.take() {
                        use std::io::Read;
                        let _ = pipe.read_to_end(&mut stderr);
                    }
                    if !status.success() {
                        let clipped: String =
                            String::from_utf8_lossy(&stderr).chars().take(300).collect();
                        return Err(ProviderError::new(
                            PROVIDER,
                            format!("espeak-ng exited with {status}: {clipped}"),
                        ));
                    }
                    return String::from_utf8(stdout).map_err(|error| {
                        ProviderError::new(
                            PROVIDER,
                            format!("espeak-ng output is not UTF-8: {error}"),
                        )
                    });
                }
                None => {
                    if std::time::Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(ProviderError::new(
                            PROVIDER,
                            format!(
                                "espeak-ng timed out after {}ms; increase with with_timeout()",
                                self.timeout.as_millis()
                            ),
                        ));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            }
        }
    }
}

/// Parse `espeak-ng --voices` table output. The header line and malformed
/// rows are skipped; parsing never fails the whole call.
pub fn parse_voices_table(stdout: &str) -> Vec<EspeakVoice> {
    let mut voices = Vec::new();
    for line in stdout.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 6 {
            continue;
        }
        if fields[0].eq_ignore_ascii_case("pty") {
            continue;
        }
        if fields[0].parse::<u32>().is_err() {
            continue;
        }
        voices.push(EspeakVoice {
            language: fields[1].to_string(),
            gender: fields[3].to_string(),
            name: fields[4].to_string(),
            file: fields[5..].join(" "),
        });
    }
    voices
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

/// Recover 0-based indices of primary-stressed syllables from raw espeak-ng
/// IPA output. `ˈ` marks the onset of the stressed syllable; `ˌ` (secondary)
/// is ignored. Returns an empty vector when no primary marker is present.
pub fn primary_stress_syllables(ipa: &str) -> Vec<usize> {
    let mut stressed = Vec::new();
    let mut offset = 0;
    for chunk in ipa.split_whitespace() {
        let mut groups_before = 0;
        let mut in_vowel = false;
        let mut marker = None;
        for ch in chunk.chars() {
            if ch == 'ˈ' {
                marker = Some(groups_before);
                in_vowel = false;
            } else if VOWELS.contains(ch) {
                if !in_vowel {
                    groups_before += 1;
                }
                in_vowel = true;
            } else {
                in_vowel = false;
            }
        }
        if let Some(local) = marker {
            stressed.push(offset + local);
        }
        offset += groups_before;
    }
    stressed
}

impl G2PProviderTrait for EspeakNgG2PProvider {
    fn phonemize(&self, text: &str, language: &str) -> Result<PhoneticCandidate, ProviderError> {
        let exact = Self::is_supported_language(language);
        let voice = self.voice_for(language).to_string();
        let stdout = self.run(text, &voice)?;
        let (ipa, phonemes, syllables) = parse_espeak_ipa(&stdout);
        let stress = primary_stress_syllables(&ipa);
        Ok(PhoneticCandidate {
            source: text.to_string(),
            language: language.to_string(),
            dialect: Some(voice),
            ipa: Some(ipa),
            articulatory_features: phonemes
                .iter()
                .map(|phoneme| crate::phonetic::features::feature_label(phoneme).to_string())
                .collect(),
            phonemes,
            stress: if stress.is_empty() {
                None
            } else {
                Some(stress)
            },
            syllables,
            confidence: if text.is_empty() {
                0.0
            } else if exact {
                TOOL_CONFIDENCE
            } else {
                // Default-voice fallback for an unmapped language: usable but
                // explicitly discounted, never presented as full coverage.
                0.5
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
    fn parses_voices_table_output() {
        let table = "Pty Language Age Gender VoiceName File\n\
             5  af       --  M      afrikaans          af\n\
             5  en       --  M      english            default\n\
             2  es       --  M      spanish            es\n\
             garbage line\n";
        let voices = parse_voices_table(table);
        assert_eq!(voices.len(), 3);
        assert_eq!(
            voices[1],
            EspeakVoice {
                language: "en".to_string(),
                name: "english".to_string(),
                gender: "M".to_string(),
                file: "default".to_string(),
            }
        );
        assert!(parse_voices_table("").is_empty());
        assert!(parse_voices_table("Pty Language Age Gender VoiceName File\n").is_empty());
    }

    #[test]
    fn recovers_primary_stress_positions() {
        assert_eq!(primary_stress_syllables("olˈa"), vec![1]);
        assert_eq!(primary_stress_syllables("ˈola"), vec![0]);
        assert_eq!(primary_stress_syllables("kafe"), Vec::<usize>::new());
        assert_eq!(primary_stress_syllables(""), Vec::<usize>::new());
        // Secondary stress is ignored; offsets accumulate across words.
        assert_eq!(primary_stress_syllables("ˈola kaˌfe"), vec![0]);
        assert_eq!(primary_stress_syllables("ˈola kaˈfe"), vec![0, 3]);
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

        let directory = std::env::temp_dir().join(format!(
            "textintel-espeak-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
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
        unsafe {
            std::env::set_var("ESPEAK_MOCK_LOG", &log);
        }
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
        unsafe {
            std::env::remove_var("ESPEAK_MOCK_LOG");
        }
        let _ = std::fs::remove_dir_all(&directory);
    }
}
