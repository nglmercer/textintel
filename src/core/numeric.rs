//! Source-offset scanning shared by numeric entity consumers.
use super::unicode::ascii_decimal_digit;

/// End of a nonempty unsigned decimal-shaped run at a UTF-8 boundary.
/// Separator punctuation is included only between decimal digits. This scans
/// a candidate; callers still choose interpretation, grouping and word policy.
pub(crate) fn scan_unsigned_decimal(text: &str, start: usize) -> Option<usize> {
    let tail = text.get(start..)?;
    let mut chars = tail.char_indices().peekable();
    let (_, first) = chars.next()?;
    ascii_decimal_digit(first)?;
    let mut end = start + first.len_utf8();
    while let Some((offset, ch)) = chars.next() {
        if ascii_decimal_digit(ch).is_some()
            || (matches!(ch, '.' | ',' | '٫' | '٬')
                && chars
                    .peek()
                    .is_some_and(|(_, ch)| ascii_decimal_digit(*ch).is_some()))
        {
            end = start + offset + ch.len_utf8();
        } else {
            break;
        }
    }
    Some(end)
}
