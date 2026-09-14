//! espeak-ng integration mechanics (§37, §88). Requires the
//! `phonetic-espeak` feature; the live binary is optional — detection and a
//! stub binary cover both paths without network or model downloads.
#![cfg(feature = "phonetic-espeak")]

use textintel::core::providers::G2PProvider;
use textintel::phonetic::EspeakNgG2PProvider;

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
fn stub_binary(tag: &str, output: &str) -> std::path::PathBuf {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!("textintel-stub-espeak-{}", unique_tag(tag)));
    let _ = std::fs::remove_file(&path);
    {
        let mut file = std::fs::File::create(&path).expect("stub write");
        writeln!(file, "#!/bin/sh").expect("stub write");
        writeln!(file, "printf '%s\\n' '{output}'").expect("stub write");
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
    let candidate = supported.phonemize("hola", "es").expect("stub phonemize");
    assert_eq!(candidate.confidence, 0.8);
    assert_eq!(candidate.stress, Some(vec![1]));
    assert_eq!(
        candidate.articulatory_features.len(),
        candidate.phonemes.len()
    );
    assert!(!candidate.articulatory_features.is_empty());

    // Unmapped language falls back to the default voice at half confidence.
    let fallback = supported
        .phonemize("hola", "xx-unknown")
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
    let error = provider.phonemize("hola", "es").expect_err("must time out");
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
            "printf 'Pty Language Age Gender VoiceName File\\n5  es -- M spanish es\\n'"
        )
        .expect("stub write");
    }
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&path).expect("stub meta").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("stub chmod");
    }
    let provider = EspeakNgG2PProvider::new().with_binary(&path);
    let voices = provider.installed_voices().expect("voices");
    assert_eq!(voices.len(), 1);
    assert_eq!(voices[0].language, "es");
    assert_eq!(voices[0].name, "spanish");
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
    let candidate = provider.phonemize("hola", "es").expect("stub phonemize");
    assert_eq!(candidate.ipa.as_deref(), Some("olˈa"));
    assert_eq!(candidate.phonemes, textintel::phonetic::parse_ipa("olˈa"));
    assert_eq!(candidate.syllables, 2);
    assert_eq!(candidate.dialect.as_deref(), Some("es"));
    assert_eq!(candidate.language, "es");
    let _ = std::fs::remove_file(&path);
}
