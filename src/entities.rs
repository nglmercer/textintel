//! Bounded rule-based entity evidence (`Basic` fallback).
//!
//! [`RuleBasedEntityProvider`] extracts URLs, emails, mentions, numbers,
//! currency, reliable dates/times, and name-like spans with deterministic
//! character scanners. No models, no network, no lexicon lookups: every
//! mention carries its UTF-8 byte span, a normalized value, a confidence,
//! the provider name, and the ambient language when known.
//!
//! Entity evidence is exposed independently on
//! [`MessageFingerprint`](crate::core::types::MessageFingerprint) and in
//! [`ComparisonResult`](crate::core::types::ComparisonResult) as
//! `entity_agreement` / `entity_conflict`. Missing evidence reads as `0.0`
//! on both, so entity-free pairs never pay a penalty.

use std::sync::Arc;

use crate::core::capabilities::{CapabilityLevel, ProviderCapabilities};
use crate::core::providers::{EntityProvider, LexiconProvider};
use crate::core::types::EntityMention;

/// Default cap on mentions per text.
pub const DEFAULT_MAX_ENTITIES: usize = 16;
/// Default cap on mention span length in characters.
pub const DEFAULT_MAX_ENTITY_SPAN: usize = 64;

/// Deterministic rule-based entity extractor. Bounds are enforced on every
/// call: spans longer than `max_span_chars` are dropped and output is
/// truncated to `max_entities` in offset order.
///
/// A lone capitalized word is name-like only when it is NOT a common word:
/// with a lexicon attached ([`Self::with_lexicon`], the engine default),
/// single title-case words and acronyms found in the lexicon (any language)
/// are sentence capitalization, not names, and are skipped. Multi-word
/// spans keep their heuristic reading.
#[derive(Clone)]
pub struct RuleBasedEntityProvider {
    max_entities: usize,
    max_span_chars: usize,
    lexicon: Option<Arc<dyn LexiconProvider>>,
}

impl std::fmt::Debug for RuleBasedEntityProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RuleBasedEntityProvider")
            .field("max_entities", &self.max_entities)
            .field("max_span_chars", &self.max_span_chars)
            .field("lexicon_attached", &self.lexicon.is_some())
            .finish()
    }
}

impl Default for RuleBasedEntityProvider {
    fn default() -> Self {
        Self {
            max_entities: DEFAULT_MAX_ENTITIES,
            max_span_chars: DEFAULT_MAX_ENTITY_SPAN,
            lexicon: None,
        }
    }
}

impl RuleBasedEntityProvider {
    pub fn new(max_entities: usize, max_span_chars: usize) -> Self {
        Self {
            max_entities: max_entities.max(1),
            max_span_chars: max_span_chars.max(1),
            lexicon: None,
        }
    }

    /// Attach a lexicon so lone capitalized common words are not mistaken
    /// for names (see the type documentation).
    pub fn with_lexicon(mut self, lexicon: Arc<dyn LexiconProvider>) -> Self {
        self.lexicon = Some(lexicon);
        self
    }

    pub fn max_entities(&self) -> usize {
        self.max_entities
    }

    pub fn max_span_chars(&self) -> usize {
        self.max_span_chars
    }

    /// True when `word` is a common word (hence sentence capitalization
    /// rather than a name). Without a lexicon nothing is common. The
    /// diacritic-stripped form counts too (`Dónde` matches `donde`), since
    /// resource word lists are not always accented.
    fn is_common_word(&self, word: &str) -> bool {
        let Some(lexicon) = &self.lexicon else {
            return false;
        };
        let folded = casefold(word);
        let stripped = crate::normalization::unicode::strip_diacritics(&folded);
        [&folded, &stripped]
            .iter()
            .any(|form| lexicon.contains(form, None) || lexicon.is_stop_word(form, None))
    }

