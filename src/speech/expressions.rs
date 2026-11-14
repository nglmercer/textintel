//! Typed mathematical expressions over the common exact-number parser.
//!
//! Potentially ambiguous operator forms are enabled only in natural style.
//! The source parser owns protection of URLs, identifiers and code regions.
use serde::{Deserialize, Serialize};

use super::core::{ascii_digits, skip_space};
use super::numbers::verbalize_count_with_options;
use super::types::{
    NumberReading, NumberValue, ReadingStyle, SpeechInterpretation, SpeechLocale, SpeechOptions,
    SpeechValue,
};

/// An expression classified independently of its spoken representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "expression", rename_all = "snake_case")]
pub enum ExpressionValue {
    Scientific {
        mantissa: NumberValue,
        exponent: i32,
    },
    Fraction {
        numerator: NumberValue,
        denominator: NumberValue,
    },
    MixedFraction {
        whole: NumberValue,
        numerator: NumberValue,
        denominator: NumberValue,
    },
    Range {
        from: NumberValue,
        to: NumberValue,
    },
    Ordinal {
        value: u64,
        marker: String,
    },
    Duration {
        components: Vec<DurationComponent>,
    },
}

use super::shapes::{self, ShapePolicy, digits_end};
pub use super::shapes::{DurationComponent, DurationUnit};

pub fn kind(value: &ExpressionValue) -> &'static str {
    match value {
        ExpressionValue::Scientific { .. } => "scientific",
        ExpressionValue::Fraction { .. } | ExpressionValue::MixedFraction { .. } => "fraction",
        ExpressionValue::Range { .. } => "range",
        ExpressionValue::Ordinal { .. } => "ordinal",
        ExpressionValue::Duration { .. } => "duration",
    }
}

use super::core::number_is_zero as zero;

fn boundary(text: &str, end: usize) -> bool {
    super::core::right_boundary(text, end)
}
fn expression_boundary(text: &str, end: usize, options: &SpeechOptions) -> bool {
    boundary(text, end)
        || (matches!(
            options.interpret_as,
            SpeechInterpretation::Auto | SpeechInterpretation::Measure
        ) && super::quantities::unit_at(text, end, options).is_some())
}

/// Classify an expression at an original UTF-8 byte boundary.
/// Protected regions must be screened by the caller before invoking this hook.
pub fn parse_at(
    text: &str,
    start: usize,
    options: &SpeechOptions,
) -> Option<(usize, ExpressionValue)> {
    text.get(start..)?;
    if !super::core::left_boundary(text, start) {
        return None;
    }
    if let Some(ordinal) = parse_ordinal(text, start, options) {
        return Some(ordinal);
    }
    if options.interpret_as == SpeechInterpretation::Duration {
        return duration(text, start, options);
    }
    if options.reading_style != ReadingStyle::Natural {
        return None;
    }
    vulgar_fraction(text, start, options)
        .or_else(|| scientific(text, start, options))
        .or_else(|| fraction_or_range(text, start, options))
        .or_else(|| duration(text, start, options))
}

fn vulgar_fraction(
    text: &str,
    start: usize,
    options: &SpeechOptions,
) -> Option<(usize, ExpressionValue)> {
    let first = text[start..].chars().next()?;
    let (fraction_start, negative) = if matches!(first, '+' | '-' | '−') {
        (start + first.len_utf8(), first != '+')
    } else {
        (start, false)
    };
    let ch = text[fraction_start..].chars().next()?;
    let (n, d) = match ch {
        '¼' => (1, 4),
        '½' => (1, 2),
        '¾' => (3, 4),
        '⅐' => (1, 7),
        '⅑' => (1, 9),
        '⅒' => (1, 10),
        '⅓' => (1, 3),
        '⅔' => (2, 3),
        '⅕' => (1, 5),
        '⅖' => (2, 5),
        '⅗' => (3, 5),
        '⅘' => (4, 5),
        '⅙' => (1, 6),
        '⅚' => (5, 6),
        '⅛' => (1, 8),
        '⅜' => (3, 8),
        '⅝' => (5, 8),
        '⅞' => (7, 8),
        _ => return None,
    };
    let end = fraction_start + ch.len_utf8();
    expression_boundary(text, end, options).then(|| {
        let mut numerator = literal(n);
        numerator.negative = negative;
        (
            end,
            ExpressionValue::Fraction {
                numerator,
                denominator: literal(d),
            },
        )
    })
}

