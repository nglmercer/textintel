//! espeak-ng integration mechanics (§37, §88). Requires the
//! `phonetic-espeak` feature; the live binary is optional — detection and a
//! stub binary cover both paths without network or model downloads.
#![cfg(feature = "phonetic-espeak")]

use textintel::core::providers::G2PProvider;
use textintel::phonetic::{EspeakNgG2PProvider, EspeakVoice, parse_espeak_ipa, parse_voices_table};

// Explicit expected voices: resource presence or a script alone must not
// accidentally select English for a known requested language family.
const REGIONAL_VOICES: &[(&str, &str)] = &[
    ("en-GB", "en"),
    ("es-MX", "es"),
    ("zh-Hans", "zh"),
    ("hi-IN", "hi"),
    ("ar-EG", "ar"),
    ("fr-CA", "fr"),
    ("pt-BR", "pt"),
    ("ru-RU", "ru"),
    ("bn-BD", "bn"),
    ("ur-PK", "ur"),
    ("id-ID", "id"),
    ("de-DE", "de"),
    ("ja-JP", "ja"),
    ("pa-Guru", "pa"),
    ("mr-IN", "mr"),
    ("te-IN", "te"),
    ("tr-TR", "tr"),
    ("ta-IN", "ta"),
    ("vi-VN", "vi"),
    ("ko-KR", "ko"),
    ("es-419", "es"),
    ("en-Latn-US", "en"),
    ("yue-Hant-HK", "yue"),
];

#[test]
fn detection_agrees_with_itself() {
    // No PATH mutation: both paths must give the same answer.
    assert_eq!(
        EspeakNgG2PProvider::is_available(),
        EspeakNgG2PProvider::auto_detect().is_ok()
    );
}

#[test]
fn missing_binary_is_an_explicit_error() {
    let provider = EspeakNgG2PProvider::new().with_binary("/nonexistent-espeak-ng-xyz");
    assert!(provider.phonemize("hola", "es").is_err());
}

#[test]
fn unsupported_languages_are_discounted_not_silent() {
    assert!(EspeakNgG2PProvider::is_supported_language("es"));
    assert!(EspeakNgG2PProvider::is_supported_language("ES"));
    assert!(!EspeakNgG2PProvider::is_supported_language("xx-unknown"));
}

#[test]
fn regional_tags_select_curated_base_voices_without_claiming_regional_coverage() {
    let provider = EspeakNgG2PProvider::new();
    for &(tag, voice) in REGIONAL_VOICES {
        for requested in [tag.to_string(), tag.to_uppercase().replace('-', "_")] {
            assert_eq!(provider.voice_for(&requested), voice, "{requested}");
            assert!(EspeakNgG2PProvider::is_supported_language(&requested));
        }
    }
    assert_eq!(provider.voice_for("  Pt_bR  "), "pt");
    for unknown in ["xx-MX", "unknown-es", "_es", ""] {
        assert_eq!(provider.voice_for(unknown), "en");
        assert!(!EspeakNgG2PProvider::is_supported_language(unknown));
    }
    assert_eq!(
        provider.capabilities().languages,
        EspeakNgG2PProvider::supported_languages()
    );
    assert!(!provider.capabilities().languages.contains(&"es-MX".into()));
}

#[test]
fn forced_voice_bounds_instance_capabilities_without_changing_static_catalog() {
    for (voice, languages) in [
        ("es-419", vec!["es"]),
        ("EN_us", vec!["en"]),
        ("pt-BR", vec!["pt"]),
        ("custom-voice", vec![]),
    ] {
        let provider = EspeakNgG2PProvider::new().for_voice(voice);
        for locale in textintel::speech::SpeechLocale::ALL {
            assert_eq!(provider.voice_for(locale.code()), voice);
        }
        assert_eq!(provider.voice_for("xx-unknown"), voice);
        assert_eq!(provider.capabilities().languages, languages);
    }
    let default = EspeakNgG2PProvider::new();
    let forced = default.clone().for_voice("es-419");
    assert_eq!(default.voice_for("fr"), "fr");
    assert_eq!(forced.voice_for("fr"), "es-419");
    assert_eq!(
        default.capabilities().languages,
        EspeakNgG2PProvider::supported_languages()
    );
    assert!(EspeakNgG2PProvider::is_supported_language("fr-CA"));
    assert_eq!(forced.for_voice("pt-BR").voice_for("es-MX"), "pt-BR");
}

