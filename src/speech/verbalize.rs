//! Deterministic spoken readings over lossless typed spans.
//!
//! This path does not apply similarity normalization, case folding, rebus
//! decoding, or leetspeak rewrites. Unsupported structured values retain their
//! source text and report a reason instead of inventing a pronunciation.

use super::lexicons::{currency_name, month, unit_pattern};
use super::numbers::{verbalize_number, verbalize_number_feminine};
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
    verbalize_document(&parse_for_speech(text, options)?)
}

/// Verbalize a document, rejecting externally constructed invalid span maps.
/// The complete original text must be partitioned once, in UTF-8 byte order.
pub fn verbalize_document(
    document: &SpeechDocument,
) -> Result<SpeechNormalization, TextIntelError> {
    let mut cursor = 0;
    let mut text = String::new();
    let mut readings = Vec::with_capacity(document.spans.len());
    let mut fully_supported = true;
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
            SpeechValue::Number { value, reading } => (verbalize_number(value, document.locale, *reading), Some("number form or range is not supported for this locale".to_string())),
            SpeechValue::Date { year, month, day } => (date(document.locale, *year, *month, *day), Some("date value or date grammar is not supported for this locale".to_string())),
            SpeechValue::Time { hour, minute, second } => (time(document.locale, *hour, *minute, *second), Some("time value or time grammar is not supported for this locale".to_string())),
            SpeechValue::Currency { code, value } => (money(document.locale, code, value), Some("currency, fractional precision, or currency grammar is not supported for this locale".to_string())),
            SpeechValue::Percent { value } => (quantity(document.locale, "concentr-percent", value), Some("percent grammar or number range is not supported for this locale".to_string())),
            SpeechValue::Measure { unit, value } => (unit_key(unit).and_then(|key| quantity(document.locale, key, value)), Some("unit, agreement, or number range is not supported for this locale".to_string())),
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
            reason: if supported { None } else { reason },
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

fn value(integer: u32) -> NumberValue {
    NumberValue {
        negative: false,
        integer: integer.to_string(),
        fraction: None,
    }
}

fn cardinal(locale: SpeechLocale, n: u32) -> Option<String> {
    verbalize_number(&value(n), locale, NumberReading::Cardinal)
}

fn valid_date(year: u32, month: u32, day: u32) -> bool {
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let last = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    };
    (1..=9999).contains(&year) && (1..=last).contains(&day)
}