fn scientific(
    text: &str,
    start: usize,
    options: &SpeechOptions,
) -> Option<(usize, ExpressionValue)> {
    let shape = shapes::scientific(text, start, options, ShapePolicy::Typed)?;
    expression_boundary(text, shape.end, options).then_some((
        shape.end,
        ExpressionValue::Scientific {
            mantissa: shape.mantissa,
            exponent: shape.exponent,
        },
    ))
}

fn fraction_or_range(
    text: &str,
    start: usize,
    options: &SpeechOptions,
) -> Option<(usize, ExpressionValue)> {
    use shapes::RelationValue;
    let shape = shapes::relation(text, start, options, ShapePolicy::Typed)?;
    if !expression_boundary(text, shape.end, options) {
        return None;
    }
    let value = match shape.value {
        RelationValue::Fraction {
            numerator,
            denominator,
        } => ExpressionValue::Fraction {
            numerator,
            denominator,
        },
        RelationValue::MixedFraction {
            whole,
            numerator,
            denominator,
        } => ExpressionValue::MixedFraction {
            whole,
            numerator,
            denominator,
        },
        RelationValue::Range { from, to } => ExpressionValue::Range { from, to },
    };
    Some((shape.end, value))
}

fn parse_ordinal(
    text: &str,
    start: usize,
    options: &SpeechOptions,
) -> Option<(usize, ExpressionValue)> {
    use SpeechLocale::*;
    if matches!(options.locale, Zh | Ja) && text[start..].starts_with('第') {
        let from = start + '第'.len_utf8();
        let end = digits_end(text, from);
        let value = ascii_digits(&text[from..end])?.parse::<u64>().ok()?;
        if value == 0 || end == from {
            return None;
        }
        return Some((
            end,
            ExpressionValue::Ordinal {
                value,
                marker: "第".into(),
            },
        ));
    }
    for (language, prefix, marker) in [(Ko, "제", "제"), (Vi, "thứ ", "thứ"), (Id, "ke-", "ke")]
    {
        if options.locale == language && text[start..].starts_with(prefix) {
            let from = skip_space(text, start + prefix.len());
            let end = digits_end(text, from);
            let value = ascii_digits(&text[from..end])?.parse::<u64>().ok()?;
            if value > 0 && boundary(text, end) {
                return Some((
                    end,
                    ExpressionValue::Ordinal {
                        value,
                        marker: marker.into(),
                    },
                ));
            }
        }
    }
    let digits_end = digits_end(text, start);
    if digits_end == start {
        return None;
    }
    let value = ascii_digits(&text[start..digits_end])?
        .parse::<u64>()
        .ok()?;
    if value == 0 {
        return None;
    }
    let suffixes: &[&str] = match options.locale {
        En => &["st", "nd", "rd", "th"],
        Es | Pt => &[".º", ".ª", "º", "ª"],
        Fr => &["ième", "ème", "eme", "er", "re", "e"],
        De | Tr => &["."],
        Ko => &["번째"],
        Vi => &[],
        _ => &[],
    };
    for &marker in suffixes.iter().chain(["st", "nd", "rd", "th"].iter()) {
        let Some(source) = text[digits_end..].get(..marker.len()) else {
            continue;
        };
        if !source.eq_ignore_ascii_case(marker) {
            continue;
        }
        let end = digits_end + marker.len();
        if !boundary(text, end) {
            continue;
        }
        if matches!(marker, "st" | "nd" | "rd" | "th") {
            let expected = if (11..=13).contains(&(value % 100)) {
                "th"
            } else {
                match value % 10 {
                    1 => "st",
                    2 => "nd",
                    3 => "rd",
                    _ => "th",
                }
            };
            if !marker.eq_ignore_ascii_case(expected) {
                return None;
            }
        }
        if marker == "." {
            let next = skip_space(text, end);
            if next == end || !text[next..].chars().next().is_some_and(char::is_alphabetic) {
                return None;
            }
        }
        if options.locale == Fr && matches!(marker, "er" | "re") && value != 1 {
            return None;
        }
        return Some((
            end,
            ExpressionValue::Ordinal {
                value,
                marker: marker.into(),
            },
        ));
    }
    None
}

