//! Deterministic spoken readings over lossless typed spans.
//!
//! This path does not apply similarity normalization, case folding, rebus
//! decoding, or leetspeak rewrites. Unsupported structured values retain their
//! source text and report a reason instead of inventing a pronunciation.

use super::core::valid_date;
use super::lexicons::{currency_name, month, unit_pattern};
use super::numbers::{verbalize_number, verbalize_number_feminine, verbalize_with_options};
use super::parser::parse_for_speech;
use super::types::{
    NumberReading, NumberValue, SpeechDocument, SpeechLocale, SpeechNormalization, SpeechOptions,
    SpeechReading, SpeechValue,
};
use crate::TextIntelError;

pub fn normalize_for_speech(
    text: &str,
    options: &SpeechOptions,
) -> Result<SpeechNormalization, TextIntelError> {
    verbalize_document_with_options(&parse_for_speech(text, options)?, options)
}

/// Verbalize a document, rejecting externally constructed invalid span maps.
/// The complete original text must be partitioned once, in UTF-8 byte order.
pub fn verbalize_document(
    document: &SpeechDocument,
) -> Result<SpeechNormalization, TextIntelError> {
    verbalize_document_with_options(document, &SpeechOptions::new(document.locale))
}

/// Render an already parsed document using explicit shared reading conventions.
pub fn verbalize_document_with_options(
    document: &SpeechDocument,
    options: &SpeechOptions,
) -> Result<SpeechNormalization, TextIntelError> {
    verbalize_document_with_mode(document, options, super::abbreviations::MatcherMode::Typed)
}

pub(crate) fn verbalize_document_with_mode(
    document: &SpeechDocument,
    options: &SpeechOptions,
    abbreviation_mode: super::abbreviations::MatcherMode,
) -> Result<SpeechNormalization, TextIntelError> {
    options.validate()?;
    let length = document.text.chars().count();
    if length > options.max_input_chars {
        return Err(TextIntelError::InputTooLong {
            length,
            maximum: options.max_input_chars,
        });
    }
    if document.spans.len() > options.max_spans {
        return Err(TextIntelError::TooManySegments {
            maximum: options.max_spans,
        });
    }
    if document.locale != options.locale {
        return Err(TextIntelError::InvalidConfiguration(
            "document and rendering locales must agree".into(),
        ));
    }
    let mut cursor = 0;
    let mut text = String::new();
    let mut readings = Vec::with_capacity(document.spans.len());
    let mut fully_supported = true;
    let mut abbreviation_matcher = None;
    for span in &document.spans {
        if span.start != cursor
            || span.end <= span.start
            || document.text.get(span.start..span.end) != Some(span.source.as_str())
        {
            return Err(TextIntelError::InvalidConfiguration("speech spans must partition the original text with valid UTF-8 boundaries and matching sources".into()));
        }
        cursor = span.end;
        let (spoken, reason) = match &span.value {
            SpeechValue::Text => (Some(span.source.clone()), None),
            SpeechValue::Abbreviation { id } => {
                let matcher = abbreviation_matcher.get_or_insert_with(|| {
                    super::abbreviations::Matcher::with_mode(
                        &document.text,
                        &super::lexer::protected_regions(&document.text),
                        document.locale,
                        options.abbreviation_policy,
                        abbreviation_mode,
                    )
                });
                (
                    abbreviation(id, span.start, span.end, &span.source, matcher),
                    Some(
                        "abbreviation entry, locale, source, context or reading policy does not agree"
                            .into(),
                    ),
                )
            }
            SpeechValue::Number { value, reading } => (verbalize_with_options(value, options, *reading), Some("number form or range is not supported for this locale".to_string())),
            SpeechValue::Date { year, month, day } => (super::temporal::render_date(*year,*month,*day,document.locale,options), Some("date value or date grammar is not supported for this locale".to_string())),
            SpeechValue::Time { hour, minute, second } => (super::temporal::render_time(*hour,*minute,*second,document.locale,options), Some("time value or time grammar is not supported for this locale".to_string())),
            SpeechValue::InflectedNumber {..} => (super::grammar::render(&span.value,options),Some("numeric inflection is unsupported for this language or value".into())),
            SpeechValue::MeasuredExpression {..} => (super::quantities::render(&span.value,&span.source,options),Some("measured expression grammar is unsupported".into())),
            SpeechValue::LabelledNumber {..} => (super::quantities::render(&span.value,&span.source,options),Some("labelled quantity reading is unsupported".into())),
            SpeechValue::Expression {value} => (super::expressions::render(value,options),Some("expression grammar is unsupported for this locale".into())),
            SpeechValue::Temporal { value } => (super::temporal::render(value,document.locale,options), Some("temporal value or reading grammar is unsupported".into())),
            SpeechValue::Currency { code, value } => (super::quantities::render(&span.value,&span.source,options).or_else(||money(document.locale, code, value)), Some("currency, fractional precision, or currency grammar is not supported for this locale".to_string())),
            SpeechValue::Percent { value } => (super::quantities::render(&span.value,&span.source,options).or_else(||quantity(document.locale, "concentr-percent", value)), Some("percent grammar or number range is not supported for this locale".to_string())),
            SpeechValue::Measure { unit, value } => (super::quantities::render(&span.value,&span.source,options).or_else(||unit_key(unit).and_then(|key| quantity(document.locale, key, value))), Some("unit, agreement, or number range is not supported for this locale".to_string())),
            SpeechValue::Identifier { kind } => (None, Some(format!("identifier {kind:?} requires an explicit application reading policy"))),
            SpeechValue::Ambiguous { reason } => (None, Some(reason.clone())),
        };
        let supported = spoken.is_some();
        fully_supported &= supported;
        let spoken = spoken.unwrap_or_else(|| span.source.clone());
        text.push_str(&spoken);
        readings.push(SpeechReading {
            start: span.start,
            end: span.end,
            source: span.source.clone(),
            spoken,
            supported,
            reason: if supported {
                match &span.value {
                    SpeechValue::Temporal { value } => {
                        super::temporal::diagnostic(value).map(str::to_owned)
                    }
                    _ => None,
                }
            } else {
                reason
            },
        });
    }
    if cursor != document.text.len() {
        return Err(TextIntelError::InvalidConfiguration(
            "speech spans must cover the complete original text".into(),
        ));
    }
    Ok(SpeechNormalization {
        text,
        locale: document.locale,
        readings,
        fully_supported,
    })
}

