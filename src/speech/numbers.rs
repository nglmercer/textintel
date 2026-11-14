//! Bounded exact-string number verbalization. No inference or floating point.
//!
//! Lexical tables and their provenance live in `data/`. Cardinal readings are
//! standalone nominative forms, not a promise of grammatical agreement with an
//! arbitrary following noun. Callers must handle noun gender/case separately.
use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;

use super::types::{NumberReading, NumberValue, SpeechLocale};

/// Maximum digits accepted in either the integer or fractional component.
/// Longer identifiers need application-specific chunking, not unbounded output.
pub const MAX_SPOKEN_DIGITS: usize = 128;

#[derive(Deserialize)]
struct Lexicon {
    small: Vec<String>,
    hundreds: Vec<String>,
    hundreds_prefix: Vec<String>,
    hundreds_tens_prefix: Vec<String>,
    thousands: Vec<String>,
    thousands_prefix: Vec<String>,
}

fn lexicon(locale: SpeechLocale) -> &'static Lexicon {
    static DATA: OnceLock<BTreeMap<String, Lexicon>> = OnceLock::new();
    &DATA.get_or_init(|| {
        // Build-owned embedded data is checked exhaustively by resource tests.
        serde_json::from_str(include_str!("data/numbers.json"))
            .expect("valid speech number lexicons")
    })[locale.code()]
}

/// Highest supported *standalone* cardinal. This does not certify currencies,
/// counters, grammatical case, regional variants, or pronunciation quality.
pub const fn cardinal_max(locale: SpeechLocale) -> u64 {
    match locale {
        SpeechLocale::Ar | SpeechLocale::Ta | SpeechLocale::Te => 9_999,
        SpeechLocale::Hi
        | SpeechLocale::Bn
        | SpeechLocale::Ur
        | SpeechLocale::Pa
        | SpeechLocale::Mr => 999_999_999,
        _ => 999_999_999_999,
    }
}

fn valid_digits(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SPOKEN_DIGITS
        && value.bytes().all(|b| b.is_ascii_digit())
}

fn digit_words(value: &str, locale: SpeechLocale) -> String {
    let words = &lexicon(locale).small;
    value
        .bytes()
        .map(|b| words[(b - b'0') as usize].as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn tight(locale: SpeechLocale) -> bool {
    matches!(locale, SpeechLocale::Zh | SpeechLocale::Ja)
}

fn sign_word(locale: SpeechLocale) -> &'static str {
    match locale {
        SpeechLocale::En => "minus",
        SpeechLocale::Es => "menos",
        SpeechLocale::Zh => "负",
        SpeechLocale::Hi => "ऋण",
        SpeechLocale::Ar => "سالب",
        SpeechLocale::Fr => "moins",
        SpeechLocale::Pt => "menos",
        SpeechLocale::Ru => "минус",
        SpeechLocale::Bn => "ঋণাত্মক",
        SpeechLocale::Ur => "منفی",
        SpeechLocale::Id => "minus",
        SpeechLocale::De => "minus",
        SpeechLocale::Ja => "マイナス",
        SpeechLocale::Pa => "ਮਾਇਨਸ",
        SpeechLocale::Mr => "ऋण",
        SpeechLocale::Te => "మైనస్",
        SpeechLocale::Tr => "eksi",
        SpeechLocale::Ta => "மைனஸ்",
        SpeechLocale::Vi => "âm",
        SpeechLocale::Ko => "마이너스",
    }
}

fn point_word(locale: SpeechLocale) -> &'static str {
    match locale {
        SpeechLocale::En => "point",
        SpeechLocale::Es => "coma",
        SpeechLocale::Zh => "点",
        SpeechLocale::Hi => "दशमलव",
        SpeechLocale::Ar => "فاصلة",
        SpeechLocale::Fr => "virgule",
        SpeechLocale::Pt => "vírgula",
        SpeechLocale::Ru => "запятая",
        SpeechLocale::Bn => "দশমিক",
        SpeechLocale::Ur => "اعشاریہ",
        SpeechLocale::Id => "koma",
        SpeechLocale::De => "Komma",
        SpeechLocale::Ja => "てん",
        SpeechLocale::Pa => "ਦਸ਼ਮਲਵ",
        SpeechLocale::Mr => "दशांश",
        SpeechLocale::Te => "దశాంశం",
        SpeechLocale::Tr => "virgül",
        SpeechLocale::Ta => "புள்ளி",
        SpeechLocale::Vi => "phẩy",
        SpeechLocale::Ko => "점",
    }
}

