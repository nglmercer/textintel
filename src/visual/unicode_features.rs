use crate::core::types::UnicodeFeatures;
use crate::normalization::unicode::{casefold_text, nfc, nfkc};
use crate::normalization::whitespace::is_extra_whitespace;
use crate::visual::homoglyph::{confusable_characters, confusable_skeleton};
use crate::visual::scripts::scripts_in;

const INVISIBLE: &[char] = &[
    '\u{00ad}', '\u{061c}', '\u{180e}', '\u{200b}', '\u{200c}', '\u{200d}', '\u{200e}', '\u{200f}',
    '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}', '\u{202e}', '\u{2060}', '\u{2066}', '\u{2067}',
    '\u{2068}', '\u{2069}', '\u{feff}',
];

const BIDI_CONTROLS: &[char] = &[
    '\u{061c}', '\u{200e}', '\u{200f}', '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}', '\u{202e}',
    '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}',
];

fn is_invisible(ch: char) -> bool {
    let code = ch as u32;
    INVISIBLE.contains(&ch)
        || ch.is_control()
        || matches!(
            code,
            0x0600..=0x0605
                | 0x06dd
                | 0x070f
                | 0x0890..=0x0891
                | 0x180e
                | 0x200b..=0x200f
                | 0x2028..=0x202e
                | 0x2060..=0x206f
                | 0xfeff
                | 0xfff9..=0xfffb
        )
}

pub fn analyze_unicode(text: &str) -> UnicodeFeatures {
    let scripts = scripts_in(text);
    let confusables = confusable_characters(text);
    let invisible_characters = text
        .chars()
        .filter(|ch| is_invisible(*ch) && !matches!(*ch, '\n' | '\r' | '\t'))
        .map(|ch| ch.to_string())
        .collect::<Vec<_>>();
    let unusual_whitespace = text
        .chars()
        .filter(|ch| is_extra_whitespace(*ch))
        .map(|ch| ch.to_string())
        .collect::<Vec<_>>();
    let combining_characters = text
        .chars()
        .filter(|ch| {
            let code = *ch as u32;
            (0x0300..=0x036f).contains(&code)
                || (0x1ab0..=0x1aff).contains(&code)
                || (0x1dc0..=0x1dff).contains(&code)
                || (0x20d0..=0x20ff).contains(&code)
                || (0xfe20..=0xfe2f).contains(&code)
        })
        .map(|ch| ch.to_string())
        .collect::<Vec<_>>();
    let bidirectional_controls = text
        .chars()
        .filter(|ch| BIDI_CONTROLS.contains(ch))
        .map(|ch| ch.to_string())
        .collect::<Vec<_>>();

    let mixed_scripts = scripts.len() > 1;
    let mut suspicious = 0.0;
    if mixed_scripts {
        suspicious += 0.4;
    }
    suspicious += (confusables.len() as f64 * 0.15).min(0.5);
    suspicious += (invisible_characters.len() as f64 * 0.1).min(0.3);
    suspicious += (unusual_whitespace.len() as f64 * 0.05).min(0.2);
    suspicious += (bidirectional_controls.len() as f64 * 0.25).min(0.5);
    if !combining_characters.is_empty() {
        suspicious += 0.05;
    }

    UnicodeFeatures {
        scripts,
        mixed_scripts,
        invisible_characters,
        unusual_whitespace,
        combining_characters,
        bidirectional_controls,
        confusable_characters: confusables,
        confusable_skeleton: Some(confusable_skeleton(text)),
        suspicious_unicode_score: suspicious.min(1.0),
        nfc: nfc(text),
        nfkc: nfkc(text),
        casefolded: casefold_text(text),
    }
}
