use textintel::core::unicode::{is_combining_mark, is_generic_diacritic};
use textintel::language::segment_message;
use textintel::normalization::unicode::strip_diacritics;
use textintel::visual::unicode_features::analyze_unicode;

#[test]
fn native_combining_marks_keep_indic_words_together() {
    for word in ["शक्ति", "தமிழ்", "తెలుగు", "বাংলা", "ਪ੍ਰੀਤ", "مُحَمَّد"]
    {
        let segments = segment_message(word, 32);
        assert_eq!(segments.len(), 1, "{word}: {segments:?}");
        assert_eq!(segments[0].text, word);
        assert_eq!(segments[0].segment_type, "text");
        assert_eq!(&word[segments[0].start..segments[0].end], word);
    }
    let text = "शक्ति தமிழ் తెలుగు বাংলা";
    assert_eq!(
        textintel::lexical::tokenizer::tokenize(text),
        ["शक्ति", "தமிழ்", "తెలుగు", "বাংলা"]
    );
}

#[test]
fn native_marks_are_reported_without_becoming_a_new_spoof_signal() {
    for (word, mark) in [
        ("शक्ति", '\u{094d}'),
        ("தமிழ்", '\u{0bcd}'),
        ("ਪ੍ਰੀਤ", '\u{0a4d}'),
    ] {
        assert!(is_combining_mark(mark));
        assert!(!is_generic_diacritic(mark));
        let report = analyze_unicode(word);
        assert!(
            report.combining_characters.contains(&mark.to_string()),
            "{report:?}"
        );
        assert!(report.malformed_graphemes.is_empty(), "{word}: {report:?}");
        assert_eq!(report.suspicious_unicode_score, 0.0, "{word}: {report:?}");
    }
}

#[test]
fn isolated_marks_and_generic_accent_policy_remain_distinct() {
    let mark = "\u{094d}";
    assert_eq!(analyze_unicode(mark).malformed_graphemes, vec![mark]);
    assert!(is_generic_diacritic('\u{0301}'));
    assert_eq!(strip_diacritics("café"), "cafe");
    assert_eq!(strip_diacritics("शक्ति தமிழ்"), "शक्ति தமிழ்");
    assert!(analyze_unicode("e\u{0301}").suspicious_unicode_score >= 0.05);
}
