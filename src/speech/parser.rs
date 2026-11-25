//! Lossless structural parsing for speech normalization.
//!
//! Parsing does not guess a locale's currency or date order. Exact source
//! spans survive; uncertain or malformed numeric constructs are explicitly
//! marked instead of being partially expanded into unrelated numbers.
use super::core::{
    digit, is_cjk, left_boundary, parse_number, right_boundary, scan_numeric_end,
    scan_numeric_end_for_locale, skip_space,
};
use super::types::{NumberReading, SpeechDocument, SpeechOptions, SpeechSpan, SpeechValue};
use crate::TextIntelError;

/// Parse speech-oriented spans without changing or dropping input bytes.
pub fn parse_for_speech(
    text: &str,
    options: &SpeechOptions,
) -> Result<SpeechDocument, TextIntelError> {
    parse_with_regions(text, options, None)
}

/// Reuse the original source's protected ranges across compatibility adapters.
pub(crate) fn parse_lexical_source(
    source: &super::lexer::LexicalSource<'_>,
    options: &SpeechOptions,
) -> Result<SpeechDocument, TextIntelError> {
    parse_with_regions(source.text(), options, Some(source.regions()))
}

fn parse_with_regions(
    text: &str,
    options: &SpeechOptions,
    regions: Option<&[std::ops::Range<usize>]>,
) -> Result<SpeechDocument, TextIntelError> {
    options.validate()?;
    let length = text.chars().count();
    if length > options.max_input_chars {
        return Err(TextIntelError::InputTooLong {
            length,
            maximum: options.max_input_chars,
        });
    }
    if matches!(text.trim_start().chars().next(), Some('{' | '['))
        && serde_json::from_str::<serde_json::Value>(text)
            .is_ok_and(|v| v.is_object() || v.is_array())
    {
        return Ok(SpeechDocument {
            text: text.into(),
            locale: options.locale,
            spans: vec![SpeechSpan {
                start: 0,
                end: text.len(),
                source: text.into(),
                value: identifier("structured_data"),
            }],
        });
    }
    let scanned_regions;
    let regions = match regions {
        Some(regions) => regions,
        None => {
            scanned_regions = super::lexer::protected_regions(text);
            &scanned_regions
        }
    };
    let mut region_index = 0;
    let abbreviations = super::abbreviations::Matcher::new(
        text,
        regions,
        options.locale,
        options.abbreviation_policy,
    );
    let mut spans = Vec::new();
    let mut cursor = 0;
    let mut plain_start = 0;
    while cursor < text.len() {
        while region_index < regions.len() && regions[region_index].end <= cursor {
            region_index += 1;
        }
        let protected = regions
            .get(region_index)
            .filter(|region| region.start == cursor)
            .map(|region| {
                (
                    region.end,
                    identifier(if text[cursor..].starts_with('`') {
                        "code"
                    } else {
                        "markup"
                    }),
                )
            });
        if let Some((end, value)) = protected.or_else(|| {
            if options.interpret_as == super::types::SpeechInterpretation::Auto
                && let Some(found) = abbreviations.match_at(cursor)
            {
                return Some((
                    found.end,
                    SpeechValue::Abbreviation {
                        id: found.entry.id.clone(),
                    },
                ));
            }
            parse_at(text, cursor, options)
        }) {
            if plain_start < cursor {
                push_span(
                    &mut spans,
                    text,
                    plain_start,
                    cursor,
                    SpeechValue::Text,
                    options,
                )?;
            }
            let value = enforce_hint(value, options);
            push_span(&mut spans, text, cursor, end, value, options)?;
            cursor = end;
            plain_start = end;
        } else {
            cursor += text[cursor..].chars().next().map_or(0, char::len_utf8);
        }
    }
    if plain_start < text.len() {
        push_span(
            &mut spans,
            text,
            plain_start,
            text.len(),
            SpeechValue::Text,
            options,
        )?;
    }
    Ok(SpeechDocument {
        text: text.into(),
        locale: options.locale,
        spans,
    })
}

fn push_span(
    spans: &mut Vec<SpeechSpan>,
    text: &str,
    start: usize,
    end: usize,
    value: SpeechValue,
    options: &SpeechOptions,
) -> Result<(), TextIntelError> {
    if spans.len() >= options.max_spans {
        return Err(TextIntelError::TooManySegments {
            maximum: options.max_spans,
        });
    }
    spans.push(SpeechSpan {
        start,
        end,
        source: text[start..end].into(),
        value,
    });
    Ok(())
}