#[test]
fn real_voice_table_layout_keeps_gender_name_file_and_alias_columns_separate() {
    // Observed with eSpeak NG 1.52.0; its PrintVoices implementation formats
    // age/gender together and replaces spaces in voice names with underscores:
    // https://github.com/espeak-ng/espeak-ng/blob/4870adfa25b1a32b4361592f1be8a40337c58d6c/src/espeak-ng.c#L165-L195
    let table = "Pty Language       Age/Gender VoiceName          File                 Other Languages\n\
 5  af              --/M      Afrikaans          gmw/af\n\
 2  en-gb           --/M      English_(Great_Britain) gmw/en           (en 2)\n\
 5  cmn             --/M      Chinese_(Mandarin,_latin_as_English) sit/cmn (zh-cmn 5)(zh 5)\n\
 5  ja              --/M      Japanese           jpx/ja\n";
    let voices = parse_voices_table(table);
    assert_eq!(voices.len(), 4);
    for (actual, (language, name, file)) in voices.iter().zip([
        ("af", "Afrikaans", "gmw/af"),
        ("en-gb", "English_(Great_Britain)", "gmw/en"),
        ("cmn", "Chinese_(Mandarin,_latin_as_English)", "sit/cmn"),
        ("ja", "Japanese", "jpx/ja"),
    ]) {
        assert_eq!(
            actual,
            &EspeakVoice {
                language: language.into(),
                name: name.into(),
                gender: "M".into(),
                file: file.into(),
            }
        );
    }
    let compatible = parse_voices_table(
        "5 test 7/F Child test/child\n5 neutral --/- Neutral test/neutral\n5 es -- M spanish es (es-MX 5)\n",
    );
    assert_eq!(compatible.len(), 3);
    assert_eq!(compatible[0].gender, "F");
    assert_eq!(compatible[1].gender, "-");
    assert_eq!(compatible[2].file, "es");
    for malformed in [
        "5 es --/M spanish",
        "bad es --/M spanish es",
        "5 es old/M spanish es",
        "5 es --/X spanish es",
        "5 es --/M/F spanish es",
        "5 es -- M spanish",
        "5 es old M spanish es",
        "5 es -- X spanish es",
    ] {
        assert!(parse_voices_table(malformed).is_empty(), "{malformed}");
    }
}

#[test]
fn espeak_control_annotations_are_removed_without_folding_generic_ipa_parentheses() {
    let raw = "(\u{200d}e\u{200d}n)t\u{200d}ʃˈa\u{200d}ɪniːz(\u{200d}j\u{200d}a)lˈe̞tə";
    let (ipa, phonemes, syllables) = parse_espeak_ipa(raw);
    assert_eq!(ipa, "tʃˈaɪniːz lˈe̞tə");
    assert_eq!(phonemes, ["tʃ", "aɪ", "n", "i", "z", "l", "e", "t", "ə"]);
    assert_eq!(syllables, 4);
    assert_eq!(
        textintel::phonetic::primary_stress_syllables(raw),
        vec![0, 2]
    );
    assert!(
        phonemes
            .iter()
            .all(|token| !matches!(token.as_str(), "(" | ")" | "\u{200d}"))
    );
    for preserved in [
        "(ə)",
        "(tʃ)",
        "(unknown)",
        "(en-US)",
        "(en",
        "((en))",
        "(ə (en))",
    ] {
        assert_eq!(parse_espeak_ipa(preserved).0, preserved);
    }
    assert_eq!(parse_espeak_ipa("(en-us)hˈəloʊ(en)").0, "hˈəloʊ");
    assert_eq!(parse_espeak_ipa("(en)(ja)\n").1.len(), 0);
    // Generic IPA remains loss-preserving; it does not interpret tool metadata.
    assert!(textintel::phonetic::parse_ipa("(en)").contains(&"(".to_string()));
    // A long malformed group is retained, with bounded control-name lookup.
    let malformed = format!("({}", "ə".repeat(8192));
    assert_eq!(parse_espeak_ipa(&malformed).0, malformed);
}