/// Read canonical ASCII decimal components, preserving every fractional zero.
/// Leading-zero cardinal inputs are rejected: use `Digits` for identifiers.
/// `None` means invalid input or a magnitude outside this locale's capability.
pub fn verbalize_number(
    value: &NumberValue,
    locale: SpeechLocale,
    reading: NumberReading,
) -> Option<String> {
    if !valid_digits(&value.integer) || value.fraction.as_ref().is_some_and(|s| !valid_digits(s)) {
        return None;
    }
    let mut body = match reading {
        NumberReading::Digits => digit_words(&value.integer, locale),
        NumberReading::Cardinal => {
            if value.integer.len() > 1 && value.integer.starts_with('0') {
                return None;
            }
            let number = value.integer.parse::<u64>().ok()?;
            if number > cardinal_max(locale) {
                return None;
            }
            cardinal(number, locale)
        }
    };
    let separator = if tight(locale) { "" } else { " " };
    if let Some(fraction) = &value.fraction {
        let digits = digit_words(fraction, locale);
        let digits = if tight(locale) {
            digits.replace(' ', "")
        } else {
            digits
        };
        body.push_str(separator);
        body.push_str(point_word(locale));
        body.push_str(separator);
        body.push_str(&digits);
    }
    if value.negative {
        body = format!("{}{separator}{body}", sign_word(locale));
    }
    Some(body)
}

/// Feminine nominative forms for es/pt/ru integers through 999 only. Other
/// contexts deliberately remain unsupported instead of replacing arbitrary
/// substrings inside a longer sentence or monetary phrase.
pub fn verbalize_number_feminine(
    value: &NumberValue,
    locale: SpeechLocale,
    reading: NumberReading,
) -> Option<String> {
    if reading == NumberReading::Digits {
        return verbalize_number(value, locale, reading);
    }
    if value.fraction.is_some() || value.integer.parse::<u64>().ok()? > 999 {
        return None;
    }
    let mut words = verbalize_number(value, locale, reading)?;
    match locale {
        SpeechLocale::Es => {
            for (masc, fem) in [
                ("doscientos", "doscientas"),
                ("trescientos", "trescientas"),
                ("cuatrocientos", "cuatrocientas"),
                ("quinientos", "quinientas"),
                ("seiscientos", "seiscientas"),
                ("setecientos", "setecientas"),
                ("ochocientos", "ochocientas"),
                ("novecientos", "novecientas"),
            ] {
                words = words.replace(masc, fem);
            }
            if let Some(prefix) = words.strip_suffix("uno") {
                words = format!("{prefix}una");
            }
        }
        SpeechLocale::Pt => {
            words = words
                .split_whitespace()
                .map(|w| match w {
                    "um" => "uma",
                    "dois" => "duas",
                    "duzentos" => "duzentas",
                    "trezentos" => "trezentas",
                    "quatrocentos" => "quatrocentas",
                    "quinhentos" => "quinhentas",
                    "seiscentos" => "seiscentas",
                    "setecentos" => "setecentas",
                    "oitocentos" => "oitocentas",
                    "novecentos" => "novecentas",
                    _ => w,
                })
                .collect::<Vec<_>>()
                .join(" ");
        }
        SpeechLocale::Ru => words = feminine_ru(&words),
        _ => return None,
    }
    Some(words)
}

fn feminine_ru(words: &str) -> String {
    if let Some(prefix) = words.strip_suffix("один") {
        format!("{prefix}одна")
    } else if let Some(prefix) = words.strip_suffix("два") {
        format!("{prefix}две")
    } else {
        words.into()
    }
}