fn abbreviation(
    id: &str,
    start: usize,
    end: usize,
    source: &str,
    matcher: &super::abbreviations::Matcher<'_>,
) -> Option<String> {
    let matched = matcher.match_at(start)?;
    (matched.end == end && matched.entry.id == id && matched.entry.written == source)
        .then(|| matched.entry.spoken.clone())
}

pub(super) fn value(integer: u32) -> NumberValue {
    NumberValue {
        negative: false,
        integer: integer.to_string(),
        fraction: None,
    }
}

pub(super) fn cardinal(locale: SpeechLocale, n: u32) -> Option<String> {
    verbalize_number(&value(n), locale, NumberReading::Cardinal)
}

pub(super) fn date(locale: SpeechLocale, year: u32, m: u32, day: u32) -> Option<String> {
    use SpeechLocale::*;
    if !valid_date(year, m, day) {
        return None;
    }
    let month_name = month(locale, m)?;
    let year_words = cardinal(locale, year)?;
    let day_words = cardinal(locale, day)?;
    Some(match locale {
        En => format!("{month_name} {}, {year_words}", english_day(day)?),
        Es | Pt => format!("{day_words} de {month_name} de {year_words}"),
        Fr => format!(
            "{} {month_name} {year_words}",
            if day == 1 { "premier" } else { &day_words }
        ),
        De => format!("{} {month_name} {year_words}", german_day(day)?),
        Zh => {
            let year = verbalize_number(&value(year), locale, NumberReading::Digits)?
                .split_whitespace()
                .collect::<String>();
            format!("{year}年{month_name}{day_words}日")
        }
        Ja => format!(
            "{}{}{}",
            japanese_year(year)?,
            japanese_month(m)?,
            japanese_day(day)?
        ),
        Ko => format!("{year_words}년 {} {day_words}일", korean_month(m)?),
        Vi => format!(
            "ngày {day_words} tháng {} năm {year_words}",
            cardinal(locale, m)?
        ),
        Ru => format!(
            "{} {month_name} {} года",
            russian_day(day)?,
            russian_year(year)?
        ),
        // Arabic dates need case-aware year/ordinal handling beyond the
        // independent cardinal-number rules. Preserve the date until that
        // grammar has an independently reviewed implementation.
        Ar => return None,
        _ => format!("{day_words} {month_name} {year_words}"),
    })
}

