//! Explicit grammatical inflections over shared exact numeric values.
//!
//! Russian genitive lexical forms and scale selection derive from Unicode
//! CLDR 48, `%spellout-cardinal-masculine-genitive` and its feminine variant:
//! <https://github.com/unicode-org/cldr/blob/release-48/common/rbnf/ru.xml>.
//! Copyright © 1991–2025 Unicode, Inc. See `data/LICENSE-UNICODE.txt`.
//! Pronunciation-oriented forms retain ё in трёх/четырёх and their compounds.
//! This module does not infer grammatical case from ordinary prose or IDs.
use serde::{Deserialize, Serialize};

use super::core::{canonical_count, left_boundary, parse_number, right_boundary, scan_numeric_end};
use super::numbers::{MAX_SPOKEN_DIGITS, cardinal_max};
use super::types::{SpeechInterpretation, SpeechLocale, SpeechOptions, SpeechValue};

/// Grammatical form explicitly carried by a parsed numeric value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumberInflection {
    Genitive,
}

/// Recognize an unambiguous Russian numeric `-х` ending in an explicit domain.
/// The caller screens protected code/URL regions before invoking this hook.
/// Auto mode leaves identifier-like source intact. Case-looking forms whose
/// endings do not support this genitive reading are preserved as ambiguous.
pub fn parse_at(text: &str, start: usize, options: &SpeechOptions) -> Option<(usize, SpeechValue)> {
    if options.locale != SpeechLocale::Ru
        || !matches!(
            options.interpret_as,
            SpeechInterpretation::Measure | SpeechInterpretation::Number
        )
        || text.get(start..).is_none()
        || !left_boundary(text, start)
    {
        return None;
    }
    let numeric_end = scan_numeric_end(text, start);
    if numeric_end <= start
        || !text[..numeric_end].ends_with('-')
        || !text[numeric_end..].starts_with('х')
    {
        return None;
    }
    let end = numeric_end + 'х'.len_utf8();
    if !right_boundary(text, end) || text[end..].starts_with(['-', '/']) {
        return None;
    }
    let value = match parse_number(&text[start..numeric_end - 1], options) {
        Ok(value) => value,
        Err(reason) => {
            return Some((
                end,
                SpeechValue::Ambiguous {
                    reason: reason.into(),
                },
            ));
        }
    };
    if value.integer.len() > MAX_SPOKEN_DIGITS {
        return Some((
            end,
            SpeechValue::Ambiguous {
                reason: "numeric form exceeds the supported inflection budget".into(),
            },
        ));
    }
    let value = canonical_count(&value);
    let valid = value.fraction.is_none()
        && value.integer.parse::<u64>().ok().is_some_and(|n| {
            n <= cardinal_max(SpeechLocale::Ru)
                && (2..=4).contains(&(n % 10))
                && !(12..=14).contains(&(n % 100))
        });
    if valid {
        Some((
            end,
            SpeechValue::InflectedNumber {
                value,
                inflection: NumberInflection::Genitive,
            },
        ))
    } else {
        Some((end, SpeechValue::Ambiguous {
            reason: "numeric suffix does not identify an unambiguous supported cardinal genitive; cardinal/ordinal case needs explicit context".into(),
        }))
    }
}

/// Render an explicitly inflected numeric value. Russian integer genitives
/// cover the same standalone bound as the shared cardinal renderer. Fractions,
/// malformed values, other languages and unrelated span kinds are unsupported.
pub fn render(value: &SpeechValue, options: &SpeechOptions) -> Option<String> {
    if options.locale != SpeechLocale::Ru {
        return None;
    }
    let SpeechValue::InflectedNumber {
        value,
        inflection: NumberInflection::Genitive,
    } = value
    else {
        return None;
    };
    if value.integer.is_empty()
        || value.integer.len() > MAX_SPOKEN_DIGITS
        || !value.integer.bytes().all(|b| b.is_ascii_digit())
        || value.fraction.is_some()
    {
        return None;
    }
    let n = value.integer.parse::<u64>().ok()?;
    if n > cardinal_max(SpeechLocale::Ru) {
        return None;
    }
    let mut remainder = n;
    let mut pieces = Vec::new();
    for (scale, one, other, feminine) in [
        (1_000_000_000_000, "триллиона", "триллионов", false),
        (1_000_000_000, "миллиарда", "миллиардов", false),
        (1_000_000, "миллиона", "миллионов", false),
        (1_000, "тысячи", "тысяч", true),
    ] {
        let group = remainder / scale;
        remainder %= scale;
        if group == 0 {
            continue;
        }
        pieces.push(under_thousand(u16::try_from(group).ok()?, feminine)?);
        let singular = group % 10 == 1 && group % 100 != 11;
        pieces.push(if singular { one } else { other }.to_string());
    }
    if remainder != 0 || pieces.is_empty() {
        pieces.push(under_thousand(u16::try_from(remainder).ok()?, false)?);
    }
    let words = pieces.join(" ");
    Some(if value.negative {
        format!("минус {words}")
    } else {
        words
    })
}

// Genitive morphology is distinct lexical data, not a string replacement over
// nominative output: e.g. сто→ста, двести→двухсот, сорок→сорока.
fn under_thousand(n: u16, feminine: bool) -> Option<String> {
    if n > 999 {
        return None;
    }
    let mut parts = Vec::new();
    let hundred = n / 100;
    if hundred == 1 {
        parts.push("ста".to_string());
    } else if hundred > 1 {
        parts.push(format!("{}сот", SMALL[hundred as usize]));
    }
    let remainder = usize::from(n % 100);
    if remainder != 0 || parts.is_empty() {
        if remainder < 20 {
            parts.push(
                if remainder == 1 && feminine {
                    "одной"
                } else {
                    SMALL[remainder]
                }
                .to_string(),
            );
        } else {
            parts.push(TENS[remainder / 10].to_string());
            let unit = remainder % 10;
            if unit != 0 {
                parts.push(
                    if unit == 1 && feminine {
                        "одной"
                    } else {
                        SMALL[unit]
                    }
                    .to_string(),
                );
            }
        }
    }
    Some(parts.join(" "))
}
const SMALL: [&str; 20] = [
    "ноля",
    "одного",
    "двух",
    "трёх",
    "четырёх",
    "пяти",
    "шести",
    "семи",
    "восьми",
    "девяти",
    "десяти",
    "одиннадцати",
    "двенадцати",
    "тринадцати",
    "четырнадцати",
    "пятнадцати",
    "шестнадцати",
    "семнадцати",
    "восемнадцати",
    "девятнадцати",
];
const TENS: [&str; 10] = [
    "",
    "",
    "двадцати",
    "тридцати",
    "сорока",
    "пятидесяти",
    "шестидесяти",
    "семидесяти",
    "восьмидесяти",
    "девяноста",
];