    fn bounded(&self, mut mentions: Vec<EntityMention>) -> Vec<EntityMention> {
        mentions.retain(|mention| {
            mention.value.chars().count() <= self.max_span_chars
                && mention.value.chars().count() > 0
        });
        mentions.sort_by(|left, right| {
            left.start
                .cmp(&right.start)
                .then_with(|| left.end.cmp(&right.end))
                .then_with(|| left.entity_type.cmp(&right.entity_type))
        });
        mentions.truncate(self.max_entities);
        mentions
    }
}

fn is_claimed(claimed: &[bool], start: usize, end: usize) -> bool {
    claimed[start..end].iter().any(|flag| *flag)
}

fn claim(claimed: &mut [bool], start: usize, end: usize) {
    for flag in &mut claimed[start..end] {
        *flag = true;
    }
}

fn trim_trailing_punctuation(value: &str) -> &str {
    value.trim_end_matches([
        '.', ',', ';', ':', '!', '?', ')', ']', '}', '\'', '"', '’', '»',
    ])
}

fn casefold(value: &str) -> String {
    crate::normalization::unicode::casefold_text(value)
}

fn push_mention(
    mentions: &mut Vec<EntityMention>,
    entity_type: &str,
    value: String,
    start: usize,
    end: usize,
    confidence: f64,
    language: Option<&str>,
) {
    if value.is_empty() || start >= end {
        return;
    }
    mentions.push(EntityMention {
        entity_type: entity_type.to_string(),
        value,
        start,
        end,
        confidence: confidence.clamp(0.0, 1.0),
        provider: "rule_based_entities".to_string(),
        language: language.map(str::to_string),
    });
}

fn scan_urls(
    text: &str,
    claimed: &mut [bool],
    mentions: &mut Vec<EntityMention>,
    language: Option<&str>,
) {
    for (offset, _) in text.char_indices() {
        if claimed[offset] {
            continue;
        }
        let rest = &text[offset..];
        let lower = rest.to_ascii_lowercase();
        let has_scheme = lower.starts_with("http://") || lower.starts_with("https://");
        let has_www = lower.starts_with("www.") && rest.len() > 4;
        if !has_scheme && !has_www {
            continue;
        }
        let mut end = offset;
        for (index, ch) in rest.char_indices() {
            if ch.is_whitespace() || matches!(ch, '<' | '>' | '"' | '\'' | '`' | '«' | '»') {
                break;
            }
            end = offset + index + ch.len_utf8();
        }
        if end <= offset {
            continue;
        }
        let mut raw = &text[offset..end];
        let trimmed = trim_trailing_punctuation(raw);
        if trimmed.is_empty() {
            continue;
        }
        end = offset + trimmed.len();
        raw = trimmed;
        if has_www && !raw.contains('.') {
            continue;
        }
        if is_claimed(claimed, offset, end) {
            continue;
        }
        claim(claimed, offset, end);
        push_mention(mentions, "url", casefold(raw), offset, end, 0.95, language);
    }
}

fn email_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '%' | '+' | '-')
}

fn scan_emails(
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

fn mention_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.')
}

fn scan_mentions(
    text: &str,
    claimed: &mut [bool],
    mentions: &mut Vec<EntityMention>,
    language: Option<&str>,
) {
    for (offset, ch) in text.char_indices() {
        if ch != '@' || claimed[offset] {
            continue;
        }
        let prev_ok = offset == 0
            || text[..offset].chars().next_back().is_some_and(|prev| {
                prev.is_whitespace() || matches!(prev, '(' | '[' | '{' | '"' | '\'')
            });
        if !prev_ok {
            continue;
        }
        let mut end = offset + 1;
        let mut count = 0;
        for next in text[offset + 1..].chars() {
            if !mention_char(next) || count >= 30 {
                break;
            }
            end += next.len_utf8();
            count += 1;
        }
        if count == 0 {
            continue;
        }
        // Avoid the domain half of a skipped email (`user@host` where the
        // email scan failed validation): a mention needs a boundary before
        // `@`, which the check above already enforces.
        if is_claimed(claimed, offset, end) {
            continue;
        }
        claim(claimed, offset, end);
        push_mention(
            mentions,
            "mention",
            casefold(&text[offset..end]),
            offset,
            end,
            0.7,
            language,
        );
    }
}