pub(super) fn english_day(day: u32) -> Option<&'static str> {
    const DAYS: [&str; 31] = [
        "first",
        "second",
        "third",
        "fourth",
        "fifth",
        "sixth",
        "seventh",
        "eighth",
        "ninth",
        "tenth",
        "eleventh",
        "twelfth",
        "thirteenth",
        "fourteenth",
        "fifteenth",
        "sixteenth",
        "seventeenth",
        "eighteenth",
        "nineteenth",
        "twentieth",
        "twenty-first",
        "twenty-second",
        "twenty-third",
        "twenty-fourth",
        "twenty-fifth",
        "twenty-sixth",
        "twenty-seventh",
        "twenty-eighth",
        "twenty-ninth",
        "thirtieth",
        "thirty-first",
    ];
    DAYS.get(day.checked_sub(1)? as usize).copied()
}
pub(super) fn german_day(day: u32) -> Option<&'static str> {
    const DAYS: [&str; 31] = [
        "erster",
        "zweiter",
        "dritter",
        "vierter",
        "fünfter",
        "sechster",
        "siebter",
        "achter",
        "neunter",
        "zehnter",
        "elfter",
        "zwölfter",
        "dreizehnter",
        "vierzehnter",
        "fünfzehnter",
        "sechzehnter",
        "siebzehnter",
        "achtzehnter",
        "neunzehnter",
        "zwanzigster",
        "einundzwanzigster",
        "zweiundzwanzigster",
        "dreiundzwanzigster",
        "vierundzwanzigster",
        "fünfundzwanzigster",
        "sechsundzwanzigster",
        "siebenundzwanzigster",
        "achtundzwanzigster",
        "neunundzwanzigster",
        "dreißigster",
        "einunddreißigster",
    ];
    DAYS.get(day.checked_sub(1)? as usize).copied()
}
/// Calendar/era year counter; four before 年 retains the established よねん.
pub(super) fn japanese_year(year: u32) -> Option<String> {
    if year == 0 {
        return None;
    }
    Some(format!(
        "{}ねん",
        replace_suffix(cardinal(SpeechLocale::Ja, year)?, "よん", "よ")
    ))
}