fn duration(text: &str, start: usize, options: &SpeechOptions) -> Option<(usize, ExpressionValue)> {
    let shape = shapes::duration(text, start, ShapePolicy::Typed)?;
    let minimum = if options.interpret_as == SpeechInterpretation::Duration {
        1
    } else {
        2
    };
    (shape.components.len() >= minimum && boundary(text, shape.end)).then_some((
        shape.end,
        ExpressionValue::Duration {
            components: shape.components,
        },
    ))
}

fn literal(value: u64) -> NumberValue {
    NumberValue {
        negative: false,
        integer: value.to_string(),
        fraction: None,
    }
}
fn number(value: &NumberValue, options: &SpeechOptions) -> Option<String> {
    verbalize_count_with_options(value, options)
}

/// Render an expression using the same exact-number and quantity core as spans.
pub fn render(value: &ExpressionValue, options: &SpeechOptions) -> Option<String> {
    render_with_gender(value, options, false)
}

/// Render with noun agreement for a measured mixed number or fraction.
pub fn render_with_gender(
    value: &ExpressionValue,
    options: &SpeechOptions,
    feminine: bool,
) -> Option<String> {
    match value {
        ExpressionValue::Scientific { mantissa, exponent } => {
            if exponent.unsigned_abs() > 9999 {
                return None;
            }
            let m = number(mantissa, options)?;
            let mut e = literal(exponent.unsigned_abs() as u64);
            e.negative = *exponent < 0;
            let e = number(&e, options)?;
            Some(scientific_template(options.locale, &m, &e))
        }
        ExpressionValue::Fraction {
            numerator,
            denominator,
        } => {
            if zero(denominator) {
                return None;
            }
            fraction_words(numerator, denominator, options, feminine)
        }
        ExpressionValue::MixedFraction {
            whole,
            numerator,
            denominator,
        } => {
            if zero(denominator)
                || numerator.negative
                || denominator.negative
                || whole.fraction.is_some()
                || numerator.fraction.is_some()
                || denominator.fraction.is_some()
            {
                return None;
            }
            let whole_words = if feminine
                && matches!(
                    options.locale,
                    SpeechLocale::Es | SpeechLocale::Pt | SpeechLocale::Ru
                ) {
                super::numbers::verbalize_number_feminine(
                    whole,
                    options.locale,
                    NumberReading::Cardinal,
                )?
            } else {
                number(whole, options)?
            };
            let fraction = fraction_words(numerator, denominator, options, feminine)?;
            Some(format!(
                "{whole_words} {} {fraction}",
                plus_word(options.locale)
            ))
        }
        ExpressionValue::Range { from, to } => Some(range_template(
            options.locale,
            &number(from, options)?,
            &number(to, options)?,
        )),
        ExpressionValue::Ordinal { value, marker } => ordinal(*value, marker, options),
        ExpressionValue::Duration { components } => {
            if components.is_empty() || components.len() > 3 {
                return None;
            }
            let mut last = 0;
            let mut pieces = Vec::new();
            for component in components {
                let (unit, order) = match component.unit {
                    DurationUnit::Hours => ("h", 1),
                    DurationUnit::Minutes => ("min", 2),
                    DurationUnit::Seconds => ("s", 3),
                };
                if order <= last {
                    return None;
                }
                last = order;
                let value = SpeechValue::Measure {
                    unit: unit.into(),
                    value: literal(component.value),
                };
                pieces.push(super::quantities::render(&value, "", options)?);
            }
            Some(pieces.join(" "))
        }
    }
}