fn ambiguous(reason: &str) -> SpeechValue {
    SpeechValue::Ambiguous {
        reason: reason.into(),
    }
}
fn identifier(kind: &str) -> SpeechValue {
    SpeechValue::Identifier { kind: kind.into() }
}
pub(super) fn token_end(text: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in text[start..].char_indices() {
        if ch.is_whitespace()
            || matches!(
                ch,
                '<' | '>'
                    | '"'
                    | '\''
                    | '('
                    | ')'
                    | '['
                    | ']'
                    | '{'
                    | '}'
                    | ','
                    | ';'
                    | '!'
                    | '?'
                    | '。'
                    | '、'
                    | '，'
                    | '；'
                    | '！'
                    | '？'
            )
        {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    while end > start
        && text[..end]
            .chars()
            .next_back()
            .is_some_and(|ch| matches!(ch, '.' | ',' | ';' | ':' | '!' | '?'))
    {
        end -= text[..end].chars().next_back().map_or(0, char::len_utf8);
    }
    end
}

fn parse_at(text: &str, start: usize, options: &SpeechOptions) -> Option<(usize, SpeechValue)> {
    if !left_boundary(text, start) {
        return None;
    }
    if let Some(end) = super::lexer::protected_path_end(text, start) {
        return Some((end, identifier("path")));
    }
    if let Some(end) = super::lexer::filename_end(text, start) {
        return Some((end, identifier("filename")));
    }
    if let Some((end, value)) = super::temporal::parse_timestamp_at(text, start) {
        return Some((end, temporal_ir(value, options)));
    }
    // Rendering style cannot turn an automatically protected phone shape
    // into a numerical range. Explicit numeric intent still selects ranges.
    if options.interpret_as == super::types::SpeechInterpretation::Auto
        && let Some(end) = telephone_end(text, start)
    {
        let raw = &text[start..end];
        let first = raw.chars().next()?;
        if (digit(first).is_some() && raw.contains('-') && !date_shaped(raw))
            || first == '+'
            || (first == '(' && !date_shaped(raw[1..].trim_end_matches(')')))
        {
            return Some((end, identifier("telephone")));
        }
    }
    if let Some(found) = super::grammar::parse_at(text, start, options) {
        return Some(found);
    }
    if !matches!(
        options.interpret_as,
        super::types::SpeechInterpretation::Number
            | super::types::SpeechInterpretation::Currency
            | super::types::SpeechInterpretation::Measure
            | super::types::SpeechInterpretation::Duration
    ) && let Some((end, value)) = super::temporal::parse_at(text, start, options)
    {
        return Some((end, temporal_ir(value, options)));
    }
    if !matches!(
        options.interpret_as,
        super::types::SpeechInterpretation::Number
            | super::types::SpeechInterpretation::Date
            | super::types::SpeechInterpretation::Time
            | super::types::SpeechInterpretation::Duration
    ) && let Some(found) = super::quantities::parse_at(text, start, options)
    {
        return Some(found);
    }
    if let Some((end, value)) = super::expressions::parse_at(text, start, options) {
        return Some((end, SpeechValue::Expression { value }));
    }
    let rest = &text[start..];
    let first = rest.chars().next()?;
    // Han, kana and Hangul do not require word-separating spaces. Do not
    // consume an entire CJK sentence as an alphanumeric identifier.
    if is_cjk(first) {
        return None;
    }
    let end = token_end(text, start);
    let token = &text[start..end];
    if ["https://", "http://", "www."].iter().any(|prefix| {
        rest.get(..prefix.len())
            .is_some_and(|s| s.eq_ignore_ascii_case(prefix))
    }) {
        let url_end = url_end(text, start);
        return Some((url_end, identifier("url")));
    }
    if token.contains('@') && email_like(token) {
        return Some((end, identifier("email")));
    }
    if matches!(first, '@' | '#') && token.len() > first.len_utf8() {
        return Some((
            end,
            identifier(if first == '@' { "mention" } else { "hashtag" }),
        ));
    }
    if first.is_alphabetic() {
        // Protect a full mixed identifier before looking for interior digits.
        if token.chars().any(|ch| digit(ch).is_some()) {
            return Some((
                end,
                identifier(if version_like(token) {
                    "version"
                } else {
                    "alphanumeric"
                }),
            ));
        }
        return None;
    }
    if digit(first).is_none() && !matches!(first, '+' | '-' | '−' | '(' | '.' | ',') {
        return None;
    }
    // Signed and parenthesized phone numbers are recognizable without
    // mistaking ordinary grouped quantities for telephone numbers.
    if matches!(first, '+' | '(')
        && let Some(end) = telephone_end(text, start)
        && !(first == '(' && date_shaped(text[start + 1..end].trim_end_matches(')')))
        && !explicit_number_candidate(text, start, end, options)
    {
        return Some((end, identifier("telephone")));
    }
    let numeric_end = numeric_candidate_end(text, start, options);
    if numeric_end == start {
        return None;
    }
    let raw = &text[start..numeric_end];
    if !right_boundary(text, numeric_end) {
        if raw.contains(':') {
            let (time_end, value) = parse_time(text, start, numeric_end, options);
            if time_end > numeric_end {
                return Some((time_end, value));
            }
            return Some((
                end.max(numeric_end).max(token_end(text, numeric_end)),
                ambiguous("time has an unsupported suffix"),
            ));
        }
        return Some((
            end.max(numeric_end).max(token_end(text, numeric_end)),
            identifier("alphanumeric"),
        ));
    }
    if digit(first).is_some()
        && raw.contains('-')
        && !date_shaped(raw)
        && let Some(end) = telephone_end(text, start)
    {
        return Some((end, identifier("telephone")));
    }
    if let Some(value) =
        super::temporal::parse_numeric_date(raw, options).map(|value| temporal_ir(value, options))
    {
        return Some((numeric_end, value));
    }
    if raw.contains(':') {
        return Some(parse_time(text, start, numeric_end, options));
    }
    if version_like(raw) {
        return Some((numeric_end, identifier("version")));
    }
    if raw.contains('/') || raw.chars().skip(1).any(|ch| matches!(ch, '+' | '-' | '−')) {
        return Some((
            numeric_end,
            ambiguous("unsupported fraction, range, or malformed date"),
        ));
    }
    let range_start = skip_space(text, numeric_end);
    if let Some(ch @ ('-' | '–' | '—' | '…')) = text[range_start..].chars().next() {
        let second_start = skip_space(text, range_start + ch.len_utf8());
        if text[second_start..]
            .chars()
            .next()
            .is_some_and(|ch| digit(ch).is_some())
        {
            let end = numeric_candidate_end(text, second_start, options);
            return Some((
                end,
                ambiguous("numeric range needs an explicit reading policy"),
            ));
        }
    }
    let number = match parse_number(raw, options) {
        Ok(value) => value,
        Err(reason) => return Some((numeric_end, ambiguous(reason))),
    };
    let reading =
        if number.fraction.is_none() && number.integer.len() > 1 && number.integer.starts_with('0')
        {
            NumberReading::Digits
        } else {
            NumberReading::Cardinal
        };
    Some((
        numeric_end,
        SpeechValue::Number {
            value: number,
            reading,
        },
    ))
}

fn url_end(text: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in text[start..].char_indices() {
        if ch.is_whitespace() || matches!(ch, '<' | '>' | '"' | '\'') {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    while end > start
        && text[..end]
            .chars()
            .next_back()
            .is_some_and(|ch| matches!(ch, '.' | ',' | ';' | '!' | '?' | ')' | ']'))
    {
        end -= text[..end].chars().next_back().map_or(0, char::len_utf8);
    }
    end
}
fn email_like(token: &str) -> bool {
    let mut pieces = token.split('@');
    let Some(local) = pieces.next() else {
        return false;
    };
    let Some(domain) = pieces.next() else {
        return false;
    };
    !local.is_empty()
        && pieces.next().is_none()
        && domain.contains('.')
        && local
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '%' | '+' | '-'))
        && domain
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-'))
        && domain.split('.').all(|part| !part.is_empty())
}
fn version_like(token: &str) -> bool {
    let value = token
        .strip_prefix('v')
        .or_else(|| token.strip_prefix('V'))
        .unwrap_or(token);
    let pieces: Vec<_> = value.split('.').collect();
    pieces.len() >= 3
        && pieces
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|ch| digit(ch).is_some()))
}
fn telephone_end(text: &str, start: usize) -> Option<usize> {
    let mut digits = 0;
    let mut end = start;
    let mut separators = 0;
    for (offset, ch) in text[start..].char_indices() {
        if digit(ch).is_some() {
            digits += 1;
            end = start + offset + ch.len_utf8();
        } else if matches!(ch, '-' | '(' | ')' | ' ' | '\u{a0}') || (ch == '+' && offset == 0) {
            separators += 1;
        } else {
            break;
        }
        if digits > 15 {
            return None;
        }
    }
    (digits >= 7 && separators > 0 && right_boundary(text, end)).then_some(end)
}