pub(super) fn japanese_month(month: u32) -> Option<&'static str> {
    [
        "いちがつ",
        "にがつ",
        "さんがつ",
        "しがつ",
        "ごがつ",
        "ろくがつ",
        "しちがつ",
        "はちがつ",
        "くがつ",
        "じゅうがつ",
        "じゅういちがつ",
        "じゅうにがつ",
    ]
    .get(month.checked_sub(1)? as usize)
    .copied()
}
pub(super) fn japanese_day(day: u32) -> Option<String> {
    let irregular = match day {
        1 => Some("ついたち"),
        2 => Some("ふつか"),
        3 => Some("みっか"),
        4 => Some("よっか"),
        5 => Some("いつか"),
        6 => Some("むいか"),
        7 => Some("なのか"),
        8 => Some("ようか"),
        9 => Some("ここのか"),
        10 => Some("とおか"),
        14 => Some("じゅうよっか"),
        // Japan Foundation Marugoto Starter wordbook, calendar/date table:
        // https://www.marugoto-online.jp/a1/support/pdf/MarugotoStarterWordbook_jpn.pdf
        17 => Some("じゅうしちにち"),
        19 => Some("じゅうくにち"),
        20 => Some("はつか"),
        24 => Some("にじゅうよっか"),
        27 => Some("にじゅうしちにち"),
        29 => Some("にじゅうくにち"),
        _ => None,
    };
    irregular
        .map(str::to_string)
        .or_else(|| cardinal(SpeechLocale::Ja, day).map(|v| format!("{v}にち")))
}
pub(super) fn korean_month(month: u32) -> Option<String> {
    match month {
        6 => Some("유월".into()),
        10 => Some("시월".into()),
        1..=12 => cardinal(SpeechLocale::Ko, month).map(|v| format!("{v}월")),
        _ => None,
    }
}
pub(super) fn russian_day(day: u32) -> Option<&'static str> {
    [
        "первое",
        "второе",
        "третье",
        "четвёртое",
        "пятое",
        "шестое",
        "седьмое",
        "восьмое",
        "девятое",
        "десятое",
        "одиннадцатое",
        "двенадцатое",
        "тринадцатое",
        "четырнадцатое",
        "пятнадцатое",
        "шестнадцатое",
        "семнадцатое",
        "восемнадцатое",
        "девятнадцатое",
        "двадцатое",
        "двадцать первое",
        "двадцать второе",
        "двадцать третье",
        "двадцать четвёртое",
        "двадцать пятое",
        "двадцать шестое",
        "двадцать седьмое",
        "двадцать восьмое",
        "двадцать девятое",
        "тридцатое",
        "тридцать первое",
    ]
    .get(day.checked_sub(1)? as usize)
    .copied()
}
pub(super) fn russian_year(year: u32) -> Option<String> {
    const SMALL: [&str; 20] = [
        "",
        "первого",
        "второго",
        "третьего",
        "четвёртого",
        "пятого",
        "шестого",
        "седьмого",
        "восьмого",
        "девятого",
        "десятого",
        "одиннадцатого",
        "двенадцатого",
        "тринадцатого",
        "четырнадцатого",
        "пятнадцатого",
        "шестнадцатого",
        "семнадцатого",
        "восемнадцатого",
        "девятнадцатого",
    ];
    const TENS: [&str; 10] = [
        "",
        "",
        "двадцатого",
        "тридцатого",
        "сорокового",
        "пятидесятого",
        "шестидесятого",
        "семидесятого",
        "восьмидесятого",
        "девяностого",
    ];
    const HUNDREDS: [&str; 10] = [
        "",
        "сотого",
        "двухсотого",
        "трёхсотого",
        "четырёхсотого",
        "пятисотого",
        "шестисотого",
        "семисотого",
        "восьмисотого",
        "девятисотого",
    ];
    const THOUSANDS: [&str; 10] = [
        "",
        "тысячного",
        "двухтысячного",
        "трёхтысячного",
        "четырёхтысячного",
        "пятитысячного",
        "шеститысячного",
        "семитысячного",
        "восьмитысячного",
        "девятитысячного",
    ];
    if year == 0 || year > 9999 {
        return None;
    }
    let tail = year % 100;
    if tail == 0 {
        let h = year % 1000;
        if h == 0 {
            return Some(THOUSANDS[(year / 1000) as usize].into());
        }
        let head = year - h;
        return if head == 0 {
            Some(HUNDREDS[(h / 100) as usize].into())
        } else {
            Some(format!(
                "{} {}",
                cardinal(SpeechLocale::Ru, head)?,
                HUNDREDS[(h / 100) as usize]
            ))
        };
    }
    let ordinal = if tail < 20 {
        SMALL[tail as usize].into()
    } else if tail.is_multiple_of(10) {
        TENS[(tail / 10) as usize].into()
    } else {
        format!(
            "{} {}",
            cardinal(SpeechLocale::Ru, tail / 10 * 10)?,
            SMALL[(tail % 10) as usize]
        )
    };
    if year == tail {
        Some(ordinal)
    } else {
        Some(format!(
            "{} {ordinal}",
            cardinal(SpeechLocale::Ru, year - tail)?
        ))
    }
}