fn cardinal(n: u64, locale: SpeechLocale) -> String {
    if locale == SpeechLocale::Zh {
        return chinese(n);
    }
    if matches!(locale, SpeechLocale::Ja | SpeechLocale::Ko) && n >= 10_000 {
        return east_asian(n, locale);
    }
    if n < 100 {
        return lexicon(locale).small[n as usize].clone();
    }
    if n < 1_000 {
        let q = (n / 100) as usize;
        let r = n % 100;
        let pack = lexicon(locale);
        if r == 0 {
            return pack.hundreds[q].clone();
        }
        let prefix = if locale == SpeechLocale::Vi && r >= 10 {
            &pack.hundreds_tens_prefix[q]
        } else {
            &pack.hundreds_prefix[q]
        };
        return format!("{prefix}{}", cardinal(r, locale));
    }
    if n < 10_000 {
        let q = (n / 1_000) as usize;
        let r = n % 1_000;
        let pack = lexicon(locale);
        if r == 0 {
            return pack.thousands[q].clone();
        }
        if locale == SpeechLocale::Pt {
            let join = if r < 100 || r.is_multiple_of(100) {
                " e "
            } else {
                " "
            };
            return format!("{}{join}{}", pack.thousands[q], cardinal(r, locale));
        }
        if locale == SpeechLocale::Vi {
            let rest = if r < 10 {
                format!("không trăm lẻ {}", cardinal(r, locale))
            } else if r < 100 {
                format!("không trăm {}", cardinal(r, locale))
            } else {
                cardinal(r, locale)
            };
            return format!("{} {rest}", pack.thousands[q]);
        }
        return format!("{}{}", pack.thousands_prefix[q], cardinal(r, locale));
    }
    large(n, locale)
}

/// Spanish apocope before masculine scale nouns, including the changed accent.
fn spanish_apocope(words: &str) -> String {
    if let Some(prefix) = words.strip_suffix("veintiuno") {
        format!("{prefix}veintiún")
    } else if let Some(prefix) = words.strip_suffix("uno") {
        format!("{prefix}un")
    } else {
        words.into()
    }
}

fn russian_scale(q: u64, one: &str, few: &str, many: &str) -> String {
    if (11..=14).contains(&(q % 100)) {
        many.into()
    } else {
        match q % 10 {
            1 => one.into(),
            2..=4 => few.into(),
            _ => many.into(),
        }
    }
}

fn large(n: u64, locale: SpeechLocale) -> String {
    use SpeechLocale::*;
    let scales: &[(u64, &str)] = match locale {
        Hi => &[(10_000_000, "करोड़"), (100_000, "लाख"), (1_000, "हज़ार")],
        Bn => &[(10_000_000, "কোটি"), (100_000, "লাখ"), (1_000, "হাজার")],
        Ur => &[(10_000_000, "کروڑ"), (100_000, "لاکھ"), (1_000, "ہزار")],
        Pa => &[(10_000_000, "ਕਰੋੜ"), (100_000, "ਲੱਖ"), (1_000, "ਹਜ਼ਾਰ")],
        Mr => &[(10_000_000, "कोटी"), (100_000, "लाख"), (1_000, "हजार")],
        Es => &[(1_000_000, "millón"), (1_000, "mil")],
        En => &[
            (1_000_000_000, "billion"),
            (1_000_000, "million"),
            (1_000, "thousand"),
        ],
        Fr => &[
            (1_000_000_000, "milliard"),
            (1_000_000, "million"),
            (1_000, "mille"),
        ],
        Pt => &[
            (1_000_000_000, "bilhão"),
            (1_000_000, "milhão"),
            (1_000, "mil"),
        ],
        De => &[
            (1_000_000_000, "Milliarde"),
            (1_000_000, "Million"),
            (1_000, "tausend"),
        ],
        Ru => &[
            (1_000_000_000, "миллиард"),
            (1_000_000, "миллион"),
            (1_000, "тысяча"),
        ],
        Id => &[
            (1_000_000_000, "miliar"),
            (1_000_000, "juta"),
            (1_000, "ribu"),
        ],
        Tr => &[
            (1_000_000_000, "milyar"),
            (1_000_000, "milyon"),
            (1_000, "bin"),
        ],
        Vi => &[
            (1_000_000_000, "tỷ"),
            (1_000_000, "triệu"),
            (1_000, "nghìn"),
        ],
        _ => unreachable!("large-number range is checked before dispatch"),
    };
    let &(base, label) = scales
        .iter()
        .find(|(base, _)| n >= *base)
        .expect("large scale exists");
    let q = n / base;
    let r = n % base;
    let quantity = cardinal(q, locale);
    let head = match locale {
        Es if base == 1_000_000 => {
            if q == 1 {
                "un millón".into()
            } else {
                format!("{} millones", spanish_apocope(&quantity))
            }
        }
        Es | Fr | Pt | Tr if base == 1_000 && q == 1 => label.into(),
        Es => format!("{} {label}", spanish_apocope(&quantity)),
        Fr => {
            let quantity = if base == 1_000 && (q.is_multiple_of(100) || q % 100 == 80) {
                quantity.strip_suffix('s').unwrap_or(&quantity).to_string()
            } else {
                quantity
            };
            format!(
                "{quantity} {label}{}",
                if base > 1_000 && q > 1 { "s" } else { "" }
            )
        }
        Pt if base > 1_000 => format!(
            "{quantity} {}",
            if q == 1 {
                label.to_string()
            } else {
                label.replace("ão", "ões")
            }
        ),
        De if base == 1_000 => format!(
            "{}tausend",
            quantity
                .strip_suffix("eins")
                .map_or_else(|| quantity.clone(), |s| format!("{s}ein"))
        ),
        De => format!(
            "{} {label}{}",
            if q == 1 { "eine".into() } else { quantity },
            if q > 1 {
                if base == 1_000_000 { "en" } else { "n" }
            } else {
                ""
            }
        ),
        Ru => {
            let quantity = if base == 1_000 {
                feminine_ru(&quantity)
            } else {
                quantity
            };
            let noun = match base {
                1_000 => russian_scale(q, "тысяча", "тысячи", "тысяч"),
                1_000_000 => russian_scale(q, "миллион", "миллиона", "миллионов"),
                _ => russian_scale(q, "миллиард", "миллиарда", "миллиардов"),
            };
            format!("{quantity} {noun}")
        }
        _ => format!("{quantity} {label}"),
    };
    if r == 0 {
        return head;
    }
    if locale == De && base == 1_000 {
        return format!("{head}{}", cardinal(r, locale));
    }
    let rest = if locale == Vi && base == 1_000 && r < 100 {
        if r < 10 {
            format!("không trăm lẻ {}", cardinal(r, locale))
        } else {
            format!("không trăm {}", cardinal(r, locale))
        }
    } else {
        cardinal(r, locale)
    };
    let join = if locale == Vi && base >= 1_000_000 && r < 10 {
        " lẻ "
    } else if locale == Pt && (r < 100 || r.is_multiple_of(100)) {
        " e "
    } else {
        " "
    };
    format!("{head}{join}{rest}")
}