/// Reduce a fraction or mixed number to exact rational components.
/// Mixed-number composition uses checked integer arithmetic; values outside
/// that arithmetic budget are unsupported rather than rounded or overflowed.
pub fn rational_components(value: &ExpressionValue) -> Option<(NumberValue, NumberValue)> {
    match value {
        ExpressionValue::Fraction {
            numerator,
            denominator,
        } => {
            let numerator = super::core::shift_decimal(numerator, 0)?;
            let denominator = super::core::shift_decimal(denominator, 0)?;
            (!zero(&denominator)).then_some((numerator, denominator))
        }
        ExpressionValue::MixedFraction {
            whole,
            numerator,
            denominator,
        } => {
            if whole.fraction.is_some()
                || numerator.fraction.is_some()
                || denominator.fraction.is_some()
                || numerator.negative
                || denominator.negative
            {
                return None;
            }
            let whole = super::core::canonical_count(whole);
            let w = whole.integer.parse::<u128>().ok()?;
            let n = numerator.integer.parse::<u128>().ok()?;
            let d = denominator
                .integer
                .parse::<u128>()
                .ok()
                .filter(|n| *n != 0)?;
            let n = w.checked_mul(d)?.checked_add(n)?;
            Some((
                NumberValue {
                    negative: whole.negative,
                    integer: n.to_string(),
                    fraction: None,
                },
                NumberValue {
                    negative: false,
                    integer: d.to_string(),
                    fraction: None,
                },
            ))
        }
        _ => None,
    }
}

fn plus_word(locale: SpeechLocale) -> &'static str {
    use SpeechLocale::*;
    match locale {
        En => "and",
        Es => "y",
        Zh => "加",
        Hi => "और",
        Ar => "و",
        Fr => "et",
        Pt => "e",
        Ru => "и",
        Bn => "ও",
        Ur => "اور",
        Id => "dan",
        De => "und",
        Ja => "と",
        Pa => "ਅਤੇ",
        Mr => "आणि",
        Te => "మరియు",
        Tr => "ve",
        Ta => "மற்றும்",
        Vi => "và",
        Ko => "더하기",
    }
}
fn fraction_words(
    numerator: &NumberValue,
    denominator: &NumberValue,
    options: &SpeechOptions,
    feminine: bool,
) -> Option<String> {
    if zero(denominator) {
        return None;
    }
    let n = number(numerator, options)?;
    let d = number(denominator, options)?;
    if options.reading_style == ReadingStyle::Natural
        && !denominator.negative
        && denominator.fraction.is_none()
        && numerator.fraction.is_none()
    {
        let denominator = denominator.integer.parse::<u64>().ok()?;
        let count = numerator.integer.parse::<u64>().ok()?;
        if options.locale == SpeechLocale::Es {
            let noun = match denominator {
                2 => {
                    if feminine {
                        "media"
                    } else {
                        "medio"
                    }
                }
                3 => "tercio",
                4 => "cuarto",
                5 => "quinto",
                6 => "sexto",
                7 => "séptimo",
                8 => "octavo",
                9 => "noveno",
                10 => "décimo",
                _ => return Some(fraction_template(options.locale, &n, &d)),
            };
            let sign = if numerator.negative { "menos " } else { "" };
            if count == 1 {
                return Some(format!(
                    "{sign}{}{noun}",
                    if denominator == 2 { "" } else { "un " }
                ));
            }
            return Some(format!("{} {noun}s", super::numbers::spanish_apocope(&n)));
        }
        if options.locale == SpeechLocale::En {
            let noun = match denominator {
                1 => {
                    if count == 1 {
                        "whole"
                    } else {
                        "wholes"
                    }
                }
                2 => {
                    if count == 1 {
                        "half"
                    } else {
                        "halves"
                    }
                }
                3 => {
                    if count == 1 {
                        "third"
                    } else {
                        "thirds"
                    }
                }
                4 => {
                    if count == 1 {
                        "quarter"
                    } else {
                        "quarters"
                    }
                }
                _ => {
                    let ordinal = ordinal(denominator, "th", options)?;
                    return Some(format!(
                        "{n} {ordinal}{}",
                        if count == 1 { "" } else { "s" }
                    ));
                }
            };
            return Some(format!("{n} {noun}"));
        }
    }
    Some(fraction_template(options.locale, &n, &d))
}