fn date(locale: SpeechLocale, year: u32, m: u32, day: u32) -> Option<String> {
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
        Ja => {
            let year = year_words
                .strip_suffix("よん")
                .map(|prefix| format!("{prefix}よ"))
                .unwrap_or(year_words);
            format!("{year}ねん{}{}", japanese_month(m)?, japanese_day(day)?)
        }
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

fn english_day(day: u32) -> Option<&'static str> {
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
fn german_day(day: u32) -> Option<&'static str> {
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
fn japanese_month(month: u32) -> Option<&'static str> {
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
fn japanese_day(day: u32) -> Option<String> {
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
        20 => Some("はつか"),
        24 => Some("にじゅうよっか"),
        _ => None,
    };
    irregular
        .map(str::to_string)
        .or_else(|| cardinal(SpeechLocale::Ja, day).map(|v| format!("{v}にち")))
}
fn korean_month(month: u32) -> Option<String> {
    match month {
        6 => Some("유월".into()),
        10 => Some("시월".into()),
        1..=12 => cardinal(SpeechLocale::Ko, month).map(|v| format!("{v}월")),
        _ => None,
    }
}
fn russian_day(day: u32) -> Option<&'static str> {
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
fn russian_year(year: u32) -> Option<String> {
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
fn join_parts(locale: SpeechLocale, parts: &[String]) -> String {
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
fn time(locale: SpeechLocale, hour: u32, minute: u32, second: Option<u32>) -> Option<String> {
    use SpeechLocale::*;
    if hour > 23 || minute > 59 || second.is_some_and(|s| s > 59) {
        return None;
    }
    if locale == Ja {
        let h = match hour % 10 {
            0 if hour == 0 => "れい".into(),
            4 => cardinal(locale, hour)?
                .strip_suffix("よん")
                .map(|s| format!("{s}よ"))?,
            7 => cardinal(locale, hour)?
                .strip_suffix("なな")
                .map(|s| format!("{s}しち"))?,
            9 => cardinal(locale, hour)?
                .strip_suffix("きゅう")
                .map(|s| format!("{s}く"))?,
            _ => cardinal(locale, hour)?,
        };
        let mut s = format!("{h}じ{}", japanese_minutes(minute)?);
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
        let mut s = format!("{h}点{}分", cardinal(locale, minute)?);
        if let Some(sec) = second {
            s.push_str(&format!("{}秒", cardinal(locale, sec)?));
        }
        return Some(s);
    }
    if locale == Ko {
        let mut s = format!(
            "{} 시 {} 분",
            korean_native(hour)?,
            cardinal(locale, minute)?
        );
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
        let mut parts = vec![
            format!("الساعة {}", HOURS[hour as usize]),
            quantity(locale, "duration-minute", &value(minute))?,
        ];
        if let Some(sec) = second {
            parts.push(quantity(locale, "duration-second", &value(sec))?);
        }
        return Some(join_parts(locale, &parts));
    }
    let mut parts = vec![
        quantity(locale, "duration-hour", &value(hour))?,
        quantity(locale, "duration-minute", &value(minute))?,
    ];
    if let Some(sec) = second {
        parts.push(quantity(locale, "duration-second", &value(sec))?);
    }
    Some(join_parts(locale, &parts))
}
fn japanese_minutes(n: u32) -> Option<String> {
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
fn korean_native(n: u32) -> Option<String> {
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

fn plural(locale: SpeechLocale, n: &NumberValue) -> Option<&'static str> {
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

fn unit_key(unit: &str) -> Option<&'static str> {
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
fn quantity(locale: SpeechLocale, key: &str, n: &NumberValue) -> Option<String> {
    if key == "concentr-percent" {
        return percent(locale, n);
    }

    // Validate even CLDR one/dual patterns which omit the numeric placeholder.
    verbalize_number(n, locale, NumberReading::Cardinal)?;
    let category = plural(locale, n)?;
    let (pattern, gender) = unit_pattern(locale, key, category)?;
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
    let result = pattern
        .replace("{0}", &number)
        .replace(['\u{00a0}', '\u{202f}'], " ");
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

fn agreed_number(locale: SpeechLocale, n: &NumberValue, gender: Option<&str>) -> Option<String> {
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
            let word = arabic_feminine(u32::try_from(integer).ok()?)?;
            result = if n.negative {
                format!("سالب {word}")
            } else {
                word
            };
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
fn arabic_feminine(n: u32) -> Option<String> {
    const SMALL: [&str; 20] = [
        "صفر",
        "واحدة",
        "اثنتان",
        "ثلاث",
        "أربع",
        "خمس",
        "ست",
        "سبع",
        "ثمان",
        "تسع",
        "عشر",
        "إحدى عشرة",
        "اثنتا عشرة",
        "ثلاث عشرة",
        "أربع عشرة",
        "خمس عشرة",
        "ست عشرة",
        "سبع عشرة",
        "ثماني عشرة",
        "تسع عشرة",
    ];
    const TENS: [&str; 10] = [
        "",
        "",
        "عشرون",
        "ثلاثون",
        "أربعون",
        "خمسون",
        "ستون",
        "سبعون",
        "ثمانون",
        "تسعون",
    ];
    if n < 20 {
        return Some(SMALL[n as usize].into());
    }
    if n > 99 {
        return None;
    }
    if n.is_multiple_of(10) {
        Some(TENS[(n / 10) as usize].into())
    } else {
        Some(format!(
            "{} و{}",
            if n % 10 == 1 {
                "إحدى"
            } else {
                SMALL[(n % 10) as usize]
            },
            TENS[(n / 10) as usize]
        ))
    }
}

fn money(locale: SpeechLocale, code: &str, n: &NumberValue) -> Option<String> {
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
    let gender = match locale {
        Es | Pt | Fr if matches!(code, "GBP" | "INR" | "PKR" | "IDR" | "TRY") => Some("feminine"),
        Ru if matches!(code, "JPY" | "KRW" | "INR" | "PKR" | "IDR" | "TRY") => Some("feminine"),
        De if matches!(code, "INR" | "PKR" | "IDR" | "TRY") => Some("feminine"),
        _ => Some("masculine"),
    };
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
    Some(format!("{number} {name}"))
}
