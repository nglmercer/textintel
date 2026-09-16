//! Email entity scanning (`local@domain.tld` spans).

use crate::core::types::EntityMention;

use super::normalize::{casefold, claim, is_claimed, push_mention, trim_trailing_punctuation};

fn email_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '%' | '+' | '-')
}

pub(crate) fn scan_emails(
    text: &str,
    claimed: &mut [bool],
    mentions: &mut Vec<EntityMention>,
    language: Option<&str>,
) {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'@' || claimed[index] {
            index += 1;
            continue;
        }
        let mut local_start = index;
        while local_start > 0
            && email_char(bytes[local_start - 1] as char)
            && !claimed[local_start - 1]
        {
            local_start -= 1;
        }
        let mut domain_end = index + 1;
        while domain_end < bytes.len()
            && (email_char(bytes[domain_end] as char))
            && !claimed[domain_end]
        {
            domain_end += 1;
        }
        index += 1;
        if local_start >= index - 1 || domain_end <= index + 1 {
            continue;
        }
        // `claimed` is byte-aligned with ASCII-only scans here; both bounds
        // sit on ASCII bytes, hence on char boundaries.
        if !text.is_char_boundary(local_start) || !text.is_char_boundary(domain_end) {
            continue;
        }
        let candidate = trim_trailing_punctuation(&text[local_start..domain_end]);
        if candidate.is_empty() {
            continue;
        }
        let end = local_start + candidate.len();
        let Some(at) = candidate.find('@') else {
            continue;
        };
        let domain = &candidate[at + 1..];
        let Some(dot) = domain.rfind('.') else {
            continue;
        };
        let tld = &domain[dot + 1..];
        if tld.len() < 2 || !tld.chars().all(|ch| ch.is_ascii_alphabetic()) {
            continue;
        }
        if candidate[..at].is_empty() {
            continue;
        }
        if is_claimed(claimed, local_start, end) {
            continue;
        }
        claim(claimed, local_start, end);
        push_mention(
            mentions,
            "email",
            casefold(candidate),
            local_start,
            end,
            0.95,
            language,
        );
    }
}
