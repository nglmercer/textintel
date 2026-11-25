//! Calendar and clock grammar shared by speech parsing and rendering.
//!
//! Semantic fields retain written-year width and presentation where it changes
//! a legitimate reading. Calendar validation is independent of verbalization;
//! malformed dates never become individually expanded numeric fragments.
use serde::{Deserialize, Serialize};

use super::core::{ascii_digits, is_cjk, valid_date};
use super::lexicons::month;
use super::numbers::verbalize_number;
use super::types::{
    ClockPolicy, DateOrder, InvalidDatePolicy, NumberNotation, NumberReading, NumberValue,
    ReadingStyle, SpeechInterpretation, SpeechLocale, SpeechOptions, SpeechValue,
};
use super::verbalize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatePresentation {
    Numeric,
    DayFirst,
    MonthFirst,
    Component,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TemporalValue {
    Date {
        year: Option<String>,
        month: Option<u32>,
        day: Option<u32>,
        presentation: DatePresentation,
        /// Preserve a written grammatical connector such as Spanish `del` or a
        /// required Russian governing preposition such as `в`.
        connector: Option<String>,
        /// An explicitly written year label, e.g. Russian `г.` or `года`.
        year_label: bool,
    },
    /// Individually bounded components which do not form a Gregorian date.
    /// Produced only by the caller's explicit literal-reading policy.
    InvalidCalendarDate {
        year: Option<String>,
        month: u32,
        day: u32,
        presentation: DatePresentation,
        connector: Option<String>,
        year_label: bool,
    },
    Clock {
        hour: u32,
        minute: Option<u32>,
        second: Option<u32>,
        /// Lexical period prefix: Korean 오전/오후, for example.
        period: Option<String>,
        /// AM/PM is retained separately; a renderer never invents a period.
        meridiem: Option<String>,
        half: bool,
    },
    /// Exact decimal seconds of one complete numeric clock, never a timestamp.
    /// The inner value must be a non-half Clock with minutes and seconds.
    FractionalClock {
        clock: Box<TemporalValue>,
        /// Canonical ASCII digits, including every leading and trailing zero.
        fraction: String,
    },
    /// Complete RFC 3339 timestamp. The wall-clock fields stay in their written
    /// offset; this value never converts to a different timezone.
    Timestamp {
        year: String,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
        /// Exact ASCII digits, including leading and trailing zeroes.
        fraction: Option<String>,
        offset: TimestampOffset,
    },
    EraYear {
        era: String,
        year: String,
    },
    Decade {
        year: u32,
    },
    CalendarSequence {
        items: Vec<CalendarItem>,
    },
    /// A written list of days sharing a named month, e.g. `17/18 juin`.
    DateList {
        days: Vec<u32>,
        month: u32,
        year: Option<String>,
    },
    Invalid {
        reason: String,
    },
}

/// RFC 3339 distinguishes `-00:00` (unknown local offset) from a known zero
/// offset. `Z`/`z` and `+00:00` retain their separate written semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TimestampOffset {
    Utc,
    Known {
        negative: bool,
        hours: u32,
        minutes: u32,
    },
    UnknownLocal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CalendarItem {
    Date { value: Box<TemporalValue> },
    Weekday { index: u8 },
    Range,
}

const RUSSIAN_NOMINATIVE_MONTHS: [&str; 12] = [
    "январь",
    "февраль",
    "март",
    "апрель",
    "май",
    "июнь",
    "июль",
    "август",
    "сентябрь",
    "октябрь",
    "ноябрь",
    "декабрь",
];
const RUSSIAN_PREPOSITIONAL_MONTHS: [&str; 12] = [
    "январе",
    "феврале",
    "марте",
    "апреле",
    "мае",
    "июне",
    "июле",
    "августе",
    "сентябре",
    "октябре",
    "ноябре",
    "декабре",
];

pub fn kind(value: &TemporalValue) -> &'static str {
    match value {
        TemporalValue::Clock { .. } | TemporalValue::FractionalClock { .. } => "time",
        TemporalValue::Timestamp { .. } => "datetime",
        TemporalValue::InvalidCalendarDate { .. } => "invalid_date",
        TemporalValue::Invalid { .. } => "ambiguous",
        _ => "date",
    }
}

/// Validated projection of a complete, unlabelled date or clock into compact IR.
/// This does not prove equivalent wording or discard arbitrary calendar metadata.
/// Comparing callers must also preserve source, spans, locale and options.
pub(crate) fn compact_value(value: &SpeechValue) -> Option<SpeechValue> {
    match value {
        SpeechValue::Date { year, month, day }
            if (1..=9999).contains(year) && valid_date(*year, *month, *day) =>
        {
            Some(value.clone())
        }
        SpeechValue::Time {
            hour,
            minute,
            second,
        } if *hour <= 23 && *minute <= 59 && second.is_none_or(|s| s <= 59) => Some(value.clone()),
        SpeechValue::Temporal {
            value:
                TemporalValue::Date {
                    year: Some(year),
                    month: Some(month),
                    day: Some(day),
                    presentation: DatePresentation::Numeric,
                    connector: None,
                    year_label: false,
                },
        } if year.len() == 4 && year.bytes().all(|c| c.is_ascii_digit()) => {
            // Re-enter only with a constructed scalar variant: depth is at
            // most two, and the same compact field guards remain authoritative.
            compact_value(&SpeechValue::Date {
                year: year.parse().ok()?,
                month: *month,
                day: *day,
            })
        }
        SpeechValue::Temporal {
            value:
                TemporalValue::Clock {
                    hour,
                    minute: Some(minute),
                    second,
                    meridiem,
                    period: None,
                    half: false,
                },
        } => {
            let hour = clock_hour_24(*hour, meridiem.as_deref())?;
            compact_value(&SpeechValue::Time {
                hour,
                minute: *minute,
                second: *second,
            })
        }
        _ => None,
    }
}