fn conjunction(locale: SpeechLocale) -> &'static str {
    use SpeechLocale::*;
    match locale {
        En => "and",
        Es => "y",
        Zh => "",
        Hi => "और",
        Ar => "و",
        Fr => "et",
        Pt => "e",
        Ru => "и",
        Bn => "এবং",
        Ur => "اور",
        Id => "dan",
        De => "und",
        Ja => "",
        Pa => "ਅਤੇ",
        Mr => "आणि",
        Te => "మరియు",
        Tr => "ve",
        Ta => "மற்றும்",
        Vi => "và",
        Ko => "",
    }
}
pub(super) fn join_parts(locale: SpeechLocale, parts: &[String]) -> String {
    if locale == SpeechLocale::Ar {
        return parts.join(" و");
    }
    let connector = conjunction(locale);
    if connector.is_empty() {
        parts.join(if matches!(locale, SpeechLocale::Zh | SpeechLocale::Ja) {
            ""
        } else {
            " "
        })
    } else {
        parts.join(&format!(" {connector} "))
    }
}
pub(super) fn time(
    locale: SpeechLocale,
    hour: u32,
    minute: Option<u32>,
    second: Option<u32>,
) -> Option<String> {
    use SpeechLocale::*;
    if hour > 23 || minute.is_some_and(|m| m > 59) || second.is_some_and(|s| s > 59) {
        return None;
    }
    if locale == Ja {
        let mut s = japanese_hour(hour)?;
        if let Some(minute) = minute {
            s.push_str(&japanese_minutes(minute)?);
        }
        if let Some(sec) = second {
            s.push_str(&format!("{}びょう", cardinal(locale, sec)?));
        }
        return Some(s);
    }
    if locale == Zh {
        let h = if hour == 2 {
            "两".into()
        } else {
            cardinal(locale, hour)?
        };
        let mut s = format!("{h}点");
        if let Some(minute) = minute {
            s.push_str(&format!("{}分", cardinal(locale, minute)?));
        }
        if let Some(sec) = second {
            s.push_str(&format!("{}秒", cardinal(locale, sec)?));
        }
        return Some(s);
    }
    if locale == Ko {
        let mut s = format!("{} 시", korean_native(hour)?);
        if let Some(minute) = minute {
            s.push_str(&format!(" {} 분", cardinal(locale, minute)?));
        }
        if let Some(sec) = second {
            s.push_str(&format!(" {} 초", cardinal(locale, sec)?));
        }
        return Some(s);
    }
    if locale == Ar {
        const HOURS: [&str; 24] = [
            "صفر",
            "الواحدة",
            "الثانية",
            "الثالثة",
            "الرابعة",
            "الخامسة",
            "السادسة",
            "السابعة",
            "الثامنة",
            "التاسعة",
            "العاشرة",
            "الحادية عشرة",
            "الثانية عشرة",
            "الثالثة عشرة",
            "الرابعة عشرة",
            "الخامسة عشرة",
            "السادسة عشرة",
            "السابعة عشرة",
            "الثامنة عشرة",
            "التاسعة عشرة",
            "العشرون",
            "الحادية والعشرون",
            "الثانية والعشرون",
            "الثالثة والعشرون",
        ];
        let mut parts = vec![format!("الساعة {}", HOURS[hour as usize])];
        if let Some(minute) = minute {
            parts.push(quantity(locale, "duration-minute", &value(minute))?);
        }
        if let Some(sec) = second {
            parts.push(quantity(locale, "duration-second", &value(sec))?);
        }
        return Some(join_parts(locale, &parts));
    }
    let mut parts = vec![quantity(locale, "duration-hour", &value(hour))?];
    if let Some(minute) = minute {
        parts.push(quantity(locale, "duration-minute", &value(minute))?);
    }
    if let Some(sec) = second {
        parts.push(quantity(locale, "duration-second", &value(sec))?);
    }
    Some(join_parts(locale, &parts))
}
/// Clock-hour counter shared by complete times and optional temporal fields.
/// Duration hours use じかん and intentionally have a different suffix rule.
pub(super) fn japanese_hour(hour: u32) -> Option<String> {
    if hour > 23 {
        return None;
    }
    let h = match hour % 10 {
        0 if hour == 0 => "れい".into(),
        4 => replace_suffix(cardinal(SpeechLocale::Ja, hour)?, "よん", "よ"),
        7 => replace_suffix(cardinal(SpeechLocale::Ja, hour)?, "なな", "しち"),
        9 => replace_suffix(cardinal(SpeechLocale::Ja, hour)?, "きゅう", "く"),
        _ => cardinal(SpeechLocale::Ja, hour)?,
    };
    Some(format!("{h}じ"))
}

pub(super) fn japanese_minutes(n: u32) -> Option<String> {
    if n > 99 {
        return None;
    }
    if n == 0 {
        return Some("れいふん".into());
    }
    let u = n % 10;
    if u == 0 {
        let tens = cardinal(SpeechLocale::Ja, n)?;
        return tens
            .strip_suffix("じゅう")
            .map(|s| format!("{s}じゅっぷん"));
    }
    let units = [
        "",
        "いっぷん",
        "にふん",
        "さんぷん",
        "よんぷん",
        "ごふん",
        "ろっぷん",
        "ななふん",
        "はっぷん",
        "きゅうふん",
    ];
    if n < 10 {
        Some(units[u as usize].into())
    } else {
        Some(format!(
            "{}{}",
            cardinal(SpeechLocale::Ja, n - u)?,
            units[u as usize]
        ))
    }
}
pub(super) fn korean_native(n: u32) -> Option<String> {
    const U: [&str; 10] = [
        "영", "한", "두", "세", "네", "다섯", "여섯", "일곱", "여덟", "아홉",
    ];
    const T: [&str; 10] = [
        "", "열", "스물", "서른", "마흔", "쉰", "예순", "일흔", "여든", "아흔",
    ];
    if n > 99 {
        return None;
    }
    if n < 10 {
        return Some(U[n as usize].into());
    }
    if n == 20 {
        return Some("스무".into());
    }
    Some(format!(
        "{}{}",
        T[(n / 10) as usize],
        if n.is_multiple_of(10) {
            ""
        } else {
            U[(n % 10) as usize]
        }
    ))
}