fn chinese_under_10000(n: u64, first: bool) -> String {
    let digits = ["零", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
    let mut result = String::new();
    let mut gap = false;
    for (base, unit) in [(1_000, "千"), (100, "百"), (10, "十"), (1, "")] {
        let digit = (n / base) % 10;
        if digit == 0 {
            if !result.is_empty() {
                gap = true;
            }
            continue;
        }
        if gap {
            result.push('零');
            gap = false;
        }
        if !(base == 10 && digit == 1 && result.is_empty() && first) {
            result.push_str(digits[digit as usize]);
        }
        result.push_str(unit);
    }
    result
}
fn chinese(n: u64) -> String {
    if n == 0 {
        return "零".into();
    }
    let mut result = String::new();
    let mut gap = false;
    for (base, unit) in [(100_000_000, "亿"), (10_000, "万"), (1, "")] {
        let group = (n / base) % 10_000;
        if group == 0 {
            if !result.is_empty() {
                gap = true;
            }
            continue;
        }
        if !result.is_empty() && (gap || group < 1_000) {
            result.push('零');
        }
        result.push_str(&chinese_under_10000(group, result.is_empty()));
        result.push_str(unit);
        gap = false;
    }
    result
}
fn east_asian(n: u64, locale: SpeechLocale) -> String {
    let scales = if locale == SpeechLocale::Ja {
        [(100_000_000, "おく"), (10_000, "まん"), (1, "")]
    } else {
        [(100_000_000, "억"), (10_000, "만"), (1, "")]
    };
    let mut groups = Vec::new();
    for (base, unit) in scales {
        let group = (n / base) % 10_000;
        if group != 0 {
            let quantity = if locale == SpeechLocale::Ko && group == 1 && base == 10_000 {
                String::new()
            } else {
                cardinal(group, locale)
            };
            groups.push(format!("{quantity}{unit}"));
        }
    }
    groups.join(if locale == SpeechLocale::Ko { " " } else { "" })
}
