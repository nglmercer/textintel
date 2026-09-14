use unicode_segmentation::UnicodeSegmentation;

use crate::core::types::UnicodeFeatures;
use crate::normalization::unicode::{casefold_text, nfc, nfkc};
use crate::normalization::whitespace::is_extra_whitespace;
use crate::visual::homoglyph::{confusable_characters, confusable_skeleton};
use crate::visual::scripts::{script_extensions_for, scripts_in};

const INVISIBLE: &[char] = &[
    '\u{00ad}', '\u{061c}', '\u{180e}', '\u{200b}', '\u{200c}', '\u{200d}', '\u{200e}', '\u{200f}',
    '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}', '\u{202e}', '\u{2060}', '\u{2066}', '\u{2067}',
    '\u{2068}', '\u{2069}', '\u{feff}',
];

const BIDI_CONTROLS: &[char] = &[
    '\u{061c}', '\u{200e}', '\u{200f}', '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}', '\u{202e}',
    '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}',
];

fn is_variation_selector(ch: char) -> bool {
    matches!(ch as u32, 0xfe00..=0xfe0f | 0xe0100..=0xe01ef)
}

fn is_joiner(ch: char) -> bool {
    matches!(ch, '\u{200c}' | '\u{200d}')
}

fn is_full_width(ch: char) -> bool {
    matches!(ch as u32, 0xff01..=0xff60 | 0xffe0..=0xffee)
}

fn is_combining_mark(ch: char) -> bool {
    let code = ch as u32;
    (0x0300..=0x036f).contains(&code)
        || (0x1ab0..=0x1aff).contains(&code)
        || (0x1dc0..=0x1dff).contains(&code)
        || (0x20d0..=0x20ff).contains(&code)
        || (0xfe20..=0xfe2f).contains(&code)
}

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
    let script_extensions = text
        .chars()
        .enumerate()
        .filter_map(|(index, ch)| {
            let extensions = script_extensions_for(ch);
            (extensions.len() > 1).then(|| format!("{}:{}", index, extensions.join("+")))
        })
        .collect::<Vec<_>>();
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
        .filter(|ch| is_combining_mark(*ch))
        .map(|ch| ch.to_string())
        .collect::<Vec<_>>();
    let bidirectional_controls = text
        .chars()
        .filter(|ch| BIDI_CONTROLS.contains(ch))
        .map(|ch| ch.to_string())
        .collect::<Vec<_>>();

    let variation_selectors = text
        .chars()
        .filter(|ch| is_variation_selector(*ch))
        .map(|ch| ch.to_string())
        .collect::<Vec<_>>();
    let joiner_characters = text
        .chars()
        .filter(|ch| is_joiner(*ch))
        .map(|ch| ch.to_string())
        .collect::<Vec<_>>();
    let full_width_characters = text
        .chars()
        .filter(|ch| is_full_width(*ch))
        .map(|ch| ch.to_string())
        .collect::<Vec<_>>();
    let malformed_graphemes = text
        .graphemes(true)
        .filter(|grapheme| {
            let chars = grapheme.chars().collect::<Vec<_>>();
            chars.first().is_some_and(|ch| is_combining_mark(*ch))
                || chars.last().is_some_and(|ch| *ch == '\u{200d}')
                || chars
                    .iter()
                    .filter(|ch| is_variation_selector(**ch))
                    .count()
                    > 1
        })
        .map(str::to_string)
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
    suspicious += (variation_selectors.len() as f64 * 0.02).min(0.1);
    suspicious += (joiner_characters.len() as f64 * 0.02).min(0.1);
    suspicious += (full_width_characters.len() as f64 * 0.10).min(0.3);
    suspicious += (script_extensions.len() as f64 * 0.05).min(0.2);
    suspicious += (malformed_graphemes.len() as f64 * 0.20).min(0.5);

    UnicodeFeatures {
        scripts,
        script_extensions,
        mixed_scripts,
        invisible_characters,
        unusual_whitespace,
        combining_characters,
        bidirectional_controls,
        variation_selectors,
        joiner_characters,
        full_width_characters,
        malformed_graphemes,
        confusable_characters: confusables,
        confusable_skeleton: Some(confusable_skeleton(text)),
        suspicious_unicode_score: suspicious.min(1.0),
        nfc: nfc(text),
        nfkc: nfkc(text),
        casefolded: casefold_text(text),
    }
}