pub(super) fn plural(locale: SpeechLocale, n: &NumberValue) -> Option<&'static str> {
    let i = n.integer.parse::<u64>().ok()?;
    let v = n.fraction.as_ref().map_or(0, String::len);
    let fraction_zero = n
        .fraction
        .as_ref()
        .is_none_or(|f| f.bytes().all(|b| b == b'0'));
    use SpeechLocale::*;
    Some(match locale {
        Zh | Id | Ja | Vi | Ko => "other",
        Ru if v == 0 => match (i % 10, i % 100) {
            (1, not11) if not11 != 11 => "one",
            (2..=4, not12_14) if !(12..=14).contains(&not12_14) => "few",
            _ => "many",
        },
        Ru => "other",
        Ar if fraction_zero => match i {
            0 => "zero",
            1 => "one",
            2 => "two",
            _ if (3..=10).contains(&(i % 100)) => "few",
            _ if (11..=99).contains(&(i % 100)) => "many",
            _ => "other",
        },
        Ar => "other",
        Fr | Pt if i == 0 || i == 1 => "one",
        Hi | Bn | Pa if i == 0 || (i == 1 && fraction_zero) => "one",
        En | De | Ur if i == 1 && v == 0 => "one",
        Es | Mr | Te | Tr | Ta if i == 1 && fraction_zero => "one",
        _ => "other",
    })
}

pub(super) fn unit_key(unit: &str) -> Option<&'static str> {
    Some(match unit {
        "m" | "length-meter" => "length-meter",
        "km" | "length-kilometer" => "length-kilometer",
        "g" | "mass-gram" => "mass-gram",
        "kg" | "mass-kilogram" => "mass-kilogram",
        "l" | "L" | "volume-liter" => "volume-liter",
        "ml" | "mL" | "volume-milliliter" => "volume-milliliter",
        "°C" | "temperature-celsius" => "temperature-celsius",
        "h" | "duration-hour" => "duration-hour",
        "min" | "duration-minute" => "duration-minute",
        "s" | "duration-second" => "duration-second",
        _ => return None,
    })
}
pub(super) fn quantity(locale: SpeechLocale, key: &str, n: &NumberValue) -> Option<String> {
    if key == "concentr-percent" {
        return percent(locale, n);
    }

    // Validate even CLDR one/dual patterns which omit the numeric placeholder.
    verbalize_number(n, locale, NumberReading::Cardinal)?;
    let category = plural(locale, n)?;
    let (pattern, gender) = unit_pattern(locale, key, category)?;
    // An implicit one/dual pattern cannot carry written decimal precision.
    // In particular, Arabic 1.000 and 2.000 must not collapse to ثانية/ثانيتان.
    let (pattern, gender) = if n.fraction.is_some() && !pattern.contains("{0}") {
        let explicit = unit_pattern(locale, key, "other")?;
        if !explicit.0.contains("{0}") {
            return None;
        }
        explicit
    } else {
        (pattern, gender)
    };
    if locale == SpeechLocale::Ja && n.fraction.is_none() {
        let sign = if n.negative { "マイナス" } else { "" };
        if key == "duration-hour" {
            let mut positive = n.clone();
            positive.negative = false;
            let number = verbalize_number(&positive, locale, NumberReading::Cardinal)?;
            return Some(format!(
                "{sign}{}じかん",
                replace_suffix(number, "よん", "よ")
            ));
        }
        if key == "duration-minute" {
            return Some(format!(
                "{sign}{}",
                japanese_minutes(n.integer.parse().ok()?)?
            ));
        }
    }
    if !pattern.contains("{0}") {
        // CLDR Arabic one/dual forms already contain the quantity. Retain
        // the sign even though the positive numeral itself is omitted.
        return Some(if n.negative {
            format!("سالب {pattern}")
        } else {
            pattern.to_string()
        });
    }
    let number = if locale == SpeechLocale::Ko && key == "duration-hour" && n.fraction.is_none() {
        let words = korean_native(n.integer.parse().ok()?)?;
        if n.negative {
            format!("마이너스 {words}")
        } else {
            words
        }
    } else {
        agreed_number(locale, n, gender)?
    };
    let result =
        noun_quantity_pattern(locale, n, pattern, &number).replace(['\u{00a0}', '\u{202f}'], " ");
    Some(
        if locale == SpeechLocale::Vi && key == "temperature-celsius" {
            result.replace("độ C", "độ xê")
        } else {
            result
        },
    )
}