fn date_shaped(raw: &str) -> bool {
    let parts: Vec<_> = raw.split('-').collect();
    parts.len() >= 3
        && (parts[0].chars().count() == 4
            || (parts[2].chars().count() == 4
                && parts[0].chars().count() <= 2
                && parts[1].chars().count() <= 2))
}

/// Translate the shared semantic representation to the established compact IR.
/// Parsing and calendar/clock validation remain in the temporal module.
fn temporal_ir(value: super::temporal::TemporalValue, options: &SpeechOptions) -> SpeechValue {
    use super::temporal::TemporalValue;
    use super::types::{NumberNotation, ReadingStyle};
    if let TemporalValue::Invalid { reason } = value {
        return ambiguous(&reason);
    }
    let value = SpeechValue::Temporal { value };
    if options.reading_style == ReadingStyle::Precise
        && options.number_notation == NumberNotation::Phonetic
        && let Some(compact) = super::temporal::compact_value(&value)
    {
        return compact;
    }
    value
}

fn parse_time(
    text: &str,
    start: usize,
    end: usize,
    options: &SpeechOptions,
) -> (usize, SpeechValue) {
    let (end, value) = super::temporal::parse_numeric_time(text, start, end, options);
    (end, temporal_ir(value, options))
}

fn enforce_hint(value: SpeechValue, options: &SpeechOptions) -> SpeechValue {
    use super::types::SpeechInterpretation as Hint;
    let accepted = match (&value, options.interpret_as) {
        (_, Hint::Auto)
        | (SpeechValue::Identifier { .. } | SpeechValue::Ambiguous { .. } | SpeechValue::Text, _) => {
            true
        }
        (SpeechValue::Number { .. } | SpeechValue::InflectedNumber { .. }, Hint::Number) => true,
        (SpeechValue::Expression { value }, Hint::Number) => {
            !matches!(value, super::expressions::ExpressionValue::Duration { .. })
        }
        (SpeechValue::Date { .. }, Hint::Date) | (SpeechValue::Time { .. }, Hint::Time) => true,
        (SpeechValue::Temporal { value }, Hint::Date) => {
            matches!(
                super::temporal::kind(value),
                "date" | "invalid_date" | "ambiguous"
            )
        }
        (SpeechValue::Temporal { value }, Hint::Time) => {
            super::temporal::kind(value) == "time" || super::temporal::kind(value) == "ambiguous"
        }
        (SpeechValue::Temporal { value }, Hint::DateTime) => {
            super::temporal::kind(value) == "datetime"
                || super::temporal::kind(value) == "ambiguous"
        }
        (SpeechValue::Currency { .. }, Hint::Currency) => true,
        (
            SpeechValue::Measure { .. }
            | SpeechValue::Percent { .. }
            | SpeechValue::InflectedNumber { .. }
            | SpeechValue::LabelledNumber { .. }
            | SpeechValue::MeasuredExpression { .. },
            Hint::Measure,
        ) => true,
        (SpeechValue::Expression { value }, Hint::Duration) => {
            super::expressions::kind(value) == "duration"
        }
        _ => false,
    };
    if accepted {
        value
    } else {
        ambiguous("input does not match the explicit speech interpretation")
    }
}

fn numeric_candidate_end(text: &str, start: usize, options: &SpeechOptions) -> usize {
    if options.interpret_as == super::types::SpeechInterpretation::Number {
        scan_numeric_end_for_locale(text, start, options.locale)
    } else {
        scan_numeric_end(text, start)
    }
}

fn explicit_number_candidate(
    text: &str,
    start: usize,
    end: usize,
    options: &SpeechOptions,
) -> bool {
    options.interpret_as == super::types::SpeechInterpretation::Number
        && scan_numeric_end_for_locale(text, start, options.locale) == end
        && parse_number(&text[start..end], options).is_ok()
}