#[cfg(unix)]
#[test]
fn language_switches_and_empty_ipa_receive_honest_confidence() {
    // Mock output checks control decoding and metadata contracts; it cannot
    // establish that eSpeak correctly reads arbitrary Japanese or other text.
    for (tag, output, expected_ipa, confidence, syllables) in [
        (
            "ja-JP",
            "(\u{200d}e\u{200d}n)olˈa(\u{200d}j\u{200d}a)",
            "olˈa",
            0.5,
            2,
        ),
        ("es-MX", "(es-la)olˈa(es)", "olˈa", 0.8, 2),
        ("zh-Hans", "(cmn)olˈa", "olˈa", 0.8, 2),
        ("en-US", "(en-us)olˈa(en)", "olˈa", 0.8, 2),
        ("es", "", "", 0.0, 0),
        ("ja", "(en)(ja)", "", 0.0, 0),
    ] {
        let path = stub_binary("switch", output);
        let provider = EspeakNgG2PProvider::new().with_binary(&path);
        let source = "静かな庭; $(printf injected) 🙂";
        let candidate = retry_on_text_file_busy(|| provider.phonemize(source, tag)).unwrap();
        assert_eq!(candidate.source, source);
        assert_eq!(candidate.language, tag);
        assert_eq!(candidate.ipa.as_deref(), Some(expected_ipa));
        assert_eq!(candidate.confidence, confidence, "{tag}: {output}");
        assert_eq!(candidate.syllables, syllables);
        assert_eq!(candidate.stress, (syllables > 0).then_some(vec![1]));
        assert_eq!(
            candidate.phonemes,
            textintel::phonetic::parse_ipa(expected_ipa)
        );
        std::fs::remove_file(path).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn regional_routing_reaches_subprocess_argv_and_preserves_requested_tag() {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    let path =
        std::env::temp_dir().join(format!("textintel-espeak-routing-{}", unique_tag("argv")));
    let log = path.with_extension("argv");
    {
        let mut file = std::fs::File::create(&path).unwrap();
        // Per-binary log avoids process-global environment mutation. Canned IPA
        // tests subprocess routing and parsing, not native pronunciation quality.
        writeln!(file, "#!/bin/sh").unwrap();
        writeln!(file, "printf '%s\\0' \"$@\" > \"$0.argv\"").unwrap();
        writeln!(file, "printf 'olˈa\\n'").unwrap();
        file.sync_all().unwrap();
    }
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let provider = EspeakNgG2PProvider::new().with_binary(&path);
    let text = "hola; $(printf injected)\n🙂";
    let requests = REGIONAL_VOICES
        .iter()
        .flat_map(|&(tag, voice)| {
            [tag.to_string(), tag.to_uppercase().replace('-', "_")].map(|tag| (tag, voice, 0.8))
        })
        .chain([
            (" EN_latn_US ".into(), "en", 0.8),
            ("xx-MX".into(), "en", 0.5),
        ]);
    for (requested, voice, confidence) in requests {
        let candidate = retry_on_text_file_busy(|| provider.phonemize(text, &requested)).unwrap();
        assert_eq!(candidate.source, text);
        assert_eq!(candidate.language, requested);
        assert_eq!(candidate.dialect.as_deref(), Some(voice));
        assert_eq!(candidate.confidence, confidence);
        let bytes = std::fs::read(&log).unwrap();
        let arguments: Vec<&[u8]> = bytes
            .split(|byte| *byte == 0)
            .filter(|arg| !arg.is_empty())
            .collect();
        assert_eq!(
            arguments,
            [
                b"-q".as_slice(),
                b"--ipa=3",
                b"-v",
                voice.as_bytes(),
                b"--",
                text.as_bytes()
            ]
        );
    }
    for forced_voice in ["es-419", "unknown-custom-voice"] {
        let forced = provider.clone().for_voice(forced_voice);
        for requested in ["es-MX", "ES_mx", "fr-CA", "en-US", "xx-MX"] {
            let candidate = retry_on_text_file_busy(|| forced.phonemize(text, requested)).unwrap();
            assert_eq!(candidate.language, requested);
            assert_eq!(candidate.source, text);
            assert_eq!(candidate.dialect.as_deref(), Some(forced_voice));
            let matching_spanish =
                forced_voice == "es-419" && matches!(requested, "es-MX" | "ES_mx");
            assert_eq!(
                candidate.confidence,
                if matching_spanish { 0.8 } else { 0.5 }
            );
            assert_eq!(
                std::fs::read(&log).unwrap(),
                format!("-q\0--ipa=3\0-v\0{forced_voice}\0--\0{text}\0").as_bytes()
            );
        }
        assert_eq!(
            retry_on_text_file_busy(|| forced.phonemize("", "es-MX"))
                .unwrap()
                .confidence,
            0.0
        );
    }
    for text in [
        "--help",
        "-12",
        "-vfr",
        "--",
        "",
        "-vfr; $(printf injected)\n🙂",
    ] {
        let candidate = retry_on_text_file_busy(|| provider.phonemize(text, "en-US")).unwrap();
        assert_eq!(candidate.source, text);
        assert_eq!(candidate.language, "en-US");
        assert_eq!(candidate.dialect.as_deref(), Some("en"));
        assert_eq!(
            std::fs::read(&log).unwrap(),
            format!("-q\0--ipa=3\0-v\0en\0--\0{text}\0").as_bytes()
        );
    }
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(log).unwrap();
}

#[cfg(unix)]
#[test]
fn forced_matching_voice_still_discounts_foreign_switches_and_empty_output() {
    for (output, confidence) in [("(en)olˈa(es)", 0.5), ("(es-la)olˈa(es)", 0.8), ("", 0.0)] {
        let path = stub_binary("forced-switch", output);
        let provider = EspeakNgG2PProvider::new()
            .with_binary(&path)
            .for_voice("es-419");
        let candidate = retry_on_text_file_busy(|| provider.phonemize("hola", "es-MX")).unwrap();
        assert_eq!(candidate.dialect.as_deref(), Some("es-419"));
        assert_eq!(candidate.confidence, confidence);
        std::fs::remove_file(path).unwrap();
    }
}

#[cfg(unix)]
fn unique_tag(tag: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}-{}-{:?}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst),
        tag,
        std::thread::current().id()
    )
}