const CURRENCY_SYMBOLS: &[(char, &str)] = &[
    ('$', "USD"),
    ('€', "EUR"),
    ('£', "GBP"),
    ('₹', "INR"),
    ('₽', "RUB"),
    ('₩', "KRW"),
    ('¥', "JPY"),
    ('¢', "CENT"),
    ('₴', "UAH"),
    ('₺', "TRY"),
    ('₪', "ILS"),
    ('₫', "VND"),
    ('₱', "PHP"),
    ('฿', "THB"),
];

const CURRENCY_CODES: &[&str] = &[
    "USD", "EUR", "GBP", "JPY", "CNY", "INR", "RUB", "KRW", "CHF", "CAD", "AUD", "BRL", "MXN",
    "ARS", "CLP", "COP", "PEN", "SEK", "NOK", "DKK", "PLN", "CZK", "HUF", "RON", "TRY", "ZAR",
    "NGN", "KES", "GHS", "PHP", "IDR", "MYR", "THB", "VND", "TWD", "HKD", "SGD", "NZD", "ILS",
    "AED", "SAR", "QAR",
];

fn normalize_amount(raw: &str) -> String {
    raw.chars()
        .filter(|ch| ch.is_ascii_digit() || matches!(ch, '.' | ','))
        .collect()
}

fn scan_currency(
    text: &str,
    claimed: &mut [bool],
    mentions: &mut Vec<EntityMention>,
    language: Option<&str>,
) {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut index = 0;
    while index < chars.len() {
        let (offset, ch) = chars[index];
        if claimed[offset] {
            index += 1;
            continue;
        }
        let code = CURRENCY_SYMBOLS.iter().find(|(symbol, _)| *symbol == ch);
        if let Some((_, code)) = code {
            let mut end = offset + ch.len_utf8();
            let mut cursor = index + 1;
            while cursor < chars.len() && chars[cursor].1 == ' ' {
                end += 1;
                cursor += 1;
            }
            let amount_start = cursor;
            let mut digits = 0;
            while cursor < chars.len() {
                let (_, next) = chars[cursor];
                if next.is_ascii_digit() {
                    digits += 1;
                    end += next.len_utf8();
                    cursor += 1;
                } else if matches!(next, '.' | ',')
                    && cursor + 1 < chars.len()
                    && chars[cursor + 1].1.is_ascii_digit()
                {
                    end += next.len_utf8();
                    cursor += 1;
                } else {
                    break;
                }
            }
            if digits > 0 && cursor > amount_start {
                if !is_claimed(claimed, offset, end) {
                    claim(claimed, offset, end);
                    let amount = normalize_amount(&text[offset..end]);
                    push_mention(
                        mentions,
                        "currency",
                        format!("{code} {amount}"),
                        offset,
                        end,
                        0.85,
                        language,
                    );
                }
                index = cursor;
                continue;
            }
        }
        index += 1;
    }
    // Amount-first form (`100 USD`): word scan over ASCII alphanumerics.
    let mut word_start: Option<usize> = None;
    let mut words: Vec<(usize, usize)> = Vec::new();
    for (offset, ch) in text.char_indices() {
        if ch.is_ascii_alphanumeric() {
            if word_start.is_none() {
                word_start = Some(offset);
            }
        } else if let Some(start) = word_start.take() {
            words.push((start, offset));
        }
    }
    if let Some(start) = word_start {
        words.push((start, text.len()));
    }
    for window in words.windows(2) {
        let (amount_start, amount_end) = window[0];
        let (code_start, code_end) = window[1];
        if is_claimed(claimed, amount_start, code_end) {
            continue;
        }
        let amount_raw = &text[amount_start..amount_end];
        let code_raw = &text[code_start..code_end];
        if !amount_raw.chars().any(|ch| ch.is_ascii_digit())
            || !amount_raw
                .chars()
                .all(|ch| ch.is_ascii_digit() || matches!(ch, '.' | ','))
        {
            continue;
        }
        if code_raw.len() == 3
            && code_raw.chars().all(|ch| ch.is_ascii_uppercase())
            && CURRENCY_CODES.contains(&code_raw)
        {
            // The two words must be adjacent modulo one space.
            if text[amount_end..code_start] == *" " {
                claim(claimed, amount_start, code_end);
                push_mention(
                    mentions,
                    "currency",
                    format!("{code_raw} {}", normalize_amount(amount_raw)),
                    amount_start,
                    code_end,
                    0.8,
                    language,
                );
            }
        }
    }
}