fn percent(locale: SpeechLocale, n: &NumberValue) -> Option<String> {
    use SpeechLocale::*;
    // CLDR formatting patterns can contain an unexpanded percent symbol;
    // speech needs lexical forms, including languages with prefix order.
    let number = if matches!(locale, Es | Ar) {
        verbalize_number(n, locale, NumberReading::Cardinal)?
    } else {
        agreed_number(locale, n, Some("masculine"))?
    };
    Some(match locale {
        Zh => {
            let mut positive = n.clone();
            positive.negative = false;
            let words = verbalize_number(&positive, locale, NumberReading::Cardinal)?;
            format!("{}百分之{words}", if n.negative { "负" } else { "" })
        }
        Ar => format!("{number} بالمائة"),
        Bn => format!("{number} শতাংশ"),
        Vi => format!("{number} phần trăm"),
        Ko => format!("{number} 퍼센트"),
        Tr => format!("yüzde {number}"),
        _ => unit_pattern(locale, "concentr-percent", plural(locale, n)?)?
            .0
            .replace("{0}", &number)
            .replace(['\u{00a0}', '\u{202f}'], " "),
    })
}

pub(super) fn agreed_number(
    locale: SpeechLocale,
    n: &NumberValue,
    gender: Option<&str>,
) -> Option<String> {
    use SpeechLocale::*;
    let words = verbalize_number(n, locale, NumberReading::Cardinal)?;
    if n.fraction.is_some() {
        return Some(words);
    }
    let integer = n.integer.parse::<u64>().ok()?;
    let feminine = gender == Some("feminine");
    let mut result = words;
    match locale {
        Es if feminine => return verbalize_number_feminine(n, locale, NumberReading::Cardinal),
        Es => {
            result = replace_suffix(result, "veintiuno", "veintiún");
            result = replace_suffix(result, "uno", "un");
        }
        Pt if feminine => return verbalize_number_feminine(n, locale, NumberReading::Cardinal),
        Fr if feminine => result = replace_suffix(result, "un", "une"),
        De if integer == 1 => {
            result = replace_suffix(result, "eins", if feminine { "eine" } else { "ein" })
        }
        Ru if feminine => return verbalize_number_feminine(n, locale, NumberReading::Cardinal),
        Ar if feminine => {
            return verbalize_number_feminine(n, locale, NumberReading::Cardinal)
                .map(super::numbers::arabic_construct_count);
        }
        Ta if integer == 1 => {
            result = if n.negative {
                "மைனஸ் ஒரு".into()
            } else {
                "ஒரு".into()
            }
        }
        Te if integer == 1 => {
            result = if n.negative {
                "మైనస్ ఒక".into()
            } else {
                "ఒక".into()
            }
        }
        _ => {}
    }
    Some(result)
}
fn replace_suffix(value: String, from: &str, to: &str) -> String {
    value
        .strip_suffix(from)
        .map(|prefix| format!("{prefix}{to}"))
        .unwrap_or(value)
}
/// Grammatical currency gender shared by integer and fractional money readers.
pub(super) fn currency_gender(locale: SpeechLocale, code: &str) -> Option<&'static str> {
    use SpeechLocale::*;
    match locale {
        // Portuguese naira and rupee/lira-type currency nouns are feminine.
        // Spanish usage is mixed but the shared lexical name uses masculine
        // naira nigeriano, while French naira and German Naira are masculine;
        // NGN is therefore deliberately absent from those branches.
        Es | Pt if matches!(code, "GBP" | "INR" | "PKR" | "IDR" | "TRY") => Some("feminine"),
        Pt if code == "NGN" => Some("feminine"),
        Fr if matches!(code, "GBP" | "INR" | "PKR" | "IDR" | "TRY") => Some("feminine"),
        // Russian иена/вона/така/найра and rupee/lira-type nouns are feminine.
        Ru if matches!(
            code,
            "JPY" | "KRW" | "INR" | "PKR" | "IDR" | "TRY" | "BDT" | "NGN"
        ) =>
        {
            Some("feminine")
        }
        De if matches!(code, "INR" | "PKR" | "IDR" | "TRY") => Some("feminine"),
        _ => Some("masculine"),
    }
}
/// Place a semantic quantity into a noun pattern, including Romance scale links.
pub(super) fn noun_quantity_pattern(
    locale: SpeechLocale,
    n: &NumberValue,
    pattern: &str,
    number: &str,
) -> String {
    let exact_millions = matches!(locale, SpeechLocale::Es | SpeechLocale::Pt)
        && n.integer
            .parse::<u64>()
            .is_ok_and(|n| n >= 1_000_000 && n.is_multiple_of(1_000_000))
        && n.fraction
            .as_ref()
            .is_none_or(|f| f.bytes().all(|b| b == b'0'));
    let pattern =
        if exact_millions && pattern.starts_with("{0} ") && !pattern.starts_with("{0} de ") {
            pattern.replacen("{0} ", "{0} de ", 1)
        } else {
            pattern.to_string()
        };
    pattern.replace("{0}", number)
}