#[cfg(unix)]
fn retry_on_text_file_busy<T>(
    mut attempt: impl FnMut() -> Result<T, textintel::core::error::ProviderError>,
) -> Result<T, textintel::core::error::ProviderError> {
    // The kernel can report ETXTBSY when a freshly written stub is executed
    // before its close has fully propagated; retry briefly, then surface the
    // last error so genuine failures still fail loudly.
    let mut last_error = None;
    for _ in 0..20 {
        match attempt() {
            Ok(value) => return Ok(value),
            Err(error) if error.to_string().contains("Text file busy") => {
                last_error = Some(error);
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.expect("retry loop always attempts at least once"))
}

#[cfg(unix)]
fn stub_binary(tag: &str, output: &str) -> std::path::PathBuf {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!("textintel-stub-espeak-{}", unique_tag(tag)));
    let _ = std::fs::remove_file(&path);
    {
        let mut file = std::fs::File::create(&path).expect("stub write");
        writeln!(file, "#!/bin/sh").expect("stub write");
        writeln!(file, "printf '%s\\n' '{output}'").expect("stub write");
        file.sync_all().expect("stub sync");
    }
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&path).expect("stub meta").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("stub chmod");
    }
    path
}

#[cfg(unix)]
#[test]
fn stub_binary_reports_stress_articulatory_and_fallback_confidence() {
    let path = stub_binary("stress", "olˈa");
    let supported = EspeakNgG2PProvider::new()
        .with_binary(&path)
        .for_voice("es");
    let candidate =
        retry_on_text_file_busy(|| supported.phonemize("hola", "es")).expect("stub phonemize");
    assert_eq!(candidate.confidence, 0.8);
    assert_eq!(candidate.stress, Some(vec![1]));
    assert_eq!(
        candidate.articulatory_features.len(),
        candidate.phonemes.len()
    );
    assert!(!candidate.articulatory_features.is_empty());

    // An unmapped request keeps the forced Spanish voice at reduced confidence.
    let fallback = retry_on_text_file_busy(|| supported.phonemize("hola", "xx-unknown"))
        .expect("stub phonemize");
    assert_eq!(fallback.confidence, 0.5);
    assert_eq!(fallback.dialect.as_deref(), Some("es"));
    let _ = std::fs::remove_file(&path);
}