fn valid_date(year: u32, month: u32, day: u32) -> bool {
    (1..=12).contains(&month) && (1..=31).contains(&day) && (1000..=2999).contains(&year)
}

fn valid_time(hour: u32, minute: u32, second: Option<u32>) -> bool {
    hour <= 23 && minute <= 59 && second.map_or(true, |value| value <= 59)
}

fn scan_dates_times(
    text: &str,
    claimed: &mut [bool],
    mentions: &mut Vec<EntityMention>,
    language: Option<&str>,
) {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        // ISO date `YYYY-MM-DD` (ASCII digits and dashes: byte offsets are
        // char boundaries).
        if index + 10 <= bytes.len()
            && bytes[index..index + 4].iter().all(|b| b.is_ascii_digit())
            && bytes[index + 4] == b'-'
            && bytes[index + 5..index + 7]
                .iter()
                .all(|b| b.is_ascii_digit())
            && bytes[index + 7] == b'-'
            && bytes[index + 8..index + 10]
                .iter()
                .all(|b| b.is_ascii_digit())
            && text.is_char_boundary(index)
            && text.is_char_boundary(index + 10)
        {
            let year: u32 = text[index..index + 4].parse().unwrap_or(0);
            let month: u32 = text[index + 5..index + 7].parse().unwrap_or(0);
            let day: u32 = text[index + 8..index + 10].parse().unwrap_or(0);
            if valid_date(year, month, day) && !is_claimed(claimed, index, index + 10) {
                claim(claimed, index, index + 10);
                push_mention(
                    mentions,
                    "date",
                    text[index..index + 10].to_string(),
                    index,
                    index + 10,
                    0.8,
                    language,
                );
                index += 10;
                continue;
            }
        }
        // Time `HH:MM` with optional `:SS`.
        if index + 5 <= bytes.len()
            && bytes[index..index + 2].iter().all(|b| b.is_ascii_digit())
            && bytes[index + 2] == b':'
            && bytes[index + 3..index + 5]
                .iter()
                .all(|b| b.is_ascii_digit())
            && text.is_char_boundary(index)
            && text.is_char_boundary(index + 5)
        {
            let hour: u32 = text[index..index + 2].parse().unwrap_or(99);
            let minute: u32 = text[index + 3..index + 5].parse().unwrap_or(99);
            let mut end = index + 5;
            let mut second = None;
            if index + 8 <= bytes.len()
                && bytes[index + 5] == b':'
                && bytes[index + 6..index + 8]
                    .iter()
                    .all(|b| b.is_ascii_digit())
                && text.is_char_boundary(index + 8)
            {
                second = Some(text[index + 6..index + 8].parse().unwrap_or(99));
                end = index + 8;
            }
            if valid_time(hour, minute, second) && !is_claimed(claimed, index, end) {
                claim(claimed, index, end);
                push_mention(
                    mentions,
                    "time",
                    text[index..end].to_string(),
                    index,
                    end,
                    0.7,
                    language,
                );
                index = end;
                continue;
            }
        }
        // Slashed date `D{1,2}/D{1,2}/D{2,4}` (day/month order ambiguous, so
        // lower confidence; still a reliable date-shaped span).
        if bytes[index].is_ascii_digit() {
            let mut cursor = index;
            while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
                cursor += 1;
            }
            let first = &text[index..cursor];
            if cursor < bytes.len() && bytes[cursor] == b'/' {
                let second_start = cursor + 1;
                let mut second_end = second_start;
                while second_end < bytes.len() && bytes[second_end].is_ascii_digit() {
                    second_end += 1;
                }
                if second_end < bytes.len() && bytes[second_end] == b'/' {
                    let third_start = second_end + 1;
                    let mut third_end = third_start;
                    while third_end < bytes.len() && bytes[third_end].is_ascii_digit() {
                        third_end += 1;
                    }
                    let second = &text[second_start..second_end];
                    let third = &text[third_start..third_end];
                    let plausible = (1..=2).contains(&first.len())
                        && (1..=2).contains(&second.len())
                        && (2..=4).contains(&third.len())
                        && first
                            .parse::<u32>()
                            .is_ok_and(|value| (1..=31).contains(&value))
                        && second
                            .parse::<u32>()
                            .is_ok_and(|value| (1..=31).contains(&value));
                    if plausible
                        && text.is_char_boundary(index)
                        && text.is_char_boundary(third_end)
                        && !is_claimed(claimed, index, third_end)
                    {
                        claim(claimed, index, third_end);
                        push_mention(
                            mentions,
                            "date",
                            text[index..third_end].to_string(),
                            index,
                            third_end,
                            0.6,
                            language,
                        );
                        index = third_end;
                        continue;
                    }
                }
            }
        }
        index += 1;
    }
}

