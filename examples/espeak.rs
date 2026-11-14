//! Reproducible G2P runtime audit through a local `espeak-ng` binary.
//!
//! ```sh
//! cargo run --features phonetic-espeak --example espeak
//! cargo run --features phonetic-espeak --example espeak -- --binary /path/to/espeak-ng --json
//! ```
//!
//! A successful process, mapped voice and nonempty IPA/phonemes are operational
//! checks, not linguistic accuracy. Japanese Kanji and Arabic vocalization
//! controls deliberately expose the engine's documented input limitations.
#![cfg(feature = "phonetic-espeak")]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{Value, json};
use textintel::core::providers::G2PProvider;
use textintel::phonetic::{EspeakNgG2PProvider, EspeakVoice};
use textintel::speech::{SpeechLocale, locales};

const HELP: &str = "Usage: cargo run --features phonetic-espeak --example espeak -- [--binary PATH] [--json]\n\
Audits 20 native-script samples and registered regional/script aliases.\n\
Success checks runtime output only, not pronunciation accuracy or audio.\n\
An absent optional PATH binary skips gracefully; an explicit bad binary or failed locale exits 1.\n\
--json emits a deterministic report; --help shows this message.";

#[derive(Default)]
struct Options {
    binary: Option<PathBuf>,
    json: bool,
    help: bool,
}

fn options() -> Result<Options, String> {
    let mut options = Options::default();
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--binary") if options.binary.is_none() => {
                let path = args.next().ok_or("--binary needs a path")?;
                if path.is_empty() {
                    return Err("--binary needs a nonempty path".into());
                }
                options.binary = Some(path.into());
            }
            Some("--json") => options.json = true,
            Some("--help" | "-h") => options.help = true,
            _ => {
                return Err(format!(
                    "unknown or repeated argument: {}",
                    arg.to_string_lossy()
                ));
            }
        }
    }
    Ok(options)
}

fn sample(locale: SpeechLocale) -> &'static str {
    use SpeechLocale::*;
    match locale {
        En => "hello",
        Es => "hola",
        Zh => "你好",
        Hi => "नमस्ते",
        Ar => "مَرْحَبًا",
        Fr => "bonjour",
        Pt => "olá",
        Ru => "привет",
        Bn => "নমস্কার",
        Ur => "سلام",
        Id => "selamat",
        De => "hallo",
        Ja => "日本語",
        Pa => "ਸਤਿ ਸ੍ਰੀ ਅਕਾਲ",
        Mr => "नमस्कार",
        Te => "నమస్కారం",
        Tr => "merhaba",
        Ta => "வணக்கம்",
        Vi => "chào",
        Ko => "안녕하세요",
    }
}

fn scope(locale: SpeechLocale) -> &'static str {
    match locale {
        SpeechLocale::Ja => {
            "Kanji probe outside eSpeak NG 1.52's documented hiragana/katakana scope; compare the kana control. Runtime success does not certify a Kanji reading."
        }
        SpeechLocale::Ar => {
            "Fully vocalized Arabic sample; compare the unvocalized control. Neither result is a native pronunciation gold."
        }
        _ => {
            "One short native-script sample; no complete script, vocabulary, dialect or linguistic accuracy claim."
        }
    }
}

fn listed(voices: &[EspeakVoice], requested: &str) -> bool {
    voices.iter().any(|voice| {
        voice.language.eq_ignore_ascii_case(requested)
            || voice.name.eq_ignore_ascii_case(requested)
            || Path::new(&voice.file)
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case(requested))
    })
}

fn probe(
    provider: &EspeakNgG2PProvider,
    voices: &[EspeakVoice],
    text: &str,
    language: &str,
    base_voice: &str,
) -> Value {
    let expected_voice = provider.voice_for(language);
    let mapped = EspeakNgG2PProvider::is_supported_language(language);
    let mut errors = Vec::new();
    if !mapped {
        errors.push("language uses an unmapped default-voice fallback".to_string());
    }
    if expected_voice != base_voice {
        errors.push("registered alias does not select its family's mapped base voice".to_string());
    }
    let (available, candidate) = match provider.phonemize(text, language) {
        Ok(candidate) => {
            if candidate.dialect.as_deref() != Some(expected_voice) {
                errors.push("candidate voice differs from the provider mapping".to_string());
            }
            if candidate.source != text || candidate.language != language {
                errors.push("candidate changed the requested source or language tag".to_string());
            }
            if candidate
                .ipa
                .as_deref()
                .is_none_or(|ipa| ipa.trim().is_empty())
            {
                errors.push("provider returned no IPA".to_string());
            }
            if candidate.phonemes.is_empty() {
                errors.push("provider returned no parsed phonemes".to_string());
            }
            (true, Some(candidate))
        }
        Err(error) => {
            errors.push(error.to_string());
            (false, None)
        }
    };
    json!({
        "requested_locale": language,
        "text": text,
        "mapped": mapped,
        "expected_voice": expected_voice,
        "voice_listed": listed(voices, expected_voice),
        "voice_available": available,
        "actual_voice": candidate.as_ref().and_then(|c| c.dialect.as_ref()),
        "ipa": candidate.as_ref().and_then(|c| c.ipa.as_ref()),
        "phonemes": candidate.as_ref().map(|c| &c.phonemes),
        "provider_confidence": candidate.as_ref().map(|c| c.confidence),
        "success": errors.is_empty(),
        "error": (!errors.is_empty()).then(|| errors.join("; ")),
    })
}