#[cfg(unix)]
#[test]
fn subprocess_timeout_kills_hung_binaries() {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!("textintel-sleep-espeak-{}", unique_tag("sleep")));
    let _ = std::fs::remove_file(&path);
    {
        let mut file = std::fs::File::create(&path).expect("stub write");
        writeln!(file, "#!/bin/sh").expect("stub write");
        writeln!(file, "sleep 30").expect("stub write");
        file.sync_all().expect("stub sync");
    }
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&path).expect("stub meta").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("stub chmod");
    }
    let provider = EspeakNgG2PProvider::new()
        .with_binary(&path)
        .with_timeout(std::time::Duration::from_millis(200));
    let error =
        retry_on_text_file_busy(|| provider.phonemize("hola", "es")).expect_err("must time out");
    assert!(
        error.to_string().contains("timed out"),
        "unexpected: {error}"
    );
    let _ = std::fs::remove_file(&path);
}

#[cfg(unix)]
#[test]
fn installed_voices_come_from_the_binary_table() {
    use std::io::Write;
    let path =
        std::env::temp_dir().join(format!("textintel-voices-espeak-{}", unique_tag("voices")));
    let _ = std::fs::remove_file(&path);
    {
        let mut file = std::fs::File::create(&path).expect("stub write");
        writeln!(file, "#!/bin/sh").expect("stub write");
        writeln!(
            file,
            "printf 'Pty Language Age/Gender VoiceName File Other Languages\\n5 es --/M Spanish roa/es (es-ES 5)\\n5 ja --/M Japanese jpx/ja\\n'"
        )
        .expect("stub write");
        file.sync_all().expect("stub sync");
    }
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&path).expect("stub meta").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("stub chmod");
    }
    let provider = EspeakNgG2PProvider::new().with_binary(&path);
    let voices = retry_on_text_file_busy(|| provider.installed_voices()).expect("voices");
    assert_eq!(voices.len(), 2);
    assert_eq!(voices[0].language, "es");
    assert_eq!(voices[0].name, "Spanish");
    assert_eq!(voices[0].gender, "M");
    assert_eq!(voices[0].file, "roa/es");
    assert_eq!(voices[1].language, "ja");
    let _ = std::fs::remove_file(&path);
}

#[cfg(unix)]
#[test]
fn stub_binary_drives_multilingual_mechanics() {
    use std::io::Write;
    let path =
        std::env::temp_dir().join(format!("textintel-stub-espeak-{}.sh", unique_tag("mech")));
    {
        let mut file = std::fs::File::create(&path).expect("stub write");
        writeln!(file, "#!/bin/sh").expect("stub write");
        writeln!(file, "printf 'olˈa\\n'").expect("stub write");
        file.sync_all().expect("stub sync");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&path).expect("stub meta").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("stub chmod");
    }
    let provider = EspeakNgG2PProvider::new()
        .with_binary(&path)
        .for_voice("es");
    let candidate =
        retry_on_text_file_busy(|| provider.phonemize("hola", "es")).expect("stub phonemize");
    assert_eq!(candidate.ipa.as_deref(), Some("olˈa"));
    assert_eq!(candidate.phonemes, textintel::phonetic::parse_ipa("olˈa"));
    assert_eq!(candidate.syllables, 2);
    assert_eq!(candidate.dialect.as_deref(), Some("es"));
    assert_eq!(candidate.language, "es");
    let _ = std::fs::remove_file(&path);
}