pub(super) fn scientific_template(locale: SpeechLocale, m: &str, e: &str) -> String {
    use SpeechLocale::*;
    match locale {
        En => format!("{m} times ten to the power of {e}"),
        Es => format!("{m} por diez elevado a {e}"),
        Zh => format!("{m}乘以十的{e}次方"),
        Hi => format!("{m} गुणा दस की घात {e}"),
        Ar => format!("{m} ضرب عشرة أس {e}"),
        Fr => format!("{m} fois dix puissance {e}"),
        Pt => format!("{m} vezes dez elevado a {e}"),
        Ru => format!("{m} умножить на десять в степени {e}"),
        Bn => format!("{m} গুণ দশের ঘাত {e}"),
        Ur => format!("{m} ضرب دس کی طاقت {e}"),
        Id => format!("{m} kali sepuluh pangkat {e}"),
        De => format!("{m} mal zehn hoch {e}"),
        Ja => format!("{m}掛ける十の{e}乗"),
        Pa => format!("{m} ਗੁਣਾ ਦਸ ਦੀ ਘਾਤ {e}"),
        Mr => format!("{m} गुणिले दहाचा घात {e}"),
        Te => format!("{m} గుణించు పది యొక్క {e} ఘాతం"),
        Tr => format!("{m} çarpı on üzeri {e}"),
        Ta => format!("{m} பெருக்கல் பத்தின் {e} அடுக்கு"),
        Vi => format!("{m} nhân mười mũ {e}"),
        Ko => format!("{m} 곱하기 십의 {e}승"),
    }
}
fn fraction_template(locale: SpeechLocale, n: &str, d: &str) -> String {
    use SpeechLocale::*;
    match locale {
        En => format!("{n} divided by {d}"),
        Es => format!("{n} dividido entre {d}"),
        Zh => format!("{n}除以{d}"),
        Hi => format!("{n} बटा {d}"),
        Ar => format!("{n} مقسوم على {d}"),
        Fr => format!("{n} divisé par {d}"),
        Pt => format!("{n} dividido por {d}"),
        Ru => format!("{n} делённое на {d}"),
        Bn => format!("{n} ভাগ {d}"),
        Ur => format!("{n} تقسیم بر {d}"),
        Id => format!("{n} dibagi {d}"),
        De => format!("{n} geteilt durch {d}"),
        Ja => format!("{n}割る{d}"),
        Pa => format!("{n} ਬਟਾ {d}"),
        Mr => format!("{n} भागिले {d}"),
        Te => format!("{n} భాగహారం {d}"),
        Tr => format!("{n} bölü {d}"),
        Ta => format!("{n} வகுத்தல் {d}"),
        Vi => format!("{n} chia {d}"),
        Ko => format!("{n} 나누기 {d}"),
    }
}
/// Divide a measured numerator by a scalar denominator. The unit belongs to
/// the numerator operand; appending it after a division would change scope.
/// Functional mathematical operator wording avoids noun/participle agreement
/// where a locale's scalar division wording assumes an abstract number.
pub(super) fn measured_division(locale: SpeechLocale, n: &str, d: &str) -> String {
    use SpeechLocale::*;
    match locale {
        // Infinitive operator wording avoids incorrectly agreeing a passive
        // participle with a singular/plural measured noun.
        Es => format!("dividir {n} entre {d}"),
        Fr => format!("diviser {n} par {d}"),
        Pt => format!("dividir {n} por {d}"),
        Ar => format!("{n} قسمة {d}"),
        Ru => format!("{n} разделить на {d}"),
        _ => fraction_template(locale, n, d),
    }
}

