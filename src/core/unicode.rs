//! Unicode character predicates shared by analysis and speech preparation.

/// Fold a decimal digit from the supported script families to ASCII.
/// This is digit-value conversion, not compatibility/leet normalization;
/// superscripts, Roman numerals, and circled numbers are not decimal digits.
pub fn ascii_decimal_digit(character: char) -> Option<char> {
    let value = character as u32;
    const ZEROES: &[u32] = &[
        0x30, 0x660, 0x6f0, 0x7c0, 0x966, 0x9e6, 0xa66, 0xae6, 0xb66, 0xbe6, 0xc66, 0xce6, 0xd66,
        0xe50, 0xed0, 0xf20, 0x1040, 0x1090, 0x17e0, 0x1810, 0xff10, 0x1d7ce, 0x1d7d8, 0x1d7e2,
        0x1d7ec, 0x1d7f6,
    ];
    ZEROES.iter().find_map(|zero| {
        (value >= *zero && value < *zero + 10).then(|| char::from(b'0' + (value - *zero) as u8))
    })
}

/// CJK script ranges that can surround a written numeric expression without
/// whitespace. This predicate does not define general word segmentation.
pub fn is_cjk(character: char) -> bool {
    matches!(character as u32, 0x1100..=0x11ff | 0x3040..=0x30ff | 0x3130..=0x318f
        | 0x3400..=0x9fff | 0xac00..=0xd7af | 0xff66..=0xff9d | 0x20000..=0x323af)
}

/// Unicode `General_Category=Mark`, including Indic vowel/virama signs,
/// Arabic marks, combining accents, and enclosing/spacing marks.
pub fn is_combining_mark(character: char) -> bool {
    unicode_normalization::char::is_combining_mark(character)
}

/// Generic combining-diacritic blocks used by accent folding and visual
/// diagnostics. Script-specific marks carry spelling information and are
/// intentionally outside this narrower presentation policy.
pub fn is_generic_diacritic(character: char) -> bool {
    matches!(character,
        '\u{0300}'..='\u{036f}'
        | '\u{1ab0}'..='\u{1aff}'
        | '\u{1dc0}'..='\u{1dff}'
        | '\u{20d0}'..='\u{20ff}'
        | '\u{fe20}'..='\u{fe2f}'
    )
}

/// Letters and their combining marks form one word across writing systems.
pub fn is_word_letter(character: char) -> bool {
    character.is_alphabetic() || is_combining_mark(character)
}