fn scan_numbers(
    text: &str,
    claimed: &mut [bool],
    mentions: &mut Vec<EntityMention>,
    language: Option<&str>,
) {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut index = 0;
    while index < chars.len() {
        let (offset, ch) = chars[index];
        if !ch.is_ascii_digit() || claimed[offset] {
            index += 1;
            continue;
        }
        let mut end = offset + ch.len_utf8();
        let mut cursor = index + 1;
        let mut digits = 1;
        while cursor < chars.len() {
            let (_, next) = chars[cursor];
            if next.is_ascii_digit() {
                digits += 1;
                end += next.len_utf8();
                cursor += 1;
            } else if matches!(next, '.' | ',')
                && cursor + 1 < chars.len()
                && chars[cursor + 1].1.is_ascii_digit()
            {
                end += next.len_utf8();
                cursor += 1;
            } else {
                break;
            }
        }
        // Standalone digit runs only: skip digits glued to letters (`abc123`
        // is a token, not a number entity).
        let glued_left = offset > 0
            && text[..offset]
                .chars()
                .next_back()
                .is_some_and(|prev| prev.is_alphanumeric());
        let glued_right = text[end..]
            .chars()
            .next()
            .is_some_and(|next| next.is_alphanumeric() && !next.is_ascii_digit());
        if digits > 0 && !glued_left && !glued_right && !is_claimed(claimed, offset, end) {
            claim(claimed, offset, end);
            push_mention(
                mentions,
                "number",
                normalize_amount(&text[offset..end]),
                offset,
                end,
                0.75,
                language,
            );
        }
        index = cursor.max(index + 1);
    }
}

/// Generic organization suffixes across languages (legal forms and
/// institution words). Generic vocabulary, never dataset content.
const ORG_SUFFIXES: &[&str] = &[
    "inc",
    "llc",
    "ltd",
    "corp",
    "corporation",
    "company",
    "co",
    "group",
    "holdings",
    "partners",
    "gmbh",
    "ag",
    "ug",
    "sa",
    "sl",
    "srl",
    "sas",
    "sarl",
    "spa",
    "bv",
    "nv",
    "ab",
    "oy",
    "pty",
    "bank",
    "banks",
    "university",
    "college",
    "institute",
    "foundation",
    "association",
    "agency",
    "ministry",
    "airlines",
    "airways",
    "railways",
    "studios",
    "records",
    "press",
    "media",
];

fn is_title_word(word: &str) -> bool {
    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_uppercase() {
        return false;
    }
    let rest: String = chars.collect();
    rest.chars().count() >= 1
        && rest
            .chars()
            .all(|ch| ch.is_lowercase() || matches!(ch, '\'' | '-' | '.'))
        && word.chars().any(|ch| ch.is_alphabetic())
}

fn is_acronym(word: &str) -> bool {
    let count = word.chars().count();
    (2..=6).contains(&count)
        && word.chars().all(|ch| ch.is_uppercase())
        && word.chars().any(|ch| ch.is_alphabetic())
}