fn range_template(locale: SpeechLocale, a: &str, b: &str) -> String {
    use SpeechLocale::*;
    match locale {
        En => format!("from {a} to {b}"),
        Es => format!("de {a} a {b}"),
        Zh => format!("从{a}到{b}"),
        Hi => format!("{a} से {b} तक"),
        Ar => format!("من {a} إلى {b}"),
        Fr => format!("de {a} à {b}"),
        Pt => format!("de {a} a {b}"),
        Ru => format!("от {a} до {b}"),
        Bn => format!("{a} থেকে {b} পর্যন্ত"),
        Ur => format!("{a} سے {b} تک"),
        Id => format!("dari {a} sampai {b}"),
        De => format!("von {a} bis {b}"),
        Ja => format!("{a}から{b}まで"),
        Pa => format!("{a} ਤੋਂ {b} ਤੱਕ"),
        Mr => format!("{a} ते {b}"),
        Te => format!("{a} నుండి {b} వరకు"),
        Tr => format!("{a} ile {b} arası"),
        Ta => format!("{a} முதல் {b} வரை"),
        Vi => format!("từ {a} đến {b}"),
        Ko => format!("{a}부터 {b}까지"),
    }
}

/// Standalone ordinal rendering; grammatical case in a surrounding sentence is
/// handled by its caller. Unsupported inflection is reported rather than guessed.
pub fn ordinal(value: u64, marker: &str, options: &SpeechOptions) -> Option<String> {
    use SpeechLocale::*;
    if value == 0 {
        return None;
    }
    let words = number(&literal(value), options)?;
    Some(match options.locale {
        En => {
            let split = words.rfind([' ', '-']).map_or(0, |i| i + 1);
            let (head, last) = words.split_at(split);
            let tail = match last {
                "one" => "first".into(),
                "two" => "second".into(),
                "three" => "third".into(),
                "five" => "fifth".into(),
                "eight" => "eighth".into(),
                "nine" => "ninth".into(),
                "twelve" => "twelfth".into(),
                word => word
                    .strip_suffix('y')
                    .map_or_else(|| format!("{word}th"), |stem| format!("{stem}ieth")),
            };
            format!("{head}{tail}")
        }
        Es | Pt => {
            let tokens = romance_ordinal(value, options.locale)?;
            if marker.ends_with('ª') {
                tokens
                    .split_whitespace()
                    .map(|s| {
                        s.strip_suffix('o')
                            .map_or_else(|| s.to_string(), |base| format!("{base}a"))
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                tokens
            }
        }
        Fr => {
            if value == 1 {
                if marker == "re" {
                    "première".into()
                } else {
                    "premier".into()
                }
            } else {
                let split = words.rfind([' ', '-']).map_or(0, |i| i + 1);
                let (head, last) = words.split_at(split);
                let tail = match last {
                    "cinq" => "cinquième".into(),
                    "neuf" => "neuvième".into(),
                    other => format!("{}ième", other.strip_suffix('e').unwrap_or(other)),
                };
                format!("{head}{tail}")
            }
        }
        De => match value {
            1 => "erste".into(),
            2 => "zweite".into(),
            3 => "dritte".into(),
            7 => "siebte".into(),
            8 => "achte".into(),
            n if n < 20 => format!("{words}te"),
            _ => format!("{words}ste"),
        },
        Ja if options.number_notation == super::types::NumberNotation::Phonetic => {
            format!("だい{words}")
        }
        Zh | Ja => format!("第{words}"),
        Ko if marker == "번째" => {
            let native = if value == 1 {
                "첫".into()
            } else {
                super::verbalize::korean_native(value.try_into().ok()?)?
            };
            format!("{native} 번째")
        }
        Ko => format!("제{words}"),
        Id => format!("ke{words}"),
        Vi => format!(
            "thứ {}",
            match value {
                1 => "nhất",
                4 => "tư",
                _ => &words,
            }
        ),
        Tr => turkish_ordinal(&words)?,
        _ => super::numbers::basic_ordinal(value, options.locale)?,
    })
}
fn romance_ordinal(value: u64, locale: SpeechLocale) -> Option<String> {
    if value > 999 {
        return None;
    }
    let (ones, tens, hundreds) = if locale == SpeechLocale::Es {
        (
            [
                "", "primero", "segundo", "tercero", "cuarto", "quinto", "sexto", "séptimo",
                "octavo", "noveno",
            ],
            [
                "",
                "décimo",
                "vigésimo",
                "trigésimo",
                "cuadragésimo",
                "quincuagésimo",
                "sexagésimo",
                "septuagésimo",
                "octogésimo",
                "nonagésimo",
            ],
            [
                "",
                "centésimo",
                "ducentésimo",
                "tricentésimo",
                "cuadringentésimo",
                "quingentésimo",
                "sexcentésimo",
                "septingentésimo",
                "octingentésimo",
                "noningentésimo",
            ],
        )
    } else {
        (
            [
                "", "primeiro", "segundo", "terceiro", "quarto", "quinto", "sexto", "sétimo",
                "oitavo", "nono",
            ],
            [
                "",
                "décimo",
                "vigésimo",
                "trigésimo",
                "quadragésimo",
                "quinquagésimo",
                "sexagésimo",
                "septuagésimo",
                "octogésimo",
                "nonagésimo",
            ],
            [
                "",
                "centésimo",
                "ducentésimo",
                "tricentésimo",
                "quadringentésimo",
                "quingentésimo",
                "sexcentésimo",
                "septingentésimo",
                "octingentésimo",
                "noningentésimo",
            ],
        )
    };
    let mut parts = Vec::new();
    if value >= 100 {
        parts.push(hundreds[(value / 100) as usize]);
    }
    if value % 100 >= 10 {
        parts.push(tens[((value % 100) / 10) as usize]);
    }
    if !value.is_multiple_of(10) {
        parts.push(ones[(value % 10) as usize]);
    }
    Some(parts.join(" "))
}
fn turkish_ordinal(words: &str) -> Option<String> {
    let split = words.rfind(' ').map_or(0, |i| i + 1);
    let (head, last) = words.split_at(split);
    let tail = match last {
        "bir" => "birinci",
        "iki" => "ikinci",
        "üç" => "üçüncü",
        "dört" => "dördüncü",
        "beş" => "beşinci",
        "altı" => "altıncı",
        "yedi" => "yedinci",
        "sekiz" => "sekizinci",
        "dokuz" => "dokuzuncu",
        "on" => "onuncu",
        "yirmi" => "yirminci",
        "otuz" => "otuzuncu",
        "kırk" => "kırkıncı",
        "elli" => "ellinci",
        "altmış" => "altmışıncı",
        "yetmiş" => "yetmişinci",
        "seksen" => "sekseninci",
        "doksan" => "doksanıncı",
        "yüz" => "yüzüncü",
        "bin" => "bininci",
        "milyon" => "milyonuncu",
        "milyar" => "milyarıncı",
        _ => return None,
    };
    Some(format!("{head}{tail}"))
}