fn passed(probe: &Value) -> bool {
    probe["success"].as_bool() == Some(true)
}

fn audit(provider: &EspeakNgG2PProvider, binary: Option<&Path>) -> Value {
    let (voices, inventory_error) = match provider.installed_voices() {
        Ok(voices) if !voices.is_empty() => (voices, None),
        Ok(voices) => (
            voices,
            Some("provider returned an empty voice inventory".to_string()),
        ),
        Err(error) => (Vec::new(), Some(error.to_string())),
    };
    let mut results = Vec::new();
    for locale in SpeechLocale::ALL {
        let language = locale.code();
        let entry =
            locales::for_language(language).expect("typed locale belongs to shared registry");
        let base = probe(
            provider,
            &voices,
            sample(locale),
            language,
            provider.voice_for(language),
        );
        let tags: BTreeSet<_> = entry
            .locales
            .iter()
            .chain(entry.typed_aliases)
            .copied()
            .collect();
        let aliases: Vec<_> = tags
            .into_iter()
            .filter(|tag| *tag != language)
            .map(|tag| {
                probe(
                    provider,
                    &voices,
                    sample(locale),
                    tag,
                    provider.voice_for(language),
                )
            })
            .collect();
        let controls = match locale {
            SpeechLocale::Ja => vec![probe(
                provider,
                &voices,
                "にほんご",
                language,
                provider.voice_for(language),
            )],
            SpeechLocale::Ar => vec![probe(
                provider,
                &voices,
                "مرحبا",
                language,
                provider.voice_for(language),
            )],
            _ => Vec::new(),
        };
        let success = passed(&base) && aliases.iter().all(passed);
        let failed: Vec<_> = std::iter::once(&base)
            .chain(&aliases)
            .filter(|case| !passed(case))
            .filter_map(|case| case["requested_locale"].as_str())
            .collect();
        results.push(json!({
            "locale": language, "scope": scope(locale), "base": base, "aliases": aliases,
            "controls": controls, "controls_count_toward_success": false,
            "success": success,
            "error": (!success).then(|| format!("failed runtime probes: {}", failed.join(", "))),
        }));
    }
    let successful_locales = results.iter().filter(|row| passed(row)).count();
    json!({
        "schema_version": 1,
        "purpose": "G2P runtime execution, mapping and nonempty-output audit; not linguistic accuracy, whole-script coverage, native-sound or audio validation.",
        "confidence_note": "Uncalibrated provider marker, reported unchanged; a reduced value does not repair or validate a reading.",
        "voice_listing_note": "Listing membership is informative only: official aliases such as en/zh may execute en-gb/cmn voices.",
        "binary": {"requested_path": binary.map(|p| p.to_string_lossy()), "discovery": if binary.is_some() {"explicit"} else {"PATH"}, "installed_voice_count": voices.len(), "inventory_error": inventory_error},
        "skipped": false, "planned_locales": SpeechLocale::ALL.len(), "probed_locales": results.len(),
        "successful_locales": successful_locales,
        "success": inventory_error.is_none() && successful_locales == SpeechLocale::ALL.len(),
        "locales": results,
    })
}

fn main() -> ExitCode {
    let options = match options() {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}\n{HELP}");
            return ExitCode::from(2);
        }
    };
    if options.help {
        println!("{HELP}");
        return ExitCode::SUCCESS;
    }
    let provider = if let Some(binary) = &options.binary {
        Ok(EspeakNgG2PProvider::new().with_binary(binary))
    } else {
        EspeakNgG2PProvider::auto_detect()
    };
    let report = match provider {
        Ok(provider) => audit(&provider, options.binary.as_deref()),
        Err(error) => {
            let report = json!({"schema_version":1, "skipped":true, "success":false,
                "planned_locales":SpeechLocale::ALL.len(), "probed_locales":0, "successful_locales":0,
                "locales":[], "error":error.to_string()});
            if options.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).expect("serializable audit")
                );
            } else {
                println!("espeak-ng unavailable ({error}); optional runtime audit skipped.");
            }
            return ExitCode::SUCCESS;
        }
    };
    if options.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("serializable audit")
        );
    } else {
        println!(
            "Runtime probes passed: {}/20. Pronunciation accuracy is not evaluated.",
            report["successful_locales"]
        );
        for row in report["locales"].as_array().expect("locale rows") {
            println!(
                "{} success={} voice={} ipa={} provider_confidence={} error={}",
                row["locale"],
                row["success"],
                row["base"]["actual_voice"],
                row["base"]["ipa"],
                row["base"]["provider_confidence"],
                row["error"]
            );
        }
    }
    if passed(&report) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