impl RuleBasedEntityProvider {
    fn scan_names(
        &self,
        text: &str,
        claimed: &mut [bool],
        mentions: &mut Vec<EntityMention>,
        language: Option<&str>,
    ) {
        // Word spans with byte offsets.
        let mut words: Vec<(usize, usize, String)> = Vec::new();
        let mut start: Option<usize> = None;
        for (offset, ch) in text.char_indices() {
            if ch.is_alphabetic() || matches!(ch, '\'' | '-') {
                if start.is_none() {
                    start = Some(offset);
                }
            } else if let Some(word_start) = start.take() {
                words.push((word_start, offset, text[word_start..offset].to_string()));
                let _ = word_start;
            }
        }
        if let Some(word_start) = start {
            words.push((word_start, text.len(), text[word_start..].to_string()));
        }
        let mut index = 0;
        while index < words.len() {
            let (word_start, word_end, word) = &words[index];
            if is_claimed(claimed, *word_start, *word_end) {
                index += 1;
                continue;
            }
            if is_acronym(word) {
                // Acronyms that are common words (`IT`, `US`) are capitalization,
                // not organizations.
                if self.is_common_word(word) {
                    index += 1;
                    continue;
                }
                claim(claimed, *word_start, *word_end);
                push_mention(
                    mentions,
                    "organization",
                    casefold(word),
                    *word_start,
                    *word_end,
                    0.5,
                    language,
                );
                index += 1;
                continue;
            }
            if !is_title_word(word) {
                index += 1;
                continue;
            }
            // Greedily extend over consecutive title words joined by one space.
            let mut end_word = index;
            while end_word + 1 < words.len() && end_word + 1 - index < 3 {
                let (next_start, next_end, next_word) = &words[end_word + 1];
                if text[words[end_word].1..*next_start] != *" " || !is_title_word(next_word) {
                    break;
                }
                if is_claimed(claimed, *next_start, *next_end) {
                    break;
                }
                end_word += 1;
            }
            let span_start = words[index].0;
            let span_end = words[end_word].1;
            let span_text = &text[span_start..span_end];
            // A lone title word that is a common word is sentence
            // capitalization (`Where`, `Thanks`), not a name. Multi-word spans
            // keep their heuristic reading.
            if end_word == index && self.is_common_word(&words[index].2) {
                index += 1;
                continue;
            }
            let last_word = &words[end_word].2;
            let is_org = ORG_SUFFIXES.contains(&casefold(last_word).as_str());
            let (entity_type, confidence) = if is_org {
                ("organization", 0.7)
            } else if end_word > index {
                ("person", 0.65)
            } else {
                ("person", 0.45)
            };
            claim(claimed, span_start, span_end);
            push_mention(
                mentions,
                entity_type,
                casefold(span_text),
                span_start,
                span_end,
                confidence,
                language,
            );
            index = end_word + 1;
        }
    }
}

impl EntityProvider for RuleBasedEntityProvider {
    fn extract(&self, text: &str, language: Option<&str>) -> Vec<EntityMention> {
        if text.is_empty() {
            return Vec::new();
        }
        let mut claimed = vec![false; text.len()];
        let mut mentions = Vec::new();
        // Priority order: structured identifiers first so numbers inside
        // URLs, emails, currency, and dates are never double-counted.
        scan_urls(text, &mut claimed, &mut mentions, language);
        scan_emails(text, &mut claimed, &mut mentions, language);
        scan_mentions(text, &mut claimed, &mut mentions, language);
        scan_currency(text, &mut claimed, &mut mentions, language);
        scan_dates_times(text, &mut claimed, &mut mentions, language);
        scan_numbers(text, &mut claimed, &mut mentions, language);
        self.scan_names(text, &mut claimed, &mut mentions, language);
        self.bounded(mentions)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new("rule_based_entities")
            .with_quality(CapabilityLevel::Basic)
            .with_fallback("heuristic spans; prefer a trained NER provider for production")
    }
}

