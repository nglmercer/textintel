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

#[cfg(unix)]
#[test]
fn stub_binary_drives_multilingual_mechanics() {
    use std::io::Write;
    let path = std::env::temp_dir().join("textintel-stub-espeak.sh");
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