pub(super) fn money(locale: SpeechLocale, code: &str, n: &NumberValue) -> Option<String> {
    use SpeechLocale::*;
    // Arabic currency numerals need case-aware agreement with the currency
    // noun; CLDR display names alone cannot supply that grammar.
    if locale == Ar {
        return None;
    }
    if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_uppercase()) {
        return None;
    }
    if matches!(code, "JPY" | "KRW")
        && n.fraction
            .as_ref()
            .is_some_and(|f| f.bytes().any(|b| b != b'0'))
    {
        return None;
    }
    let gender = currency_gender(locale, code);
    if matches!(locale, En | Es)
        && let Some(fraction) = &n.fraction
    {
        if fraction.is_empty()
            || fraction.len() > 2
            || !fraction.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let mut integer = n.clone();
        integer.fraction = None;
        let major = money(locale, code, &integer)?;
        let cents = format!("{fraction:0<2}").parse::<u32>().ok()?;
        if cents == 0 {
            return Some(major);
        }
        let minor = match (locale, code) {
            (Es, "EUR" | "PEN") => {
                if cents == 1 {
                    "céntimo"
                } else {
                    "céntimos"
                }
            }
            (Es, "USD" | "BRL") => {
                if cents == 1 {
                    "centavo"
                } else {
                    "centavos"
                }
            }
            (Es, "GBP") => {
                if cents == 1 {
                    "penique"
                } else {
                    "peniques"
                }
            }
            (En, "USD" | "EUR" | "PEN" | "BRL") => {
                if cents == 1 {
                    "cent"
                } else {
                    "cents"
                }
            }
            (En, "GBP") => {
                if cents == 1 {
                    "penny"
                } else {
                    "pence"
                }
            }
            _ => return None,
        };
        let number = agreed_number(locale, &value(cents), Some("masculine"))?;
        return Some(format!(
            "{major} {} {number} {minor}",
            if locale == Es { "con" } else { "and" }
        ));
    }
    let category = plural(locale, n)?;
    let name = match (locale, code, category) {
        (Es, "PEN", "one") => "sol",
        (Es, "PEN", _) => "soles",
        _ => currency_name(locale, code, category)?,
    };
    let number = agreed_number(locale, n, gender)?;
    let name = if locale == De
        && name
            .split_whitespace()
            .next()
            .is_some_and(|w| w.ends_with("ischer") || w.ends_with("ische") || w.ends_with("isches"))
    {
        let mut chars = name.chars();
        format!("{}{}", chars.next()?.to_lowercase(), chars.as_str())
    } else {
        name.to_string()
    };
    Some(noun_quantity_pattern(
        locale,
        n,
        &format!("{{0}} {name}"),
        &number,
    ))
}