/// Entity agreement in `[0.0, 1.0]`: the fraction of the larger side's
/// mentions matched by the other side. Exact `(type, value)` matches count
/// `1.0`; compatible matches count `0.5`: for person/organization names,
/// same-type containment or near-identical values, and for any type an
/// entity value appearing as an ordinary token span in the other raw text.
/// Identifiers (URLs, emails, mentions, numbers, currency, dates, times)
/// never fuzz-match. Empty on either side reads `0.0`.
pub fn entity_agreement(
    left: &[EntityMention],
    left_raw: &str,
    right: &[EntityMention],
    right_raw: &str,
) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let left_fold = casefold(left_raw);
    let right_fold = casefold(right_raw);
    let mut matched = 0.0;
    let mut used = vec![false; right.len()];
    for mention in left {
        let mut best: Option<(usize, f64)> = None;
        for (position, candidate) in right.iter().enumerate() {
            if used[position] || candidate.entity_type != mention.entity_type {
                continue;
            }
            if candidate.value == mention.value {
                best = Some((position, 1.0));
                break;
            }
            if best.is_none()
                && compatible_values(&mention.entity_type, &mention.value, &candidate.value)
            {
                best = Some((position, 0.5));
            }
        }
        if let Some((position, weight)) = best {
            used[position] = true;
            matched += weight;
            continue;
        }
        // Entity-vs-ordinary-token: the value surfaces in the other text
        // without being extracted there (casing or context defeated the
        // heuristic). Counts as weak compatibility, never conflict.
        if mention.value.chars().count() >= 3 && right_fold.contains(&mention.value) {
            matched += 0.5;
        }
    }
    // Symmetric token-side check for right-only values missed above.
    for (position, mention) in right.iter().enumerate() {
        if used[position] || mention.value.chars().count() < 3 {
            continue;
        }
        if left_fold.contains(&mention.value)
            && !left
                .iter()
                .any(|candidate| candidate.entity_type == mention.entity_type)
        {
            matched += 0.25;
        }
    }
    (matched / left.len().max(right.len()) as f64).clamp(0.0, 1.0)
}

/// Entity conflict in `[0.0, 1.0]`: the fraction of shared entity types whose
/// value sets are disjoint with no compatible pair. Types appearing on one
/// side only are not conflicts, and empty on either side reads `0.0`, so
/// missing evidence never penalizes.
pub fn entity_conflict(left: &[EntityMention], right: &[EntityMention]) -> f64 {
    use std::collections::{BTreeMap, BTreeSet};
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let mut left_by_type: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut right_by_type: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for mention in left {
        left_by_type
            .entry(mention.entity_type.as_str())
            .or_default()
            .push(mention.value.as_str());
    }
    for mention in right {
        right_by_type
            .entry(mention.entity_type.as_str())
            .or_default()
            .push(mention.value.as_str());
    }
    let shared: BTreeSet<&str> = left_by_type
        .keys()
        .collect::<BTreeSet<_>>()
        .intersection(&right_by_type.keys().collect::<BTreeSet<_>>())
        .copied()
        .copied()
        .collect();
    if shared.is_empty() {
        return 0.0;
    }
    let mut disjoint = 0usize;
    for entity_type in &shared {
        let left_values = &left_by_type[entity_type];
        let right_values = &right_by_type[entity_type];
        let mut linked = false;
        for left_value in left_values {
            for right_value in right_values {
                if left_value == right_value
                    || compatible_values(entity_type, left_value, right_value)
                {
                    linked = true;
                    break;
                }
            }
            if linked {
                break;
            }
        }
        if !linked {
            disjoint += 1;
        }
    }
    (disjoint as f64 / shared.len() as f64).clamp(0.0, 1.0)
}

fn compatible_values(entity_type: &str, left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    // Identifiers match exactly or not at all: a different URL path, email,
    // number, amount, or timestamp is a different referent even when the
    // strings look alike (`…/north` vs `…/south`). Only person and
    // organization names tolerate containment and near-matches (typos,
    // shortenings like `Smith` for `John Smith`).
    if !matches!(entity_type, "person" | "organization") {
        return false;
    }
    let (shorter, longer) = if left.len() <= right.len() {
        (left, right)
    } else {
        (right, left)
    };
    if shorter.chars().count() >= 3 && longer.contains(shorter) {
        return true;
    }
    crate::lexical::character::combined_character_similarity(left, right) > 0.85
}