/// Invalidity remains visible even when written components can be spoken.
/// Publicly constructed sequences are inspected without recursive stack use.
pub fn diagnostic(value: &TemporalValue) -> Option<&str> {
    let mut pending = vec![value];
    let mut visited = 0;
    while let Some(value) = pending.pop() {
        visited += 1;
        if visited > 256 {
            return Some("calendar sequence exceeds diagnostic limit");
        }
        match value {
            TemporalValue::Invalid { reason } => return Some(reason),
            TemporalValue::InvalidCalendarDate { .. } => return Some("invalid calendar date"),
            TemporalValue::CalendarSequence { items } => {
                if items.len() + pending.len() > 256 {
                    return Some("calendar sequence exceeds diagnostic limit");
                }
                for item in items.iter().rev() {
                    if let CalendarItem::Date { value } = item {
                        pending.push(value);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Recognize a complete temporal expression beginning at a UTF-8 boundary.
/// Returned byte offsets always refer to the caller's original input.
pub fn parse_at(
    text: &str,
    start: usize,
    options: &SpeechOptions,
) -> Option<(usize, TemporalValue)> {
    let rest = text.get(start..)?;
    if rest.is_empty()
        || matches!(
            options.interpret_as,
            SpeechInterpretation::Number
                | SpeechInterpretation::Currency
                | SpeechInterpretation::Measure
                | SpeechInterpretation::Duration
        )
    {
        return None;
    }
    if let Some(parsed) = parse_timestamp_at(text, start) {
        return Some(parsed);
    }
    if !possible_start(rest, options.locale) {
        return None;
    }
    if let Some(parsed) = parse_fractional_clock_at(text, start, options) {
        return Some(parsed);
    }
    let mut ends = vec![];
    for (offset, ch) in rest.char_indices().take(100) {
        let end = offset + ch.len_utf8();
        if rest[end..].chars().next().is_none_or(|next| {
            next.is_whitespace()
                || matches!(
                    next,
                    ',' | ';'
                        | '!'
                        | '?'
                        | '。'
                        | '！'
                        | '？'
                        | '('
                        | ')'
                        | '（'
                        | '）'
                        | '〜'
                        | '～'
                )
                || is_cjk(next)
                || next == '.'
                    && rest[end + 1..]
                        .chars()
                        .next()
                        .is_none_or(|after| after.is_whitespace())
        }) {
            ends.push(end);
        }
    }
    // Longest valid expression wins. Restrict candidates to a small bounded
    // prefix; this is a grammar scan, not an unbounded suffix search.
    for end in ends.into_iter().rev() {
        let raw = rest[..end].trim_end();
        if raw.is_empty() {
            continue;
        }
        if let Some(value) = parse_expression(raw, options) {
            return Some((start + raw.len(), value));
        }
    }
    None
}

/// Parse a complete RFC 3339 timestamp atomically. Once the fixed date/T
/// prefix is present, malformed suffixes become one unresolved value instead
/// of falling through to separate date, clock and offset fragments.
pub fn parse_timestamp_at(text: &str, start: usize) -> Option<(usize, TemporalValue)> {
    let rest = text.get(start..)?;
    if !timestamp_prefix(rest.as_bytes()) {
        return None;
    }
    let end = timestamp_candidate_end(rest);
    let raw = &rest[..end];
    let parsed = parse_timestamp(raw)
        .unwrap_or_else(|| invalid("invalid or unsupported RFC 3339 timestamp"));
    Some((start + end, parsed))
}

fn timestamp_prefix(bytes: &[u8]) -> bool {
    bytes.len() >= 11
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(u8::is_ascii_digit)
        && matches!(bytes[10], b'T' | b't')
}

fn timestamp_candidate_end(rest: &str) -> usize {
    let mut end = 0;
    for (offset, ch) in rest.char_indices() {
        if ch.is_whitespace()
            || matches!(
                ch,
                ';' | '!' | '?' | '。' | '！' | '？' | '(' | ')' | '（' | '）'
            )
            || ch == ',' && has_complete_timestamp_offset(&rest[..offset])
        {
            break;
        }
        end = offset + ch.len_utf8();
    }
    // Sentence punctuation is outside the timestamp. Brackets and path-like
    // suffixes remain inside so IXDTF annotations cannot be silently dropped.
    while end > 0 && rest[..end].ends_with('.') {
        end -= rest[..end].chars().next_back().map_or(0, char::len_utf8);
    }
    while end > 0 && rest[..end].ends_with(',') {
        end -= 1;
    }
    end
}

fn has_complete_timestamp_offset(prefix: &str) -> bool {
    prefix.ends_with(['Z', 'z'])
        || prefix.as_bytes().get(prefix.len().saturating_sub(6)) == Some(&b'+')
            && prefix.as_bytes().get(prefix.len().saturating_sub(3)) == Some(&b':')
        || prefix.as_bytes().get(prefix.len().saturating_sub(6)) == Some(&b'-')
            && prefix.as_bytes().get(prefix.len().saturating_sub(3)) == Some(&b':')
}

fn parse_timestamp(raw: &str) -> Option<TemporalValue> {
    let bytes = raw.as_bytes();
    if !timestamp_prefix(bytes) || bytes.iter().any(|byte| !byte.is_ascii()) {
        return None;
    }
    let year = &raw[..4];
    let year_number = year.parse::<u32>().ok()?;
    let month = raw[5..7].parse::<u32>().ok()?;
    let day = raw[8..10].parse::<u32>().ok()?;
    let hour = raw.get(11..13)?.parse::<u32>().ok()?;
    let minute = raw.get(14..16)?.parse::<u32>().ok()?;
    let second = raw.get(17..19)?.parse::<u32>().ok()?;
    if bytes.get(13) != Some(&b':')
        || bytes.get(16) != Some(&b':')
        || !valid_timestamp_date(year_number, month, day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let mut cursor = 19;
    let fraction = if bytes.get(cursor) == Some(&b'.') {
        cursor += 1;
        let begin = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        if cursor == begin || cursor - begin > super::core::MAX_NUMBER_DIGITS {
            return None;
        }
        Some(raw[begin..cursor].to_owned())
    } else {
        None
    };
    let offset = match bytes.get(cursor).copied()? {
        b'Z' | b'z' if cursor + 1 == bytes.len() => TimestampOffset::Utc,
        sign @ (b'+' | b'-') if cursor + 6 == bytes.len() && bytes[cursor + 3] == b':' => {
            let hours = raw.get(cursor + 1..cursor + 3)?.parse::<u32>().ok()?;
            let minutes = raw.get(cursor + 4..cursor + 6)?.parse::<u32>().ok()?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            if sign == b'-' && hours == 0 && minutes == 0 {
                TimestampOffset::UnknownLocal
            } else {
                TimestampOffset::Known {
                    negative: sign == b'-',
                    hours,
                    minutes,
                }
            }
        }
        _ => return None,
    };
    if second == 60 && !valid_leap_second(year_number, month, day, hour, minute, &offset) {
        return None;
    }
    Some(TemporalValue::Timestamp {
        year: year.to_owned(),
        month,
        day,
        hour,
        minute,
        second,
        fraction,
        offset,
    })
}

fn valid_timestamp_date(year: u32, month: u32, day: u32) -> bool {
    year <= 9999
        && day >= 1
        && crate::core::calendar::days_in_month(year, month).is_some_and(|maximum| day <= maximum)
}

/// IERS has announced no leap second after 2016 through Bulletin C 72
/// (2026-07). Historical positive leap-second dates follow the NIST table.
/// This versioned snapshot deliberately fails closed until the data is updated.
fn valid_leap_second(
    year: u32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    offset: &TimestampOffset,
) -> bool {
    use chrono::{Datelike, Duration, NaiveDate, Timelike};
    let local_minus_utc_minutes = match offset {
        TimestampOffset::Utc => 0,
        TimestampOffset::Known {
            negative,
            hours,
            minutes,
        } => {
            let absolute = (*hours * 60 + *minutes) as i32;
            if *negative { -absolute } else { absolute }
        }
        // RFC 3339 `-00:00` says the UTC instant is known. It is read as UTC
        // for validating a leap second while retaining the unknown local zone.
        TimestampOffset::UnknownLocal => 0,
    };
    let Some(local) = NaiveDate::from_ymd_opt(year as i32, month, day)
        .and_then(|date| date.and_hms_opt(hour, minute, 59))
    else {
        return false;
    };
    let utc = local - Duration::minutes(i64::from(local_minus_utc_minutes));
    if utc.hour() != 23 || utc.minute() != 59 || utc.second() != 59 {
        return false;
    }
    let date = (utc.year(), utc.month(), utc.day());
    matches!(
        date,
        (1972, 6, 30)
            | (1972, 12, 31)
            | (1973, 12, 31)
            | (1974, 12, 31)
            | (1975, 12, 31)
            | (1976, 12, 31)
            | (1977, 12, 31)
            | (1978, 12, 31)
            | (1979, 12, 31)
            | (1981, 6, 30)
            | (1982, 6, 30)
            | (1983, 6, 30)
            | (1985, 6, 30)
            | (1987, 12, 31)
            | (1989, 12, 31)
            | (1990, 12, 31)
            | (1992, 6, 30)
            | (1993, 6, 30)
            | (1994, 6, 30)
            | (1995, 12, 31)
            | (1997, 6, 30)
            | (1998, 12, 31)
            | (2005, 12, 31)
            | (2008, 12, 31)
            | (2012, 6, 30)
            | (2015, 6, 30)
            | (2016, 12, 31)
    )
}

fn possible_start(text: &str, locale: SpeechLocale) -> bool {
    let first = text.chars().next().unwrap_or(' ');
    if super::core::digit(first).is_some() {
        return true;
    }
    let token = text.split_whitespace().next().unwrap_or("");
    month_number(locale, token.trim_end_matches(',')).is_some()
        || matches!(locale, SpeechLocale::Ja) && text.starts_with(['R', 'H', 'S', '令', '平', '昭'])
        || matches!(locale, SpeechLocale::Ko)
            && (clock_period_prefix(text, locale).0.is_some()
                || ["기원전", "서기"]
                    .iter()
                    .any(|prefix| text.starts_with(prefix)))
        || locale == SpeechLocale::Zh && text.starts_with("公元")
        || locale == SpeechLocale::Ru && {
            let mut tokens = text.split_whitespace();
            tokens
                .next()
                .is_some_and(|token| token.trim_end_matches(',').to_lowercase() == "в")
                && tokens
                    .next()
                    .and_then(russian_prepositional_month)
                    .is_some()
        }
        || locale == SpeechLocale::Vi
            && ["ngày ", "Ngày ", "tháng "]
                .iter()
                .any(|prefix| text.starts_with(prefix))
}

fn invalid(reason: &str) -> TemporalValue {
    TemporalValue::Invalid {
        reason: reason.into(),
    }
}

fn parse_expression(raw: &str, opts: &SpeechOptions) -> Option<TemporalValue> {
    if opts.interpret_as == SpeechInterpretation::Time {
        return parse_clock(raw, opts);
    }
    parse_era(raw, opts.locale)
        .or_else(|| parse_calendar_sequence(raw, opts))
        .or_else(|| {
            if opts.interpret_as != SpeechInterpretation::Date {
                parse_clock(raw, opts)
            } else {
                None
            }
        })
        .or_else(|| parse_date(raw, opts))
}

fn number(raw: &str) -> Option<u32> {
    ascii_digits(raw)?.parse().ok()
}

fn russian_prepositional_month(raw: &str) -> Option<u32> {
    let raw = raw.trim_end_matches('.').to_lowercase();
    RUSSIAN_PREPOSITIONAL_MONTHS
        .iter()
        .position(|name| raw == *name)
        .map(|index| index as u32 + 1)
}

fn month_number(locale: SpeechLocale, raw: &str) -> Option<u32> {
    let raw = raw.trim_end_matches('.').to_lowercase();
    if raw.is_empty() {
        return None;
    }
    let mut abbreviated = Vec::new();
    for m in 1..=12 {
        let name = month(locale, m)?.to_lowercase();
        if raw == name {
            return Some(m);
        }
        if raw.chars().count() >= 3 && name.starts_with(&raw) {
            abbreviated.push(m);
        }
    }
    // Calendar month names use grammatical forms not always present in CLDR's
    // standalone table. These are lexical aliases, not example-specific rows.
    if locale == SpeechLocale::Ru {
        for (i, name) in RUSSIAN_NOMINATIVE_MONTHS.iter().enumerate() {
            if raw == *name {
                return Some(i as u32 + 1);
            }
            if raw.chars().count() >= 3 && name.starts_with(&raw) {
                abbreviated.push(i as u32 + 1);
            }
        }
    }
    abbreviated.sort_unstable();
    abbreviated.dedup();
    (abbreviated.len() == 1).then(|| abbreviated[0])
}

fn day_number(raw: &str) -> Option<u32> {
    let raw = raw.trim_end_matches('.');
    let digits: String = raw
        .chars()
        .take_while(|c| super::core::digit(*c).is_some())
        .collect();
    let n = number(&digits)?;
    let suffix = &raw[digits.len()..];
    if !suffix.is_empty() {
        let expected = match n % 100 {
            11..=13 => "th",
            _ => match n % 10 {
                1 => "st",
                2 => "nd",
                3 => "rd",
                _ => "th",
            },
        };
        if suffix != expected {
            return None;
        }
    }
    Some(n)
}

fn bounded_calendar_components(month: u32, day: u32) -> bool {
    (1..=12).contains(&month) && (1..=31).contains(&day)
}

/// Validate fields independently before checking their Gregorian combination.
/// A yearless February29 is possible; the existing short zero-year convention
/// remains a compatibility detail of ordinary Date, never a literal era guess.
fn calendar_validity(
    year: Option<&str>,
    m: Option<u32>,
    d: Option<u32>,
) -> Result<bool, &'static str> {
    if m.is_some_and(|m| !(1..=12).contains(&m)) || d.is_some_and(|d| !(1..=31).contains(&d)) {
        return Err("invalid calendar date");
    }
    let calendar_year = match year {
        Some(raw) => match raw.parse::<u32>() {
            Ok(0) if raw.len() <= 2 => None,
            Ok(y) if (1..=9999).contains(&y) => Some(y),
            _ => return Err("invalid calendar year"),
        },
        None => None,
    };
    Ok(match (m, d) {
        (Some(m), Some(d)) => valid_date(calendar_year.unwrap_or(2000), m, d),
        _ => true,
    })
}

fn literal_invalid_calendar(year: Option<&str>, month: u32, day: u32) -> bool {
    bounded_calendar_components(month, day)
        && year.is_none_or(|raw| {
            !raw.is_empty()
                && raw.len() <= 4
                && raw.bytes().all(|c| c.is_ascii_digit())
                && raw.parse::<u32>().is_ok_and(|y| (1..=9999).contains(&y))
        })
        && calendar_validity(year, Some(month), Some(day)) == Ok(false)
}

fn date_value_with_policy(
    year: Option<String>,
    m: Option<u32>,
    d: Option<u32>,
    presentation: DatePresentation,
    connector: Option<String>,
    year_label: bool,
    policy: InvalidDatePolicy,
) -> TemporalValue {
    match calendar_validity(year.as_deref(), m, d) {
        Err(reason) => return invalid(reason),
        Ok(false) => {
            if policy == InvalidDatePolicy::ReadComponents
                && let (Some(month), Some(day)) = (m, d)
                && literal_invalid_calendar(year.as_deref(), month, day)
            {
                return TemporalValue::InvalidCalendarDate {
                    year,
                    month,
                    day,
                    presentation,
                    connector,
                    year_label,
                };
            }
            return invalid("invalid calendar date");
        }
        Ok(true) => {}
    }
    TemporalValue::Date {
        year,
        month: m,
        day: d,
        presentation,
        connector,
        year_label,
    }
}

fn date_value(
    year: Option<String>,
    m: Option<u32>,
    d: Option<u32>,
    presentation: DatePresentation,
    connector: Option<String>,
    year_label: bool,
) -> TemporalValue {
    date_value_with_policy(
        year,
        m,
        d,
        presentation,
        connector,
        year_label,
        InvalidDatePolicy::Preserve,
    )
}

fn parsed_date_value(
    year: Option<String>,
    m: Option<u32>,
    d: Option<u32>,
    presentation: DatePresentation,
    connector: Option<String>,
    year_label: bool,
    opts: &SpeechOptions,
) -> TemporalValue {
    date_value_with_policy(
        year,
        m,
        d,
        presentation,
        connector,
        year_label,
        opts.invalid_date_policy,
    )
}

fn strip_russian_year_label(raw: &str) -> Option<&str> {
    ["г.", " года"].iter().find_map(|suffix| {
        let start = raw.len().checked_sub(suffix.len())?;
        (raw.get(start..)?.to_lowercase() == *suffix).then(|| raw[..start].trim_end())
    })
}

fn month_year_connector(locale: SpeechLocale, connector: &str) -> Option<&'static str> {
    if connector.eq_ignore_ascii_case("de") {
        Some("de")
    } else if connector.eq_ignore_ascii_case("del") {
        Some("del")
    } else if locale == SpeechLocale::En && connector.eq_ignore_ascii_case("of") {
        Some("of")
    } else {
        None
    }
}

fn parse_date(raw: &str, opts: &SpeechOptions) -> Option<TemporalValue> {
    let locale = opts.locale;
    if opts.interpret_as == SpeechInterpretation::Date
        && let Some(year) = ascii_digits(raw)
        && year.len() == 4
    {
        return Some(parsed_date_value(
            Some(year),
            None,
            None,
            DatePresentation::Component,
            None,
            false,
            opts,
        ));
    }
    if matches!(locale, SpeechLocale::En | SpeechLocale::Fr) && raw.ends_with('s') {
        if opts.interpret_as != SpeechInterpretation::Date {
            return None;
        }
        let stem = raw[..raw.len() - 1].trim_end();
        if let Some(year) = number(stem)
            && (stem.len() == 2 || (1000..=9990).contains(&year))
            && year.is_multiple_of(10)
        {
            return Some(TemporalValue::Decade { year });
        }
    }
    if matches!(
        locale,
        SpeechLocale::Zh | SpeechLocale::Ja | SpeechLocale::Ko
    ) && let Some(value) = parse_east_asian_date(raw, opts)
    {
        return Some(value);
    }
    if locale == SpeechLocale::Vi {
        if let Some(value) = parse_vietnamese_date(raw, opts) {
            return Some(value);
        }
        if let Some(body) = raw
            .strip_prefix("ngày ")
            .or_else(|| raw.strip_prefix("Ngày "))
        {
            // The written day label is part of this source span even in
            // Precise mode; leaving it as prose would render "ngày ngày…".
            let mut value = parse_numeric_date_impl(body, opts, false, false)
                .or_else(|| parse_date(body, opts))?;
            match &mut value {
                TemporalValue::Date { presentation, .. }
                | TemporalValue::InvalidCalendarDate { presentation, .. } => {
                    *presentation = DatePresentation::Component;
                }
                _ => {}
            }
            return Some(value);
        }
    }
    let (body, year_label) = if locale == SpeechLocale::Ru {
        strip_russian_year_label(raw)
            .map(|s| (s.trim_end(), true))
            .unwrap_or((raw, false))
    } else {
        (raw, false)
    };
    // Named dates accept optional grammatical connectors and commas. Numeric
    // widths remain explicit and invalid named dates are preserved as a whole.
    let normalized = body.replace([',', '-'], " ");
    let words: Vec<&str> = normalized.split_whitespace().collect();
    if locale == SpeechLocale::Ru
        && words.len() == 3
        && words[0].to_lowercase() == "в"
        && let Some(m) = russian_prepositional_month(words[1])
        && let Some(year) = ascii_digits(words[2]).filter(|year| year.len() == 4)
    {
        return Some(parsed_date_value(
            Some(year),
            Some(m),
            None,
            DatePresentation::MonthFirst,
            Some("в".into()),
            year_label,
            opts,
        ));
    }
    if let Some((index, m)) = words
        .iter()
        .enumerate()
        .find_map(|(i, w)| month_number(locale, w).map(|m| (i, m)))
    {
        if index == 1 && words[0].contains('/') && (2..=3).contains(&words.len()) {
            let days: Vec<u32> = words[0].split('/').map(day_number).collect::<Option<_>>()?;
            let year = words.get(2).map(|s| ascii_digits(s)).transpose_option()?;
            if days.len() < 2 || days.len() > 31 {
                return Some(invalid("invalid calendar day list"));
            }
            if days.iter().any(|d| {
                matches!(
                    date_value(
                        year.clone(),
                        Some(m),
                        Some(*d),
                        DatePresentation::DayFirst,
                        None,
                        false
                    ),
                    TemporalValue::Invalid { .. }
                )
            }) {
                return Some(invalid("invalid calendar date in day list"));
            }
            return Some(TemporalValue::DateList {
                days,
                month: m,
                year,
            });
        }
        if index == 0 {
            // Month-year partial dates (`May 2024`, `mayo de 2024`): the day
            // is absent, not unknown. Four-digit years only — shorter numbers
            // after a month name are ordinals or days, never years. Width is
            // decimal-digit count after Unicode folding, never UTF-8 bytes.
            let four_digit_year = |raw: &str| ascii_digits(raw).filter(|year| year.len() == 4);
            let month_year = match &words[1..] {
                [year] => four_digit_year(year).map(|y| (None, y)),
                [connector, year] => month_year_connector(locale, connector)
                    .and_then(|c| four_digit_year(year).map(|y| (Some(c.into()), y))),
                _ => None,
            };
            if let Some((connector, year)) = month_year {
                return Some(parsed_date_value(
                    Some(year),
                    Some(m),
                    None,
                    DatePresentation::MonthFirst,
                    connector,
                    year_label,
                    opts,
                ));
            }
        }
        if index == 0 && (2..=3).contains(&words.len()) {
            let day = day_number(words[1])?;
            let year = words.get(2).map(|s| ascii_digits(s)).transpose_option()?;
            return Some(parsed_date_value(
                year,
                Some(m),
                Some(day),
                DatePresentation::MonthFirst,
                None,
                year_label,
                opts,
            ));
        }
        if index == 1 || (index == 2 && words[1] == "de") {
            let day = day_number(words[0])?;
            let after = &words[index + 1..];
            let (connector, year) = match after {
                [] => (None, None),
                [year] => (None, Some(ascii_digits(year)?)),
                [connector @ ("de" | "del"), year] => {
                    (Some((*connector).into()), Some(ascii_digits(year)?))
                }
                _ => return None,
            };
            return Some(parsed_date_value(
                year,
                Some(m),
                Some(day),
                DatePresentation::DayFirst,
                connector,
                year_label,
                opts,
            ));
        }
    }
    parse_numeric_date_impl(body, opts, year_label, true)
}

/// Numeric date grammar used by both the structural front end and named-date parser.
pub(super) fn parse_numeric_date(raw: &str, opts: &SpeechOptions) -> Option<TemporalValue> {
    parse_numeric_date_impl(raw, opts, false, false)
}

fn resolve_day_month(
    a: u32,
    b: u32,
    year: u32,
    opts: &SpeechOptions,
) -> Result<(u32, u32), &'static str> {
    let dmy = valid_date(year, b, a);
    let mdy = valid_date(year, a, b);
    match opts.date_order {
        DateOrder::DayMonthYear => Ok((b, a)),
        DateOrder::MonthDayYear => Ok((a, b)),
        DateOrder::RejectAmbiguous if dmy && (!mdy || a == b) => Ok((b, a)),
        DateOrder::RejectAmbiguous if mdy && !dmy => Ok((a, b)),
        DateOrder::RejectAmbiguous if dmy && mdy => Err("date order is ambiguous; set date_order"),
        DateOrder::RejectAmbiguous
            if opts.invalid_date_policy == InvalidDatePolicy::ReadComponents =>
        {
            match (
                bounded_calendar_components(b, a),
                bounded_calendar_components(a, b),
            ) {
                (true, false) => Ok((b, a)),
                (false, true) => Ok((a, b)),
                _ => Err("invalid calendar date"),
            }
        }
        _ => Err("invalid calendar date"),
    }
}

fn parse_numeric_date_impl(
    body: &str,
    opts: &SpeechOptions,
    year_label: bool,
    legacy_dispatch: bool,
) -> Option<TemporalValue> {
    let separator = if body.contains('/') {
        '/'
    } else if body.contains('-') {
        '-'
    } else if body.contains('.') {
        '.'
    } else {
        return None;
    };
    let pieces: Vec<_> = body.split(separator).collect();
    if !(2..=3).contains(&pieces.len()) || pieces.iter().any(|p| p.is_empty()) {
        return None;
    }
    let digits: Vec<String> = pieces
        .iter()
        .map(|p| ascii_digits(p))
        .collect::<Option<_>>()?;
    if opts.interpret_as != SpeechInterpretation::Date
        && pieces.len() == 3
        && digits[0].len() != 4
        && digits[2].len() != 4
    {
        return if legacy_dispatch || separator == '.' {
            None
        } else {
            Some(invalid(
                "date needs a four-digit year and one- or two-digit day/month",
            ))
        };
    }
    if legacy_dispatch
        && opts.reading_style == ReadingStyle::Precise
        && opts.number_notation == NumberNotation::Phonetic
        && pieces.len() == 3
        && separator != '.'
        && !year_label
        && (digits[0].len() == 4 || digits[2].len() == 4)
    {
        return None;
    }
    let values: Vec<u32> = digits
        .iter()
        .map(|p| p.parse().ok())
        .collect::<Option<_>>()?;
    // Partial forms require explicit calendar intent; no locale silently
    // changes a decimal/range into a date.
    if pieces.len() == 2 {
        if opts.interpret_as != SpeechInterpretation::Date {
            return None;
        }
        if digits[0].len() == 4 {
            return Some(parsed_date_value(
                Some(digits[0].clone()),
                Some(values[1]),
                None,
                DatePresentation::Numeric,
                None,
                year_label,
                opts,
            ));
        }
        if digits[1].len() == 4 {
            if digits[0].len() > 2 {
                return Some(invalid("invalid partial calendar date width"));
            }
            return Some(parsed_date_value(
                Some(digits[1].clone()),
                Some(values[0]),
                None,
                DatePresentation::Numeric,
                None,
                year_label,
                opts,
            ));
        }
        if opts.date_order == DateOrder::YearMonthDay {
            return Some(parsed_date_value(
                Some(digits[0].clone()),
                Some(values[1]),
                None,
                DatePresentation::Numeric,
                None,
                year_label,
                opts,
            ));
        }
        if digits.iter().any(|s| s.len() > 2) {
            return Some(invalid("invalid partial calendar date width"));
        }
        let (a, b) = (values[0], values[1]);
        let (m, d) = match resolve_day_month(a, b, 2000, opts) {
            Ok(fields) => fields,
            Err(reason) => return Some(invalid(reason)),
        };
        return Some(parsed_date_value(
            None,
            Some(m),
            Some(d),
            DatePresentation::Numeric,
            None,
            year_label,
            opts,
        ));
    }
    if digits.iter().any(|s| s.len() > 4) {
        return Some(invalid("invalid calendar date width"));
    }
    let (year, m, d) = if digits[0].len() == 4 || opts.date_order == DateOrder::YearMonthDay {
        if digits[1].len() > 2 || digits[2].len() > 2 {
            return Some(invalid("invalid calendar date width"));
        }
        (digits[0].clone(), values[1], values[2])
    } else {
        if digits[0].len() > 2 || digits[1].len() > 2 || !matches!(digits[2].len(), 2 | 4) {
            return Some(invalid("invalid calendar date width"));
        }
        let a = values[0];
        let b = values[1];
        let y = values[2];
        let (m, d) = match resolve_day_month(a, b, if y == 0 { 2000 } else { y }, opts) {
            Ok(fields) => fields,
            Err(reason) => return Some(invalid(reason)),
        };
        (digits[2].clone(), m, d)
    };
    Some(parsed_date_value(
        Some(year),
        Some(m),
        Some(d),
        DatePresentation::Numeric,
        None,
        year_label,
        opts,
    ))
}

trait TransposeOption<T> {
    fn transpose_option(self) -> Option<Option<T>>;
}
impl<T> TransposeOption<T> for Option<Option<T>> {
    fn transpose_option(self) -> Option<Option<T>> {
        match self {
            None => Some(None),
            Some(v) => v.map(Some),
        }
    }
}

fn parse_east_asian_date(raw: &str, opts: &SpeechOptions) -> Option<TemporalValue> {
    let locale = opts.locale;
    let markers = if locale == SpeechLocale::Ko {
        ['년', '월', '일']
    } else {
        ['年', '月', '日']
    };
    let mut rest = raw;
    let mut year = None;
    let mut m = None;
    let mut d = None;
    for (index, marker) in markers.into_iter().enumerate() {
        if let Some((prefix, suffix)) = rest.split_once(marker) {
            let digits = ascii_digits(prefix.trim())?;
            match index {
                0 => year = Some(digits),
                1 => m = Some(digits.parse().ok()?),
                _ => d = Some(digits.parse().ok()?),
            };
            rest = suffix;
        }
    }
    if locale == SpeechLocale::Zh
        && let Some((prefix, suffix)) = rest.split_once('号')
    {
        d = Some(number(prefix)?);
        rest = suffix;
    }
    if !rest.trim().is_empty() || year.is_none() && m.is_none() && d.is_none() {
        return None;
    }
    Some(parsed_date_value(
        year,
        m,
        d,
        DatePresentation::Component,
        None,
        false,
        opts,
    ))
}

fn parse_calendar_sequence(raw: &str, opts: &SpeechOptions) -> Option<TemporalValue> {
    if !matches!(
        opts.locale,
        SpeechLocale::Ja | SpeechLocale::Zh | SpeechLocale::Ko
    ) || !raw.chars().any(|c| matches!(c, '(' | '（' | '〜' | '～'))
    {
        return None;
    }
    let mut cursor = 0;
    let mut items = Vec::new();
    let mut saw_structure = false;
    while cursor < raw.len() {
        let rest = &raw[cursor..];
        if rest.starts_with(['〜', '～']) {
            if items.is_empty() || matches!(items.last(), Some(CalendarItem::Range)) {
                return None;
            }
            cursor += rest.chars().next()?.len_utf8();
            items.push(CalendarItem::Range);
            saw_structure = true;
            continue;
        }
        if rest.starts_with(['(', '（']) {
            if !matches!(items.last(), Some(CalendarItem::Date { .. })) {
                return None;
            }
            let close = if rest.starts_with('（') { '）' } else { ')' };
            let opening = rest.chars().next()?.len_utf8();
            let end = rest.find(close)?;
            let word = &rest[opening..end];
            let day = match opts.locale {
                SpeechLocale::Ja => ["月", "火", "水", "木", "金", "土", "日"]
                    .iter()
                    .position(|v| word == *v || word == format!("{v}曜日")),
                SpeechLocale::Zh => ["一", "二", "三", "四", "五", "六", "日"]
                    .iter()
                    .position(|v| word == *v || word == format!("星期{v}")),
                SpeechLocale::Ko => ["월", "화", "수", "목", "금", "토", "일"]
                    .iter()
                    .position(|v| word == *v || word == format!("{v}요일")),
                _ => None,
            }?;
            if let Some(CalendarItem::Date { value }) = items.last()
                && let TemporalValue::Date {
                    year: Some(year),
                    month: Some(m),
                    day: Some(d),
                    ..
                } = value.as_ref()
            {
                use chrono::Datelike;
                if chrono::NaiveDate::from_ymd_opt(year.parse().ok()?, *m, *d)?
                    .weekday()
                    .num_days_from_monday()
                    != day as u32
                {
                    return Some(invalid("weekday does not match calendar date"));
                }
            }
            items.push(CalendarItem::Weekday { index: day as u8 });
            cursor += end + close.len_utf8();
            saw_structure = true;
            continue;
        }
        let end = rest.find(['(', '（', '〜', '～']).unwrap_or(rest.len());
        let value = parse_east_asian_date(&rest[..end], opts)?;
        if matches!(value, TemporalValue::Invalid { .. }) {
            return Some(value);
        }
        if matches!(value, TemporalValue::InvalidCalendarDate { .. }) {
            return Some(invalid("invalid calendar date"));
        }
        items.push(CalendarItem::Date {
            value: Box::new(value),
        });
        cursor += end;
    }
    if !saw_structure || matches!(items.last(), Some(CalendarItem::Range)) {
        return None;
    }
    Some(TemporalValue::CalendarSequence { items })
}

fn parse_vietnamese_date(raw: &str, opts: &SpeechOptions) -> Option<TemporalValue> {
    let words: Vec<_> = raw.split_whitespace().collect();
    match words.as_slice() {
        ["tháng", m] => Some(parsed_date_value(
            None,
            Some(number(m)?),
            None,
            DatePresentation::Component,
            None,
            false,
            opts,
        )),
        ["ngày" | "Ngày", d] => Some(parsed_date_value(
            None,
            None,
            Some(number(d)?),
            DatePresentation::Component,
            None,
            false,
            opts,
        )),
        ["tháng", m, year] => Some(parsed_date_value(
            Some(ascii_digits(year)?),
            Some(number(m)?),
            None,
            DatePresentation::Component,
            None,
            false,
            opts,
        )),
        ["tháng", m, "năm", year] => Some(parsed_date_value(
            Some(ascii_digits(year)?),
            Some(number(m)?),
            None,
            DatePresentation::Component,
            None,
            false,
            opts,
        )),
        ["ngày" | "Ngày", d, "tháng", m, "năm", year] => Some(parsed_date_value(
            Some(ascii_digits(year)?),
            Some(number(m)?),
            Some(number(d)?),
            DatePresentation::Component,
            None,
            false,
            opts,
        )),
        _ => None,
    }
}

/// Named era years are positive ordinals, not astronomical year numbers.
fn positive_era_year(year: &str) -> Option<u32> {
    number(year).filter(|year| *year > 0)
}

fn parse_era(raw: &str, locale: SpeechLocale) -> Option<TemporalValue> {
    if locale == SpeechLocale::Ja {
        for (prefix, era) in [
            ("R.", "令和"),
            ("H.", "平成"),
            ("S.", "昭和"),
            ("令和", "令和"),
            ("平成", "平成"),
            ("昭和", "昭和"),
        ] {
            if let Some(year) = raw.strip_prefix(prefix) {
                let year = ascii_digits(year.trim_end_matches('年'))?;
                if positive_era_year(&year).is_none() {
                    return Some(invalid("era year must be positive"));
                }
                return Some(TemporalValue::EraYear {
                    era: era.into(),
                    year,
                });
            }
        }
    }
    for (prefix, suffix) in match locale {
        SpeechLocale::Ko => [("기원전", "년"), ("서기", "년")],
        SpeechLocale::Zh => [("公元前", "年"), ("公元", "年")],
        _ => return None,
    } {
        if let Some(year) = raw
            .strip_prefix(prefix)
            .and_then(|s| s.trim().strip_suffix(suffix))
        {
            let year = ascii_digits(year)?;
            if positive_era_year(&year).is_none() {
                return Some(invalid("era year must be positive"));
            }
            return Some(TemporalValue::EraYear {
                era: prefix.into(),
                year,
            });
        }
    }
    None
}

fn parse_clock(raw: &str, opts: &SpeechOptions) -> Option<TemporalValue> {
    if opts.reading_style == ReadingStyle::Precise
        && opts.number_notation == NumberNotation::Phonetic
        && raw.contains(':')
        && !raw
            .chars()
            .any(|c| c.is_alphabetic() && !matches!(c, 'a' | 'p' | 'm' | 'A' | 'P' | 'M'))
    {
        return None;
    }
    parse_clock_impl(raw, opts, false)
}

/// Decimal seconds are a clock profile, not an RFC3339 timestamp grammar.
/// Inspect the complete numeric candidate before a shorter clock can succeed.
fn parse_fractional_clock_at(
    text: &str,
    start: usize,
    opts: &SpeechOptions,
) -> Option<(usize, TemporalValue)> {
    let (period, numeric_body) = clock_period_prefix(text.get(start..)?, opts.locale);
    let numeric_start = text.len() - numeric_body.len();
    let numeric_end = super::core::scan_numeric_end(text, numeric_start);
    let raw = text.get(numeric_start..numeric_end)?;
    let (separator, marker) = raw
        .char_indices()
        .find(|(_, c)| matches!(c, '.' | ','))
        .or_else(|| {
            // The shared numeric scanner removes sentence-final separators.
            // A following ASCII suffix proves this is not a plain final stop:
            // `03:15:01.Z` must not become a successful shorter clock.
            let marker = text[numeric_end..].chars().next()?;
            let next = text[numeric_end + marker.len_utf8()..].chars().next()?;
            (matches!(marker, '.' | ',') && next.is_ascii_alphabetic())
                .then_some((raw.len(), marker))
        })?;
    if marker == ',' && opts.interpret_as != SpeechInterpretation::Time {
        return None;
    }
    let prefix = &raw[..separator];
    if prefix.chars().filter(|c| *c == ':').count() != 2
        || !prefix
            .chars()
            .all(|c| c == ':' || super::core::digit(c).is_some())
    {
        return None;
    }
    let fraction_start = numeric_start + separator + marker.len_utf8();
    let fraction_end = fraction_start
        + text[fraction_start..]
            .char_indices()
            .take_while(|(_, c)| super::core::digit(*c).is_some())
            .map(|(i, c)| i + c.len_utf8())
            .last()
            .unwrap_or(0);
    let token_end = super::parser::token_end(text, numeric_start)
        .max(super::parser::token_end(text, numeric_end))
        .max(numeric_end);
    // The normal parser retains these fields as identifiers. In particular a
    // clock-looking email address cannot become a supported numeric prefix.
    if super::lexer::is_protected_identifier(&text[start..token_end]) {
        return None;
    }
    let suffix_end = numeric_time_suffix_end(text, fraction_end);
    let continuation = text[suffix_end..].chars().next().is_some_and(|c| {
        super::core::is_word(c)
            || matches!(c, '+' | '-' | '−' | ':' | '/')
            || matches!(c, '.' | ',')
                && text[suffix_end + c.len_utf8()..]
                    .chars()
                    .next()
                    .is_some_and(|next| next.is_alphanumeric())
    });
    if fraction_end != numeric_end || continuation {
        return Some((
            token_end.max(suffix_end),
            invalid("unsupported fractional clock suffix"),
        ));
    }
    if let Some(end) = fractional_zone_suffix_end(text, suffix_end) {
        return Some((end, invalid("fractional clock timezone is not supported")));
    }
    let fraction = text[fraction_start..fraction_end]
        .chars()
        .take(super::core::MAX_NUMBER_DIGITS + 1)
        .map(super::core::digit)
        .collect::<Option<String>>()?;
    if !valid_fraction(&fraction) {
        return Some((
            token_end.max(suffix_end),
            invalid("invalid fractional-second precision"),
        ));
    }
    if prefix.chars().count() > 8 {
        return Some((suffix_end, invalid("invalid time width or digits")));
    }
    // Reuse the established clock decoder, including its source-period policy.
    // Only the bounded suffix is copied; source offsets still refer to text.
    let mut body = String::new();
    if let Some(period) = period {
        body.push_str(period);
        body.push(' ');
    }
    body.push_str(prefix);
    let clock_end = body.len();
    if suffix_end > fraction_end {
        body.push(' ');
        body.push_str(&text[super::core::skip_space(text, fraction_end)..suffix_end]);
    }
    let (_, clock) = parse_numeric_time(&body, 0, clock_end, opts);
    match clock {
        TemporalValue::Clock {
            minute: Some(_),
            second: Some(_),
            half: false,
            ..
        } => Some((
            suffix_end,
            TemporalValue::FractionalClock {
                clock: Box::new(clock),
                fraction,
            },
        )),
        TemporalValue::Invalid { .. } => Some((suffix_end, clock)),
        _ => Some((
            suffix_end,
            invalid("fractional seconds require a complete clock"),
        )),
    }
}

/// Recognize only explicit unsupported zone forms, without assigning a zone
/// abbreviation, offset value, or geographical time zone to ordinary prose.
fn fractional_zone_suffix_end(text: &str, end: usize) -> Option<usize> {
    let start = super::core::skip_space(text, end);
    if start == end {
        return None;
    }
    let zone_end = super::parser::token_end(text, start);
    let zone = &text[start..zone_end];
    let utc = zone.get(..3).is_some_and(|s| s.eq_ignore_ascii_case("UTC"))
        && zone[3..]
            .chars()
            .next()
            .is_none_or(|c| matches!(c, '+' | '-' | '−'));
    let offset = zone.starts_with(['+', '-', '−'])
        && zone.contains(':')
        && zone
            .chars()
            .skip(1)
            .all(|c| c == ':' || super::core::digit(c).is_some());
    (zone.eq_ignore_ascii_case("Z") || utc || offset).then_some(zone_end)
}

fn valid_fraction(fraction: &str) -> bool {
    !fraction.is_empty()
        && fraction.len() <= super::core::MAX_NUMBER_DIGITS
        && fraction.bytes().all(|c| c.is_ascii_digit())
}

/// Parse an already isolated numeric clock and its optional AM/PM suffix.
/// Bounds are validated before producing semantic fields; the legacy Time IR
/// permits 00:00..23:59:59, while explicitly labelled clocks can express 24:00.
pub(super) fn parse_numeric_time(
    text: &str,
    start: usize,
    end: usize,
    opts: &SpeechOptions,
) -> (usize, TemporalValue) {
    let Some(_) = text.get(start..end) else {
        return (end, invalid("invalid clock span"));
    };
    let final_end = numeric_time_suffix_end(text, end);
    let parsed = parse_clock_impl(&text[start..final_end], opts, true)
        .unwrap_or_else(|| invalid("invalid time width or digits"));
    (final_end, parsed)
}

fn numeric_time_suffix_end(text: &str, end: usize) -> usize {
    let suffix = super::core::skip_space(text, end);
    let rest = &text[suffix..];
    for marker in ["a.m.", "p.m.", "a.m", "p.m", "am", "pm"] {
        if rest
            .get(..marker.len())
            .is_some_and(|s| s.eq_ignore_ascii_case(marker))
            && rest[marker.len()..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric() && c != '_')
        {
            return suffix + marker.len();
        }
    }
    end
}

/// Decode a written hour without changing the meaning of its explicit period.
/// The tolerant profile normalizes only coherent, redundant 24h notation;
/// semantic Clock values always store the ordinary 1..12 hour with AM/PM.
fn decode_clock_hour(hour: u32, meridiem: Option<&str>, policy: ClockPolicy) -> Option<u32> {
    match meridiem {
        None => Some(hour),
        Some("am" | "pm") if (1..=12).contains(&hour) => Some(hour),
        Some("pm")
            if policy == ClockPolicy::NormalizeRedundantPeriod && (13..=23).contains(&hour) =>
        {
            Some(hour - 12)
        }
        Some("am") if policy == ClockPolicy::NormalizeRedundantPeriod && hour == 0 => Some(12),
        _ => None,
    }
}

/// Canonical 24-hour value for a validated optional 12-hour period.
/// Shared by compact IR and renderers that do not verbalize AM/PM labels.
fn clock_hour_24(hour: u32, meridiem: Option<&str>) -> Option<u32> {
    let hour = decode_clock_hour(hour, meridiem, ClockPolicy::Strict)?;
    match meridiem {
        Some("am") => Some(hour % 12),
        Some("pm") => Some(hour % 12 + 12),
        None => Some(hour),
        _ => None,
    }
}

fn effective_clock_meridiem<'a>(
    period: Option<&str>,
    meridiem: Option<&'a str>,
    locale: SpeechLocale,
) -> Option<Option<&'a str>> {
    let prefix_marker = if let Some(period) = period {
        if clock_period_prefix(period, locale).0 != Some(period) {
            return None;
        }
        match period {
            "오전" => Some("am"),
            "오후" => Some("pm"),
            // Broad day periods retain their lexical meaning without assigning
            // guessed hour ranges or an implicit AM/PM interpretation.
            _ => None,
        }
    } else {
        None
    };
    if prefix_marker.is_some() && meridiem.is_some() && prefix_marker != meridiem {
        return None;
    }
    Some(meridiem.or(prefix_marker))
}

/// One recognizer serves ordinary and fractional clocks, including attached
/// prefixes. Broad day-period words do not imply newly guessed hour ranges.
fn clock_period_prefix(body: &str, locale: SpeechLocale) -> (Option<&str>, &str) {
    if locale == SpeechLocale::Ko {
        for label in ["오전", "오후", "새벽", "아침", "저녁", "밤"] {
            if let Some(rest) = body.strip_prefix(label) {
                return (Some(label), rest.trim_start());
            }
        }
    }
    (None, body)
}

fn parse_clock_impl(
    raw: &str,
    opts: &SpeechOptions,
    legacy_validation: bool,
) -> Option<TemporalValue> {
    let locale = opts.locale;
    let (period, mut body) = clock_period_prefix(raw.trim(), locale);
    let period = period.map(str::to_owned);
    let mut meridiem = None;
    let lower = body.to_ascii_lowercase();
    for marker in ["a.m.", "p.m.", "a.m", "p.m", "am", "pm"] {
        if lower.ends_with(marker) {
            body = body[..body.len() - marker.len()].trim_end();
            meridiem = Some(if marker.starts_with('p') { "pm" } else { "am" }.to_string());
            break;
        }
    }
    if body.contains('.') && meridiem.is_none() && opts.interpret_as != SpeechInterpretation::Time {
        return None;
    }
    if locale == SpeechLocale::De
        && let Some(unlabelled) = body
            .strip_suffix("Uhr")
            .or_else(|| body.strip_suffix("uhr"))
    {
        body = unlabelled.trim_end();
    }
    if body.contains('h') && opts.interpret_as != SpeechInterpretation::Time {
        return None;
    }
    let had_h = body.ends_with('h');
    if had_h {
        body = body[..body.len() - 1].trim_end();
    }
    // Spaced units retain their quantity interpretation, e.g. `3 h`.
    if had_h && number(body).is_some() && opts.interpret_as != SpeechInterpretation::Time {
        return None;
    }
    let half = body.ends_with('반');
    if half {
        body = body[..body.len() - '반'.len_utf8()].trim_end();
    }
    let markers = match locale {
        SpeechLocale::Ja => Some(['時', '分', '秒']),
        SpeechLocale::Ko => Some(['시', '분', '초']),
        _ => None,
    };
    let (hour, minute, second) = if let Some(markers) = markers.filter(|m| body.contains(m[0])) {
        let (hour, tail) = body.split_once(markers[0])?;
        let hour = number(hour.trim())?;
        let mut rest = tail.trim();
        let mut minute = None;
        let mut second = None;
        if let Some((n, tail)) = rest.split_once(markers[1]) {
            minute = Some(number(n.trim())?);
            rest = tail.trim();
        }
        if let Some((n, tail)) = rest.split_once(markers[2]) {
            second = Some(number(n.trim())?);
            rest = tail.trim();
        }
        if !rest.is_empty() {
            return None;
        }
        (hour, minute, second)
    } else {
        let separator = if body.contains(':') {
            ':'
        } else if body.contains('h') {
            'h'
        } else if body.contains('.')
            && matches!(
                locale,
                SpeechLocale::En | SpeechLocale::Es | SpeechLocale::Pt
            )
        {
            '.'
        } else {
            '\0'
        };
        if separator == '\0' {
            if !had_h && meridiem.is_none() && period.is_none() {
                return None;
            }
            (number(body)?, None, None)
        } else {
            let pieces: Vec<_> = body.split(separator).collect();
            if !(2..=3).contains(&pieces.len()) || pieces[0].chars().count() > 2 {
                return None;
            }
            if pieces.iter().skip(1).any(|p| p.chars().count() != 2) {
                return None;
            }
            let ns: Vec<_> = pieces.iter().map(|s| number(s)).collect::<Option<_>>()?;
            (ns[0], Some(ns[1]), ns.get(2).copied())
        }
    };
    if (legacy_validation && hour > 23)
        || hour > 24
        || minute.is_some_and(|m| m > 59)
        || second.is_some_and(|s| s > 59)
        || hour == 24 && (minute.unwrap_or(0) > 0 || second.unwrap_or(0) > 0)
    {
        return Some(invalid("invalid clock time"));
    }
    let Some(effective_meridiem) =
        effective_clock_meridiem(period.as_deref(), meridiem.as_deref(), locale)
    else {
        return Some(invalid("contradictory clock periods"));
    };
    let Some(hour) = decode_clock_hour(hour, effective_meridiem, opts.clock_policy) else {
        return Some(invalid("invalid twelve-hour clock time"));
    };
    Some(TemporalValue::Clock {
        hour,
        minute,
        second,
        period,
        meridiem,
        half,
    })
}

fn cardinal(locale: SpeechLocale, n: u32) -> Option<String> {
    verbalize::cardinal(locale, n)
}
fn digits(locale: SpeechLocale, raw: &str) -> Option<String> {
    verbalize_number(
        &NumberValue {
            negative: false,
            integer: raw.into(),
            fraction: None,
        },
        locale,
        NumberReading::Digits,
    )
}

fn orthographic_number(locale: SpeechLocale, n: u32, opts: &SpeechOptions) -> Option<String> {
    super::numbers::verbalize_number_with_options(
        &NumberValue {
            negative: false,
            integer: n.to_string(),
            fraction: None,
        },
        locale,
        NumberReading::Cardinal,
        opts.reading_style,
        opts.number_notation,
    )
}

/// Render existing complete-date IR through the same grammar as named dates.
pub fn render_date(
    year: u32,
    m: u32,
    d: u32,
    locale: SpeechLocale,
    opts: &SpeechOptions,
) -> Option<String> {
    if !valid_date(year, m, d) {
        return None;
    }
    if opts.reading_style == ReadingStyle::Precise
        && opts.number_notation == NumberNotation::Phonetic
        && let Some(result) = verbalize::date(locale, year, m, d)
    {
        return Some(result);
    }
    render(
        &TemporalValue::Date {
            year: Some(year.to_string()),
            month: Some(m),
            day: Some(d),
            presentation: DatePresentation::Numeric,
            connector: None,
            year_label: false,
        },
        locale,
        opts,
    )
}

/// Render existing clock IR through the same grammar as localized clocks.
pub fn render_time(
    hour: u32,
    minute: u32,
    second: Option<u32>,
    locale: SpeechLocale,
    opts: &SpeechOptions,
) -> Option<String> {
    if hour > 23 || minute > 59 || second.is_some_and(|s| s > 59) {
        return None;
    }
    if opts.reading_style == ReadingStyle::Precise
        && opts.number_notation == NumberNotation::Phonetic
        && locale != SpeechLocale::De
        && let Some(result) = verbalize::time(locale, hour, Some(minute), second)
    {
        return Some(result);
    }
    render(
        &TemporalValue::Clock {
            hour,
            minute: Some(minute),
            second,
            period: None,
            meridiem: None,
            half: false,
        },
        locale,
        opts,
    )
}

pub fn render(value: &TemporalValue, locale: SpeechLocale, opts: &SpeechOptions) -> Option<String> {
    match value {
        TemporalValue::Invalid { .. } => None,
        TemporalValue::InvalidCalendarDate {
            year,
            month,
            day,
            presentation,
            connector,
            year_label,
        } => {
            if opts.invalid_date_policy != InvalidDatePolicy::ReadComponents
                || !literal_invalid_calendar(year.as_deref(), *month, *day)
            {
                return None;
            }
            format_calendar_components(
                year.as_deref(),
                Some(*month),
                Some(*day),
                presentation,
                connector.as_deref(),
                *year_label,
                locale,
                opts,
            )
        }
        TemporalValue::Date {
            year,
            month,
            day,
            presentation,
            connector,
            year_label,
        } => render_calendar(
            year.as_deref(),
            *month,
            *day,
            presentation,
            connector.as_deref(),
            *year_label,
            locale,
            opts,
        ),
        TemporalValue::Clock {
            hour,
            minute,
            second,
            period,
            meridiem,
            half,
        } => render_clock(
            *hour,
            *minute,
            *second,
            None,
            period.as_deref(),
            meridiem.as_deref(),
            *half,
            locale,
            opts,
        ),
        TemporalValue::FractionalClock { clock, fraction } => {
            if !valid_fraction(fraction) {
                return None;
            }
            let TemporalValue::Clock {
                hour,
                minute: Some(minute),
                second: Some(second),
                period,
                meridiem,
                half: false,
            } = clock.as_ref()
            else {
                return None;
            };
            if *hour > 23 {
                return None;
            }
            render_clock(
                *hour,
                Some(*minute),
                Some(*second),
                Some(fraction),
                period.as_deref(),
                meridiem.as_deref(),
                false,
                locale,
                opts,
            )
        }
        TemporalValue::Timestamp {
            year,
            month,
            day,
            hour,
            minute,
            second,
            fraction,
            offset,
        } => render_timestamp(
            year,
            *month,
            *day,
            *hour,
            *minute,
            *second,
            fraction.as_deref(),
            offset,
            locale,
            opts,
        ),
        TemporalValue::EraYear { era, year } => {
            let n = positive_era_year(year)?;
            let year = ascii_digits(year)?;
            Some(match locale {
                SpeechLocale::Ja if opts.number_notation == NumberNotation::Phonetic => {
                    format!("{era}{}", verbalize::japanese_year(n)?)
                }
                SpeechLocale::Ja => format!("{era}{}年", orthographic_number(locale, n, opts)?),
                SpeechLocale::Ko => format!("{era} {}년", cardinal(locale, n)?),
                SpeechLocale::Zh => format!("{era}{}年", digits(locale, &year)?.replace(' ', "")),
                _ => return None,
            })
        }
        TemporalValue::Decade { year } => {
            if locale == SpeechLocale::Fr {
                if !year.is_multiple_of(10) || *year > 9990 {
                    return None;
                }
                let words = orthographic_number(locale, *year, opts)?;
                return Some(format!(
                    "années {}",
                    if year % 100 == 80 {
                        words.trim_end_matches('s')
                    } else {
                        &words
                    }
                ));
            }
            if locale != SpeechLocale::En {
                return None;
            }
            let century = year / 100;
            let decade = year % 100;
            let plural = match decade {
                0 => "hundreds",
                10 => "tens",
                20 => "twenties",
                30 => "thirties",
                40 => "forties",
                50 => "fifties",
                60 => "sixties",
                70 => "seventies",
                80 => "eighties",
                90 => "nineties",
                _ => return None,
            };
            Some(if *year < 100 {
                plural.into()
            } else {
                format!("{} {plural}", cardinal(locale, century)?)
            })
        }
        TemporalValue::CalendarSequence { items } => {
            let mut result = String::new();
            for item in items {
                match item {
                    CalendarItem::Date { value } => {
                        // Sequences contain calendar leaves, never recursively
                        // nested public documents with unbounded render depth.
                        if !matches!(value.as_ref(), TemporalValue::Date { .. }) {
                            return None;
                        }
                        result.push_str(&render(value, locale, opts)?);
                    }
                    CalendarItem::Range => result.push_str(match locale {
                        SpeechLocale::Ja => "から",
                        SpeechLocale::Zh => "至",
                        SpeechLocale::Ko => "부터",
                        _ => return None,
                    }),
                    CalendarItem::Weekday { index } => {
                        let name = match locale {
                            SpeechLocale::Ja => [
                                "月曜日",
                                "火曜日",
                                "水曜日",
                                "木曜日",
                                "金曜日",
                                "土曜日",
                                "日曜日",
                            ]
                            .get(*index as usize)?,
                            SpeechLocale::Zh => [
                                "星期一",
                                "星期二",
                                "星期三",
                                "星期四",
                                "星期五",
                                "星期六",
                                "星期日",
                            ]
                            .get(*index as usize)?,
                            SpeechLocale::Ko => [
                                "월요일",
                                "화요일",
                                "수요일",
                                "목요일",
                                "금요일",
                                "토요일",
                                "일요일",
                            ]
                            .get(*index as usize)?,
                            _ => return None,
                        };
                        result.push_str(name);
                    }
                }
            }
            Some(result)
        }
        TemporalValue::DateList {
            days,
            month: m,
            year,
        } => {
            if days.len() < 2 || days.len() > 31 {
                return None;
            }
            let mut words = vec![];
            for d in days {
                if matches!(
                    date_value(
                        year.clone(),
                        Some(*m),
                        Some(*d),
                        DatePresentation::DayFirst,
                        None,
                        false
                    ),
                    TemporalValue::Invalid { .. }
                ) {
                    return None;
                }
                words.push(match locale {
                    SpeechLocale::En => english_ordinal(*d)?.replace('-', " "),
                    SpeechLocale::De => verbalize::german_day(*d)?.into(),
                    SpeechLocale::Ru => verbalize::russian_day(*d)?.into(),
                    _ => orthographic_number(locale, *d, opts)?,
                });
            }
            let month_name = month(locale, *m)?;
            let connector = if matches!(locale, SpeechLocale::Es | SpeechLocale::Pt) {
                " de "
            } else {
                " "
            };
            let mut result = format!("{}{connector}{month_name}", words.join(" "));
            if let Some(y) = year {
                result.push_str(connector);
                result.push_str(&calendar_year(locale, y.parse().ok()?)?);
            }
            Some(result)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_timestamp(
    year: &str,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    fraction: Option<&str>,
    offset: &TimestampOffset,
    locale: SpeechLocale,
    opts: &SpeechOptions,
) -> Option<String> {
    let year_number = year.parse::<u32>().ok()?;
    if year.len() != 4
        || !year.bytes().all(|byte| byte.is_ascii_digit())
        || !valid_timestamp_date(year_number, month, day)
        || hour > 23
        || minute > 59
        || second > 60
        || fraction.is_some_and(|digits| !valid_fraction(digits))
        || second == 60 && !valid_leap_second(year_number, month, day, hour, minute, offset)
    {
        return None;
    }
    let date = format_calendar_components(
        Some(year),
        Some(month),
        Some(day),
        &DatePresentation::Numeric,
        None,
        false,
        locale,
        opts,
    )?;
    let clock = if second == 60 {
        let time = render_clock(
            hour,
            Some(minute),
            None,
            None,
            None,
            None,
            false,
            locale,
            opts,
        )?;
        let seconds = verbalize::quantity(
            locale,
            "duration-second",
            &NumberValue {
                negative: false,
                integer: "60".into(),
                fraction: fraction.map(str::to_owned),
            },
        )?;
        verbalize::join_parts(locale, &[time, seconds])
    } else {
        render_clock(
            hour,
            Some(minute),
            Some(second),
            fraction,
            None,
            None,
            false,
            locale,
            opts,
        )?
    };
    let zone = render_timestamp_offset(offset, locale)?;
    Some(format!("{date}, {clock} {zone}"))
}

fn render_timestamp_offset(offset: &TimestampOffset, locale: SpeechLocale) -> Option<String> {
    use SpeechLocale::*;
    match offset {
        TimestampOffset::Utc => Some("UTC".into()),
        TimestampOffset::UnknownLocal => Some(
            match locale {
                En => "UTC, local offset unknown",
                Es => "UTC, desfase local desconocido",
                Zh => "UTC，本地时差未知",
                Hi => "UTC, स्थानीय समयांतर अज्ञात",
                Ar => "UTC، فرق التوقيت المحلي غير معروف",
                Fr => "UTC, décalage local inconnu",
                Pt => "UTC, deslocamento local desconhecido",
                Ru => "UTC, местное смещение неизвестно",
                Bn => "UTC, স্থানীয় সময়ের ব্যবধান অজানা",
                Ur => "UTC، مقامی آفسیٹ نامعلوم",
                Id => "UTC, offset lokal tidak diketahui",
                De => "UTC, lokaler Offset unbekannt",
                Ja => "UTC、現地オフセット不明",
                Pa => "UTC, ਸਥਾਨਕ ਆਫਸੈੱਟ ਅਣਜਾਣ",
                Mr => "UTC, स्थानिक ऑफसेट अज्ञात",
                Te => "UTC, స్థానిక ఆఫ్‌సెట్ తెలియదు",
                Tr => "UTC, yerel saat farkı bilinmiyor",
                Ta => "UTC, உள்ளூர் நேர விலகல் தெரியவில்லை",
                Vi => "UTC, độ lệch giờ địa phương không xác định",
                Ko => "UTC, 현지 오프셋 알 수 없음",
            }
            .into(),
        ),
        TimestampOffset::Known {
            negative,
            hours,
            minutes,
        } => {
            if *hours > 23 || *minutes > 59 || *negative && *hours == 0 && *minutes == 0 {
                return None;
            }
            let sign = match (locale, negative) {
                (En, false) => "plus",
                (Es, false) => "más",
                (Zh, false) => "加",
                (Hi, false) => "प्लस",
                (Ar, false) => "زائد",
                (Fr, false) => "plus",
                (Pt, false) => "mais",
                (Ru, false) => "плюс",
                (Bn, false) => "যোগ",
                (Ur, false) => "جمع",
                (Id, false) => "plus",
                (De, false) => "plus",
                (Ja, false) => "プラス",
                (Pa, false) => "ਜੋੜ",
                (Mr, false) => "अधिक",
                (Te, false) => "ప్లస్",
                (Tr, false) => "artı",
                (Ta, false) => "கூட்டல்",
                (Vi, false) => "cộng",
                (Ko, false) => "플러스",
                (En, true) => "minus",
                (Es, true) => "menos",
                (Zh, true) => "减",
                (Hi, true) => "माइनस",
                (Ar, true) => "ناقص",
                (Fr, true) => "moins",
                (Pt, true) => "menos",
                (Ru, true) => "минус",
                (Bn, true) => "বিয়োগ",
                (Ur, true) => "منفی",
                (Id, true) => "minus",
                (De, true) => "minus",
                (Ja, true) => "マイナス",
                (Pa, true) => "ਘਟਾਓ",
                (Mr, true) => "वजा",
                (Te, true) => "మైనస్",
                (Tr, true) => "eksi",
                (Ta, true) => "கழித்தல்",
                (Vi, true) => "trừ",
                (Ko, true) => "마이너스",
            };
            let hour = verbalize::quantity(locale, "duration-hour", &number_value(*hours))?;
            let minute = verbalize::quantity(locale, "duration-minute", &number_value(*minutes))?;
            let amount = verbalize::join_parts(locale, &[hour, minute]);
            Some(format!("UTC {sign} {amount}"))
        }
    }
}

fn number_value(value: u32) -> NumberValue {
    NumberValue {
        negative: false,
        integer: value.to_string(),
        fraction: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn render_calendar(
    year: Option<&str>,
    m: Option<u32>,
    d: Option<u32>,
    presentation: &DatePresentation,
    connector: Option<&str>,
    year_label: bool,
    locale: SpeechLocale,
    opts: &SpeechOptions,
) -> Option<String> {
    let validated = date_value(
        year.map(str::to_owned),
        m,
        d,
        presentation.clone(),
        connector.map(str::to_owned),
        year_label,
    );
    if matches!(validated, TemporalValue::Invalid { .. }) {
        return None;
    }
    format_calendar_components(
        year,
        m,
        d,
        presentation,
        connector,
        year_label,
        locale,
        opts,
    )
}

fn russian_calendar_year(raw: u32) -> Option<String> {
    let year = if raw == 0 {
        "ноль".to_owned()
    } else if raw < 10 {
        format!("ноль {}", verbalize::russian_year(raw)?)
    } else {
        verbalize::russian_year(raw)?
    };
    if let Some(tail) = year.strip_prefix("одна тысяча") {
        Some(format!("тысяча{tail}"))
    } else {
        Some(year)
    }
}

#[allow(clippy::too_many_arguments)]
fn format_calendar_components(
    year: Option<&str>,
    m: Option<u32>,
    d: Option<u32>,
    presentation: &DatePresentation,
    connector: Option<&str>,
    year_label: bool,
    locale: SpeechLocale,
    opts: &SpeechOptions,
) -> Option<String> {
    use SpeechLocale::*;
    let year_n = year.and_then(|y| y.parse::<u32>().ok());
    if matches!(locale, Zh | Ja | Ko) {
        let mut parts = vec![];
        if let Some(year) = year {
            if locale == Ja && opts.number_notation == NumberNotation::Phonetic {
                parts.push(if year_n? == 0 {
                    format!("{}ねん", cardinal(locale, 0)?)
                } else {
                    verbalize::japanese_year(year_n?)?
                });
            } else {
                let spoken = if locale == Zh {
                    digits(locale, year)?.replace(' ', "")
                } else {
                    orthographic_number(locale, year_n?, opts)?
                };
                parts.push(format!(
                    "{spoken}{}",
                    if locale == Ko { "년" } else { "年" }
                ));
            }
        }
        if let Some(m) = m {
            let month = match (locale, opts.number_notation) {
                (Ja, NumberNotation::Phonetic) => verbalize::japanese_month(m)?.into(),
                (Ko, _) => match m {
                    6 => "유월".into(),
                    10 => "시월".into(),
                    _ => format!("{}월", cardinal(locale, m)?),
                },
                _ => format!("{}月", orthographic_number(locale, m, opts)?),
            };
            parts.push(month);
        }
        if let Some(d) = d {
            if locale == Ja && opts.number_notation == NumberNotation::Phonetic {
                parts.push(verbalize::japanese_day(d)?);
                return Some(parts.join(""));
            }
            let suffix = if locale == Ko {
                "일"
            } else if locale == Ja && opts.number_notation == NumberNotation::Phonetic {
                "にち"
            } else {
                "日"
            };
            parts.push(format!("{}{suffix}", orthographic_number(locale, d, opts)?));
        }
        return Some(parts.join(if locale == Ko { " " } else { "" }));
    }
    let month_name = m.and_then(|m| month(locale, m));
    let day = d.map(|d| cardinal(locale, d)).transpose_option()?;
    let year = year_n
        .map(|y| calendar_year(locale, y))
        .transpose_option()?;
    if locale == Vi {
        let mut parts = vec![];
        if let Some(day) = day {
            parts.push(format!("ngày {day}"));
        }
        if let Some(m) = m {
            parts.push(format!("tháng {}", cardinal(locale, m)?));
        }
        if let Some(year) = year {
            parts.push(format!("năm {year}"));
        }
        return Some(parts.join(" "));
    }
    if locale == En {
        let month_name = month_name?.to_lowercase();
        if d.is_none() {
            let mut output = month_name;
            if let Some(year) = year {
                if connector == Some("of") {
                    output.push_str(" of ");
                } else {
                    output.push(' ');
                }
                output.push_str(&year);
            }
            return Some(output);
        }
        let day = english_ordinal(d?)?.replace('-', " ");
        let mut output = if *presentation == DatePresentation::MonthFirst {
            format!("{month_name} {day}")
        } else {
            format!("the {day} of {month_name}")
        };
        if let Some(year) = year {
            output.push(' ');
            output.push_str(&year);
        }
        return Some(output);
    }
    if locale == Ru {
        if d.is_none() {
            // Month-year without a day keeps the month nominative — genitive
            // (`мая`) is only grammatical when governed by a day number. The
            // year renders genitive with explicit `года`, matching dated
            // usage (`две тысячи двадцать четвёртого года`).
            let prepositional = connector == Some("в");
            let names = if prepositional {
                RUSSIAN_PREPOSITIONAL_MONTHS
            } else {
                RUSSIAN_NOMINATIVE_MONTHS
            };
            let mut result = names.get(m?.checked_sub(1)? as usize)?.to_string();
            if prepositional {
                result.insert(0, ' ');
                result.insert(0, 'в');
            }
            if let Some(raw) = year_n {
                result.push(' ');
                result.push_str(&russian_calendar_year(raw)?);
                result.push_str(" года");
            }
            return Some(result);
        }
        let d = d?;
        let day = if year_n.is_some_and(|y| y < 100) || *presentation == DatePresentation::DayFirst
        {
            verbalize::russian_year(d)?
        } else {
            verbalize::russian_day(d)?.into()
        };
        let mut result = format!("{day} {}", month_name?);
        if let Some(raw) = year_n {
            result.push(' ');
            result.push_str(&russian_calendar_year(raw)?);
        }
        if year_label {
            result.push_str(" года");
        }
        return Some(result);
    }
    let day = match locale {
        Es if d == Some(1) => Some("primero".to_owned()),
        Pt if d == Some(1) => Some("primeiro".to_owned()),
        Fr if d == Some(1) => Some("premier".to_owned()),
        De => d
            .map(|d| verbalize::german_day(d).map(str::to_owned))
            .transpose_option()?,
        _ => day,
    };
    let mut result = match (day, month_name) {
        (Some(day), Some(month)) if matches!(locale, Es | Pt) => format!("{day} de {month}"),
        (Some(day), Some(month)) => format!("{day} {month}"),
        (None, Some(month)) => month.to_owned(),
        (Some(day), None) => day,
        _ => return year,
    };
    if let Some(year) = year {
        result.push_str(match locale {
            Es | Pt => {
                if connector == Some("del") {
                    " del "
                } else {
                    " de "
                }
            }
            _ => " ",
        });
        result.push_str(&year);
    }
    Some(result)
}

fn calendar_year(locale: SpeechLocale, n: u32) -> Option<String> {
    if locale == SpeechLocale::Vi && (1000..=9999).contains(&n) && (1..100).contains(&(n % 1000)) {
        return Some(format!(
            "{}{}{}",
            cardinal(locale, n / 1000 * 1000)?,
            if n % 1000 < 10 { " lẻ " } else { " " },
            cardinal(locale, n % 1000)?
        ));
    }
    if locale == SpeechLocale::En && (1000..=1999).contains(&n) {
        return Some(if n.is_multiple_of(100) {
            format!("{} hundred", cardinal(locale, n / 100)?)
        } else {
            format!(
                "{} {}",
                cardinal(locale, n / 100)?,
                cardinal(locale, n % 100)?
            )
        });
    }
    if locale == SpeechLocale::En && (2010..=2099).contains(&n) {
        return Some(format!("twenty {}", cardinal(locale, n % 100)?).replace('-', " "));
    }
    if locale == SpeechLocale::Hi && (2100..=2999).contains(&n) {
        return Some(format!(
            "{} सौ{}",
            cardinal(locale, n / 100)?,
            if n.is_multiple_of(100) {
                String::new()
            } else {
                format!(" {}", cardinal(locale, n % 100)?)
            }
        ));
    }
    cardinal(locale, n)
}

fn english_ordinal(n: u32) -> Option<String> {
    verbalize::english_day(n).map(str::to_owned)
}
#[allow(clippy::too_many_arguments)]
fn render_clock(
    hour: u32,
    minute: Option<u32>,
    second: Option<u32>,
    fraction: Option<&str>,
    period: Option<&str>,
    meridiem: Option<&str>,
    half: bool,
    locale: SpeechLocale,
    opts: &SpeechOptions,
) -> Option<String> {
    use SpeechLocale::*;
    // Parsing may tolerate redundant source notation; public semantic values
    // must already be canonical and cannot opt out of validation.
    let effective_meridiem = effective_clock_meridiem(period, meridiem, locale)?;
    if decode_clock_hour(hour, effective_meridiem, ClockPolicy::Strict) != Some(hour) {
        return None;
    }
    if hour > 24
        || minute.is_some_and(|m| m > 59)
        || second.is_some_and(|s| s > 59)
        || hour == 24 && (minute.unwrap_or(0) > 0 || second.unwrap_or(0) > 0)
    {
        return None;
    }
    // These branches have established spoken AM/PM forms. The others must
    // retain the source's time value using their existing 24-hour rendering path.
    // CLDR 48 availableFormats.Hm exists for all 20 locales; no new period
    // vocabulary or implicit day rollover is introduced here.
    // CLDR 48 Korean hm is `a h:mm`: a formal written prefix already carries
    // AM/PM, so retain its canonical 12-hour value instead of adding 12 again.
    let prefix_marks_period = effective_clock_meridiem(period, None, locale)?.is_some();
    let hour = if matches!(locale, En | Es | Zh) || prefix_marks_period {
        hour
    } else {
        clock_hour_24(hour, effective_meridiem)?
    };
    let h = orthographic_number(locale, hour, opts)?;
    let m = minute
        .map(|m| orthographic_number(locale, m, opts))
        .transpose_option()?;
    let precise_seconds = fraction.map(|_| {
        let mut options = opts.clone();
        options.reading_style = ReadingStyle::Precise;
        options
    });
    let second_options = precise_seconds.as_ref().unwrap_or(opts);
    let s = second
        .map(|s| {
            if let Some(fraction) = fraction {
                super::numbers::verbalize_with_options(
                    &NumberValue {
                        negative: false,
                        integer: s.to_string(),
                        fraction: Some(fraction.to_owned()),
                    },
                    second_options,
                    NumberReading::Cardinal,
                )
            } else {
                orthographic_number(locale, s, opts)
            }
        })
        .transpose_option()?;
    let quantity = |key: &str, n: u32| {
        super::quantities::render_measure(
            key,
            &NumberValue {
                negative: false,
                integer: n.to_string(),
                fraction: fraction
                    .filter(|_| key == "duration-second")
                    .map(str::to_owned),
            },
            if fraction.is_some() && key == "duration-second" {
                second_options
            } else {
                opts
            },
        )
    };
    let mut result = match locale {
        En => {
            let mut x = h.replace('-', " ");
            if let Some(minute) = minute.filter(|m| *m > 0) {
                x.push_str(if minute < 10 { " oh " } else { " " });
                x.push_str(&m?.replace('-', " "));
            }
            if minute == Some(0) && meridiem.is_none() {
                // Cambridge excludes o'clock from the 24-hour clock; a complete
                // zero-minute military hour uses hundred, without AM/PM guessing.
                // https://assets.cambridge.org/97813166/36176/excerpt/9781316636176_excerpt.pdf
                // https://media.defense.gov/2014/Jun/20/2002655870/-1/-1/1/140620-N-ZZ182-6514.pdf
                x.push_str(if (1..=12).contains(&hour) {
                    " o'clock"
                } else {
                    " hundred"
                });
            }
            if let Some(second) = second {
                x.push(' ');
                x.push_str(&quantity("duration-second", second)?);
            }
            if let Some(marker) = meridiem {
                x.push(' ');
                x.push_str(&marker.to_ascii_uppercase());
            }
            x
        }
        Es => {
            // Spanish clock hours imply feminine `hora(s)`, including
            // compound forms such as veintiuna, not only una.
            let h = super::numbers::verbalize_number_feminine(
                &NumberValue {
                    negative: false,
                    integer: hour.to_string(),
                    fraction: None,
                },
                Es,
                NumberReading::Cardinal,
            )?;
            let mut x = h;
            if let Some(m) = m.filter(|_| minute != Some(0)) {
                x.push_str(&format!(" y {m}"));
            }
            if let Some(second) = second {
                x.push_str(" y ");
                x.push_str(&quantity("duration-second", second)?);
            }
            if let Some(marker) = meridiem {
                x.push_str(if marker == "am" {
                    " de la mañana"
                } else {
                    " de la tarde"
                });
            }
            x
        }
        Pt => {
            // Keep the established clock spelling for midnight. General
            // Portuguese quantity plural rules classify zero as one, but the
            // existing clock register says zero horas. Other values use the
            // shared feminine agreement, including uma and vinte e duas.
            let mut x = if hour == 0 {
                format!("{h} horas")
            } else {
                quantity("duration-hour", hour)?
            };
            if let Some(m) = m.filter(|_| minute != Some(0)) {
                x.push_str(&format!(" e {m}"));
            }
            if let Some(second) = second {
                x.push_str(" e ");
                x.push_str(&quantity("duration-second", second)?);
            }
            x
        }
        De => {
            // Duden: the numeral before the noun Uhr is ein, not eins.
            // https://www.duden.de/sprachwissen/sprachratgeber/Ein-eins-unbestimmter-Artikel-oder-Zahlwort
            let h = if hour == 1 { "ein".to_owned() } else { h };
            let mut x = format!("{h} uhr");
            if minute == Some(0) && opts.reading_style == ReadingStyle::Precise {
                // A clock uses Uhr; Stunden/Minuten alone describe duration.
                // https://www.duden.de/sprachwissen/sprachratgeber/Uhrzeitangaben
                let zero_minutes = quantity("duration-minute", 0)?;
                x.push(' ');
                x.push_str(&zero_minutes.to_lowercase());
            }
            if let Some(m) = m.filter(|_| minute != Some(0)) {
                x.push(' ');
                x.push_str(&m);
            }
            if let Some(second) = second {
                x.push(' ');
                x.push_str(&quantity("duration-second", second)?.to_lowercase());
            }
            x
        }
        Hi => {
            let mut x = if minute.is_none() || minute == Some(0) && second.is_none() {
                format!("{h} बजे")
            } else {
                format!("{h} बजकर {} मिनट", m?)
            };
            if let Some(s) = s {
                x.push_str(&format!(" {s} सेकंड"));
            }
            x
        }
        Ja => {
            if opts.number_notation == NumberNotation::Phonetic {
                let mut x = verbalize::japanese_hour(hour)?;
                if let Some(minute) = minute {
                    x.push_str(&verbalize::japanese_minutes(minute)?);
                }
                if let Some(second) = second {
                    let words = if fraction.is_some() {
                        s?
                    } else {
                        cardinal(locale, second)?
                    };
                    x.push_str(&format!("{words}びょう"));
                }
                x
            } else {
                let mut x = format!("{h}時");
                if let Some(m) = m {
                    x.push_str(&format!("{m}分"));
                }
                if let Some(s) = s {
                    x.push_str(&format!("{s}秒"));
                }
                x
            }
        }
        Ko => {
            let mut x = format!("{}시", verbalize::korean_native(hour)?);
            if let Some(m) = m {
                x.push_str(&format!(" {m}분"));
            }
            if let Some(s) = s {
                x.push_str(&format!(" {s}초"));
            }
            if half {
                x.push_str(" 반");
            }
            x
        }
        Zh => {
            let mut x = format!("{h}点");
            if let Some(m) = m {
                x.push_str(&format!("{m}分"));
            }
            if let Some(s) = s {
                x.push_str(&format!("{s}秒"));
            }
            if let Some(marker) = meridiem {
                x.push_str(marker);
            }
            x
        }
        Vi => {
            let mut x = format!("{h} giờ");
            if let Some(m) = m {
                x.push_str(&format!(" {m} phút"));
            }
            if let Some(s) = s {
                x.push_str(&format!(" {s} giây"));
            }
            x
        }
        Ru => {
            let mut x = quantity("duration-hour", hour)?;
            if let Some(minute) = minute.filter(|m| *m != 0) {
                x.push_str(&format!(" и {}", quantity("duration-minute", minute)?));
            }
            if let Some(second) = second {
                x.push_str(&format!(" и {}", quantity("duration-second", second)?));
            }
            x
        }
        _ if fraction.is_some() => verbalize::join_parts(
            locale,
            &[
                verbalize::time(locale, hour, minute, None)?,
                quantity("duration-second", second?)?,
            ],
        ),
        _ => verbalize::time(locale, hour, minute, second)?,
    };
    if let Some(period) = period {
        result = format!("{period} {result}");
    }
    Some(result)
}

#[cfg(test)]
mod compact_projection_tests {
    use super::*;
    fn date(year: &str, month: u32, day: u32) -> SpeechValue {
        SpeechValue::Temporal {
            value: TemporalValue::Date {
                year: Some(year.into()),
                month: Some(month),
                day: Some(day),
                presentation: DatePresentation::Numeric,
                connector: None,
                year_label: false,
            },
        }
    }
    fn clock(hour: u32, second: Option<u32>, marker: Option<&str>) -> SpeechValue {
        SpeechValue::Temporal {
            value: TemporalValue::Clock {
                hour,
                minute: Some(15),
                second,
                meridiem: marker.map(str::to_owned),
                period: None,
                half: false,
            },
        }
    }
    #[test]
    fn compact_projection_checks_fields_in_both_representations() {
        for (year, month, day) in [(1, 1, 1), (2024, 2, 29), (9999, 12, 31)] {
            let compact = SpeechValue::Date { year, month, day };
            assert_eq!(compact_value(&compact), Some(compact.clone()));
            assert_eq!(
                compact_value(&date(&format!("{year:04}"), month, day)),
                Some(compact)
            );
        }
        for (year, month, day) in [
            (0, 1, 1),
            (10000, 1, 1),
            (2023, 2, 29),
            (2024, 6, 31),
            (2024, 13, 1),
            (2024, 1, 0),
        ] {
            assert!(compact_value(&SpeechValue::Date { year, month, day }).is_none());
            assert!(compact_value(&date(&format!("{year:04}"), month, day)).is_none());
        }
        for year in ["24", "２０２４", "+024", "2024 "] {
            assert!(compact_value(&date(year, 1, 1)).is_none());
        }
        for second in [None, Some(0), Some(59)] {
            assert_eq!(
                compact_value(&clock(4, second, Some("pm"))),
                Some(SpeechValue::Time {
                    hour: 16,
                    minute: 15,
                    second
                })
            );
        }
        assert_ne!(
            compact_value(&clock(4, None, None)),
            compact_value(&clock(4, Some(0), None))
        );
        assert_ne!(
            compact_value(&clock(4, None, None)),
            compact_value(&clock(4, None, Some("pm")))
        );
        assert_eq!(
            compact_value(&clock(12, None, Some("am"))),
            Some(SpeechValue::Time {
                hour: 0,
                minute: 15,
                second: None
            })
        );
        assert_eq!(
            compact_value(&clock(12, None, Some("pm"))),
            Some(SpeechValue::Time {
                hour: 12,
                minute: 15,
                second: None
            })
        );
        for (hour, minute, second) in [(24, 0, None), (9, 60, None), (9, 15, Some(60))] {
            assert!(
                compact_value(&SpeechValue::Time {
                    hour,
                    minute,
                    second
                })
                .is_none()
            );
        }
        for (hour, marker) in [
            (24, None),
            (0, Some("am")),
            (13, Some("pm")),
            (1, Some("PM")),
            (1, Some("unknown")),
        ] {
            assert!(compact_value(&clock(hour, None, marker)).is_none());
        }
    }
    #[test]
    fn compact_projection_excludes_partial_or_labelled_calendar_metadata() {
        let mut candidates = Vec::new();
        for presentation in [
            DatePresentation::DayFirst,
            DatePresentation::MonthFirst,
            DatePresentation::Component,
        ] {
            let mut value = date("2024", 1, 1);
            if let SpeechValue::Temporal {
                value:
                    TemporalValue::Date {
                        presentation: p, ..
                    },
            } = &mut value
            {
                *p = presentation;
            }
            candidates.push(value);
        }
        for field in ["connector", "label", "partial"] {
            let mut value = date("2024", 1, 1);
            if let SpeechValue::Temporal {
                value:
                    TemporalValue::Date {
                        connector,
                        year_label,
                        day,
                        ..
                    },
            } = &mut value
            {
                match field {
                    "connector" => *connector = Some("del".into()),
                    "label" => *year_label = true,
                    _ => *day = None,
                }
            }
            candidates.push(value);
        }
        candidates.push(SpeechValue::Temporal {
            value: TemporalValue::InvalidCalendarDate {
                year: Some("2024".into()),
                month: 2,
                day: 29,
                presentation: DatePresentation::Numeric,
                connector: None,
                year_label: false,
            },
        });
        candidates.push(SpeechValue::Temporal {
            value: TemporalValue::EraYear {
                era: "令和".into(),
                year: "4".into(),
            },
        });
        for field in ["period", "half", "partial", "minute", "second"] {
            let mut value = clock(4, None, None);
            if let SpeechValue::Temporal {
                value:
                    TemporalValue::Clock {
                        period,
                        half,
                        minute,
                        second,
                        ..
                    },
            } = &mut value
            {
                match field {
                    "period" => *period = Some("오후".into()),
                    "half" => *half = true,
                    "partial" => *minute = None,
                    "minute" => *minute = Some(60),
                    _ => *second = Some(60),
                }
            }
            candidates.push(value);
        }
        for value in candidates {
            assert!(compact_value(&value).is_none(), "{value:?}");
        }
    }
}