/// Type-level entity evidence lines (counts and type names only, never
/// mention values, so evidence stays safe to log).
pub fn entity_evidence_lines(
    left: &[EntityMention],
    left_raw: &str,
    right: &[EntityMention],
    right_raw: &str,
) -> Vec<String> {
    let agreement = entity_agreement(left, left_raw, right, right_raw);
    let conflict = entity_conflict(left, right);
    vec![
        format!("entity_agreement={agreement:.3}"),
        format!("entity_conflict={conflict:.3}"),
        format!("entity_count_a={}", left.len()),
        format!("entity_count_b={}", right.len()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(text: &str) -> Vec<EntityMention> {
        RuleBasedEntityProvider::default().extract(text, Some("en"))
    }

    #[test]
    fn structured_identifiers_win_over_numbers() {
        let mentions =
            extract("contact ada@example.com or visit https://example.com/42 by 2026-01-30");
        let types: Vec<&str> = mentions
            .iter()
            .map(|mention| mention.entity_type.as_str())
            .collect();
        assert!(types.contains(&"email"));
        assert!(types.contains(&"url"));
        assert!(types.contains(&"date"));
        // The `42` inside the URL and the date parts are claimed, not numbers.
        assert!(!mentions
            .iter()
            .any(|mention| mention.entity_type == "number" && mention.value == "42"));
        for mention in &mentions {
            assert!(mention.start < mention.end);
            assert_eq!(mention.provider, "rule_based_entities");
            assert_eq!(mention.language.as_deref(), Some("en"));
        }
    }

    #[test]
    fn currency_and_mentions_normalize() {
        let mentions = extract("send $1,200 to @Ada_Lovelace before 14:30");
        assert!(mentions
            .iter()
            .any(|mention| mention.entity_type == "currency"));
        assert!(mentions
            .iter()
            .any(|mention| mention.entity_type == "mention" && mention.value == "@ada_lovelace"));
        assert!(mentions.iter().any(|mention| mention.entity_type == "time"));
    }

    #[test]
    fn names_and_org_suffixes_classify() {
        let person = extract("Ada Lovelace wrote the notes");
        assert!(person
            .iter()
            .any(|mention| mention.entity_type == "person" && mention.value == "ada lovelace"));
        let org = extract("Ada Lovelace Bank approved the transfer");
        assert!(org
            .iter()
            .any(|mention| mention.entity_type == "organization"));
    }

    #[test]
    fn agreement_and_conflict_are_graded() {
        let left = extract("Ada Lovelace met Grace Hopper");
        let right_same = extract("Ada Lovelace met Grace Hopper");
        assert!(
            entity_agreement(
                &left,
                "Ada Lovelace met Grace Hopper",
                &right_same,
                "Ada Lovelace met Grace Hopper"
            ) > 0.9
        );
        assert_eq!(entity_conflict(&left, &right_same), 0.0);
        let right_other = extract("Alan Turing met John Neumann");
        assert_eq!(
            entity_agreement(
                &left,
                "Ada Lovelace met Grace Hopper",
                &right_other,
                "Alan Turing met John Neumann"
            ),
            0.0
        );
        assert_eq!(entity_conflict(&left, &right_other), 1.0);
        // Missing evidence never penalizes.
        assert_eq!(entity_agreement(&left, "x", &[], "y"), 0.0);
        assert_eq!(entity_conflict(&left, &[]), 0.0);
    }

    #[test]
    fn bounds_cap_count_and_span() {
        let provider = RuleBasedEntityProvider::new(2, 8);
        let mentions = provider.extract(
            "Ada Lovelace and Grace Hopper met Alan Turing at Example Corporation",
            None,
        );
        assert!(mentions.len() <= 2);
        assert!(mentions
            .iter()
            .all(|mention| mention.value.chars().count() <= 8));
    }
}
