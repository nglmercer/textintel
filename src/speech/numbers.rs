//! Bounded exact-string number verbalization. No inference or floating point.
//!
//! Lexical tables and their provenance live in `data/`. Cardinal readings are
//! standalone nominative forms, not a promise of grammatical agreement with an
//! arbitrary following noun. Callers must handle noun gender/case separately.
use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;

use super::types::{
    NumberNotation, NumberReading, NumberValue, ReadingStyle, SpeechLocale, SpeechOptions,
};

/// Maximum digits accepted in either the integer or fractional component.
/// Longer identifiers need application-specific chunking, not unbounded output.
pub const MAX_SPOKEN_DIGITS: usize = super::core::MAX_NUMBER_DIGITS;

const CJK_DIGITS: [&str; 10] = ["零", "一", "二", "三", "四", "五", "六", "七", "八", "九"];

#[derive(Deserialize)]
struct Lexicon {
    small: Vec<String>,
    hundreds: Vec<String>,
    hundreds_prefix: Vec<String>,
    hundreds_tens_prefix: Vec<String>,
    thousands: Vec<String>,
    thousands_prefix: Vec<String>,
    #[serde(default)]
    basic_ordinals: Vec<String>,
    cardinal_zero: Option<String>,
}

fn lexicon(locale: SpeechLocale) -> &'static Lexicon {
    static DATA: OnceLock<BTreeMap<String, Lexicon>> = OnceLock::new();
    &DATA.get_or_init(|| {
        // Build-owned embedded data is checked exhaustively by resource tests.
        serde_json::from_str(include_str!("data/numbers.json"))
            .expect("valid speech number lexicons")
    })[locale.code()]
}

/// Basic standalone ordinal vocabulary where a language requires irregular
/// lexical forms. Larger ordinals are composed by the expression grammar.
/// An absent entry is explicitly unsupported, never a cardinal substitution.
pub fn basic_ordinal(value: u64, locale: SpeechLocale) -> Option<String> {
    let index = usize::try_from(value).ok()?;
    let word = lexicon(locale).basic_ordinals.get(index)?;
    (!word.is_empty()).then(|| word.clone())
}

/// Highest supported *standalone* cardinal. This does not certify currencies,
/// counters, grammatical case, regional variants, or pronunciation quality.
pub const fn cardinal_max(locale: SpeechLocale) -> u64 {
    match locale {
        SpeechLocale::Ta
        | SpeechLocale::Te
        | SpeechLocale::Hi
        | SpeechLocale::Bn
        | SpeechLocale::Ur
        | SpeechLocale::Pa
        | SpeechLocale::Mr => 999_999_999,
        SpeechLocale::En
        | SpeechLocale::Es
        | SpeechLocale::Fr
        | SpeechLocale::De
        | SpeechLocale::Ru
        | SpeechLocale::Pt
        | SpeechLocale::Zh
        | SpeechLocale::Ja
        | SpeechLocale::Ko => 999_999_999_999_999,
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

fn decimal_digit_words(value: &str, locale: SpeechLocale) -> String {
    let pack = lexicon(locale);
    value
        .bytes()
        .map(|b| {
            if b == b'0' {
                pack.cardinal_zero
                    .as_deref()
                    .unwrap_or(pack.small[0].as_str())
            } else {
                pack.small[(b - b'0') as usize].as_str()
            }
        })
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
        let digits = if reading == NumberReading::Digits {
            digit_words(fraction, locale)
        } else {
            decimal_digit_words(fraction, locale)
        };
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

/// Render a number with an explicit speech style and written-script policy.
/// `Precise` + `Phonetic` is identical to [`verbalize_number`]. `Natural`
/// selects conversational conjunctions and spacing, without changing numeric
/// values or inventing a reading for padded identifiers.
pub fn verbalize_number_with_options(
    value: &NumberValue,
    locale: SpeechLocale,
    reading: NumberReading,
    style: ReadingStyle,
    notation: NumberNotation,
) -> Option<String> {
    let reading = if style == ReadingStyle::Digits {
        NumberReading::Digits
    } else {
        reading
    };
    // Validate both decimal components and magnitude through the common path.
    let standard = verbalize_number(value, locale, reading)?;
    if locale == SpeechLocale::Ja && notation == NumberNotation::Orthographic {
        if style == ReadingStyle::Natural
            && reading == NumberReading::Cardinal
            && value.integer == "0"
            && value.fraction.is_none()
        {
            return Some(if value.negative {
                "マイナスゼロ".into()
            } else {
                "ゼロ".into()
            });
        }
        return japanese_written(value, reading);
    }
    if style != ReadingStyle::Natural {
        return Some(standard);
    }
    let mut spoken = match locale {
        SpeechLocale::Ru | SpeechLocale::Ar
            if reading == NumberReading::Cardinal && value.fraction.is_some() =>
        {
            rational_decimal(value, locale).unwrap_or(standard)
        }
        SpeechLocale::En if reading == NumberReading::Cardinal => {
            let integer = value.integer.parse::<u64>().ok()?;
            let mut natural = english_cardinal(integer, true, false)?;
            if let Some(fraction) = &value.fraction {
                natural.push_str(" point ");
                natural.push_str(&digit_words(fraction, locale));
            }
            if value.negative {
                natural = format!("minus {natural}");
            }
            natural
        }
        SpeechLocale::Fr => standard.replace("-et-", " et "),
        SpeechLocale::Ko => {
            if value.negative {
                let prefix = format!("{} ", sign_word(locale));
                format!(
                    "{} {}",
                    sign_word(locale),
                    standard
                        .strip_prefix(&prefix)
                        .unwrap_or(&standard)
                        .replace(' ', "")
                )
            } else {
                standard.replace(' ', "")
            }
        }
        SpeechLocale::Ar => arabic_modern_spelling(&standard),
        _ => standard,
    };
    // Natural English reads compounds without typographical hyphens.
    if locale == SpeechLocale::En {
        spoken = spoken.replace('-', " ");
    }
    Some(spoken)
}

/// Convenience entry point for consumers already holding speech options.
pub fn verbalize_with_options(
    value: &NumberValue,
    options: &SpeechOptions,
    reading: NumberReading,
) -> Option<String> {
    if options.interpret_as == super::types::SpeechInterpretation::Number
        && options.reading_style != ReadingStyle::Digits
    {
        return verbalize_count_with_options(value, options);
    }
    verbalize_number_with_options(
        value,
        options.locale,
        reading,
        options.reading_style,
        options.number_notation,
    )
}

/// Read an already classified mathematical count or quantity. Integer padding
/// is canonicalized only after numeric semantics have been established;
/// explicit digit style continues to preserve every written zero.
pub fn verbalize_count_with_options(
    value: &NumberValue,
    options: &SpeechOptions,
) -> Option<String> {
    if !valid_digits(&value.integer) || value.fraction.as_ref().is_some_and(|f| !valid_digits(f)) {
        return None;
    }
    let canonical = if options.reading_style == ReadingStyle::Digits {
        value.clone()
    } else {
        super::core::canonical_count(value)
    };
    verbalize_number_with_options(
        &canonical,
        options.locale,
        NumberReading::Cardinal,
        options.reading_style,
        options.number_notation,
    )
}

/// Render a semantic quantity with explicit noun gender. Feminine Spanish,
/// Portuguese, Russian and bounded Arabic integer agreement share the cardinal
/// core. Spanish/Portuguese decimal digit sequences retain every zero; Arabic
/// fractional agreement remains unsupported. Other inflections return
/// unsupported instead of silently claiming agreement.
pub fn verbalize_with_gender(
    value: &NumberValue,
    options: &SpeechOptions,
    gender: Option<&str>,
) -> Option<String> {
    let base = verbalize_count_with_options(value, options)?;
    match gender {
        None | Some("masculine" | "m") => return Some(base),
        Some("feminine" | "f") => {}
        _ => return None,
    }
    if options.reading_style == ReadingStyle::Digits {
        return Some(base);
    }
    if !matches!(
        options.locale,
        SpeechLocale::Es | SpeechLocale::Pt | SpeechLocale::Ru | SpeechLocale::Ar
    ) {
        return None;
    }
    let canonical = super::core::canonical_count(value);
    if options.locale == SpeechLocale::Ar {
        // Fractional-number agreement needs its own grammatical context;
        // integer noun gender must not rewrite digits or rational denominators.
        if canonical.fraction.is_some() {
            return None;
        }
        let words =
            verbalize_number_feminine(&canonical, SpeechLocale::Ar, NumberReading::Cardinal)?;
        return Some(if options.reading_style == ReadingStyle::Natural {
            arabic_modern_spelling(&words)
        } else {
            words
        });
    }
    if options.locale == SpeechLocale::Ru
        && options.reading_style == ReadingStyle::Natural
        && canonical
            .fraction
            .as_ref()
            .is_some_and(|fraction| fraction.len() <= 6)
    {
        return Some(base);
    }
    let mut integer = canonical.clone();
    integer.fraction = None;
    let mut spoken = verbalize_number_feminine(&integer, options.locale, NumberReading::Cardinal)?;
    if let Some(fraction) = &canonical.fraction {
        let mut words = Vec::with_capacity(fraction.len());
        for byte in fraction.bytes() {
            let digit = NumberValue {
                negative: false,
                integer: char::from(byte).to_string(),
                fraction: None,
            };
            words.push(verbalize_number_feminine(
                &digit,
                options.locale,
                NumberReading::Cardinal,
            )?);
        }
        spoken.push(' ');
        spoken.push_str(point_word(options.locale));
        spoken.push(' ');
        spoken.push_str(&words.join(" "));
    }
    Some(spoken)
}

/// Shared English cardinal composition with presentation-only adapters.
/// British conjunctions and typographical hyphens do not alter numeric
/// grouping; both front ends consume this same bounded integer core.
pub(crate) fn english_cardinal(n: u64, british: bool, hyphenated: bool) -> Option<String> {
    if n > cardinal_max(SpeechLocale::En) {
        return None;
    }
    let raw = cardinal(n, SpeechLocale::En);
    let mut words = if british {
        let input = raw.split_whitespace().collect::<Vec<_>>();
        let mut output = Vec::with_capacity(input.len() + 5);
        for (index, &word) in input.iter().enumerate() {
            output.push(word);
            if word == "hundred"
                && input.get(index + 1).is_some_and(|next| {
                    !matches!(*next, "thousand" | "million" | "billion" | "trillion")
                })
            {
                output.push("and");
            }
        }
        let joined = output.join(" ");
        let tail = n % 1000;
        if n >= 1000 && (1..100).contains(&tail) {
            let remainder = cardinal(tail, SpeechLocale::En);
            let suffix = format!(" {remainder}");
            let head = joined.strip_suffix(&suffix)?;
            format!("{head} and {remainder}")
        } else {
            joined
        }
    } else {
        raw
    };
    if !hyphenated {
        words = words.replace('-', " ");
    }
    Some(words)
}

/// Shared lexical digit name for front ends that already validated a decimal
/// digit. Integer composition and digit spellings use one embedded lexicon.
pub(crate) fn digit_word(digit: u8, locale: SpeechLocale) -> Option<&'static str> {
    if digit > 9 {
        return None;
    }
    Some(lexicon(locale).small[digit as usize].as_str())
}

/// Locale-specific presentation adapters share integer decomposition, scales
/// and vocabulary with the ordinary typed number renderer.
pub(crate) fn french_cardinal(n: u64, feminine: bool, traditional: bool) -> Option<String> {
    if n > cardinal_max(SpeechLocale::Fr) {
        return None;
    }
    let mut words = cardinal(n, SpeechLocale::Fr);
    if feminine && let Some(prefix) = words.strip_suffix("un") {
        words = format!("{prefix}une");
    }
    if traditional {
        words = words.replace("-et-", " et ");
    }
    Some(words)
}
pub(crate) fn german_cardinal(n: u64, final_one: &str, explicit_one: bool) -> Option<String> {
    if n > cardinal_max(SpeechLocale::De) || !matches!(final_one, "eins" | "ein" | "eine") {
        return None;
    }
    let mut words = cardinal(n, SpeechLocale::De);
    if !explicit_one {
        words = words.replace("einhundert", "hundert");
        // Omit one only for a thousand section equal to one. A suffix in
        // zweihunderteintausend is the essential one in 201,000.
        if (n / 1000) % 1000 == 1 {
            words = words.replace("eintausend", "tausend");
        }
    }
    if final_one != "eins"
        && let Some(prefix) = words.strip_suffix("eins")
    {
        words = format!("{prefix}{final_one}");
    }
    Some(words)
}
pub(crate) fn russian_cardinal(n: u64, feminine: bool, omit_initial_one: bool) -> Option<String> {
    if n > cardinal_max(SpeechLocale::Ru) {
        return None;
    }
    let mut words = if feminine {
        feminine_ru(&cardinal(n, SpeechLocale::Ru))
    } else {
        cardinal(n, SpeechLocale::Ru)
    };
    if omit_initial_one
        && let Some((one, rest)) = words.split_once(' ')
        && matches!(one, "один" | "одна")
        && rest
            .split_whitespace()
            .next()
            .is_some_and(|scale| matches!(scale, "тысяча" | "миллион" | "миллиард" | "триллион"))
    {
        words = rest.into();
    }
    Some(words)
}
pub(crate) fn chinese_cardinal(n: u64, traditional: bool) -> Option<String> {
    if n > cardinal_max(SpeechLocale::Zh) {
        return None;
    }
    let words = cardinal(n, SpeechLocale::Zh);
    Some(if traditional {
        words.replace('万', "萬").replace('亿', "億")
    } else {
        words
    })
}
pub(crate) fn cjk_digit_word(digit: u8) -> Option<&'static str> {
    CJK_DIGITS.get(digit as usize).copied()
}
pub(crate) fn decimal_digit_word(digit: u8, locale: SpeechLocale) -> Option<&'static str> {
    if digit > 9 {
        return None;
    }
    let pack = lexicon(locale);
    Some(if digit == 0 {
        pack.cardinal_zero
            .as_deref()
            .unwrap_or(pack.small[0].as_str())
    } else {
        pack.small[digit as usize].as_str()
    })
}

pub(crate) fn portuguese_cardinal(n: u64, feminine: bool, brazilian: bool) -> Option<String> {
    if n > cardinal_max(SpeechLocale::Pt) {
        return None;
    }
    Some(portuguese_compose(n, feminine, brazilian))
}
fn portuguese_small(n: u64, feminine: bool, brazilian: bool) -> String {
    debug_assert!(n < 1000);
    let words = cardinal(n, SpeechLocale::Pt);
    let words = if feminine {
        feminine_small(words, SpeechLocale::Pt).expect("Portuguese gender grammar")
    } else {
        words
    };
    if brazilian {
        words
    } else {
        words
            .split_whitespace()
            .map(|word| match word {
                "dezesseis" => "dezasseis",
                "dezessete" => "dezassete",
                "dezenove" => "dezanove",
                other => other,
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}
fn portuguese_low_components(
    n: u64,
    feminine: bool,
    brazilian: bool,
    parts: &mut Vec<(u64, String)>,
) {
    let thousands = n / 1000;
    let units = n % 1000;
    if thousands == 1 {
        parts.push((1, "mil".into()));
    } else if thousands > 1 {
        parts.push((
            thousands,
            format!("{} mil", portuguese_small(thousands, feminine, brazilian)),
        ));
    }
    if units > 0 {
        parts.push((units, portuguese_small(units, feminine, brazilian)));
    }
}
fn portuguese_join(parts: &[(u64, String)]) -> String {
    let mut out = String::new();
    for (index, (value, words)) in parts.iter().enumerate() {
        if index > 0 {
            out.push_str(
                if index + 1 == parts.len() && (*value < 100 || value.is_multiple_of(100)) {
                    " e "
                } else {
                    " "
                },
            );
        }
        out.push_str(words);
    }
    out
}
fn portuguese_scale(value: u64, one: &str, many: &str, brazilian: bool) -> String {
    format!(
        "{} {}",
        portuguese_small(value, false, brazilian),
        if value == 1 { one } else { many }
    )
}
fn portuguese_compose(n: u64, feminine: bool, brazilian: bool) -> String {
    if n < 1000 {
        return portuguese_small(n, feminine, brazilian);
    }
    let mut parts = Vec::new();
    if brazilian {
        for (size, one, many) in [
            (1_000_000_000_000, "trilhão", "trilhões"),
            (1_000_000_000, "bilhão", "bilhões"),
            (1_000_000, "milhão", "milhões"),
        ] {
            let value = (n / size) % 1000;
            if value > 0 {
                parts.push((value, portuguese_scale(value, one, many, true)));
            }
        }
    } else {
        let billions = n / 1_000_000_000_000;
        if billions > 0 {
            parts.push((
                billions,
                portuguese_scale(billions, "bilião", "biliões", false),
            ));
        }
        let millions = (n / 1_000_000) % 1_000_000;
        if millions == 1 {
            parts.push((1, "um milhão".into()));
        } else if millions > 1 {
            let mut coefficients = Vec::new();
            portuguese_low_components(millions, false, false, &mut coefficients);
            parts.push((
                millions,
                format!("{} milhões", portuguese_join(&coefficients)),
            ));
        }
    }
    portuguese_low_components(n % 1_000_000, feminine, brazilian, &mut parts);
    portuguese_join(&parts)
}

/// Spoken decimal fractions use their denominator to retain decimal precision.
/// Longer fractions retain the ordinary digit-by-digit reading instead.
fn rational_decimal(value: &NumberValue, locale: SpeechLocale) -> Option<String> {
    let fraction = value.fraction.as_ref()?;
    let numerator = fraction.parse::<u64>().ok()?;
    let integer = value.integer.parse::<u64>().ok()?;
    let body = match locale {
        SpeechLocale::Ru if fraction.len() <= 6 => {
            let denominators = [
                ("десятая", "десятых"),
                ("сотая", "сотых"),
                ("тысячная", "тысячных"),
                ("десятитысячная", "десятитысячных"),
                ("стотысячная", "стотысячных"),
                ("миллионная", "миллионных"),
            ];
            let singular = |n: u64| n % 10 == 1 && n % 100 != 11;
            let (one, many) = denominators[fraction.len() - 1];
            let whole = feminine_ru(&cardinal(integer, locale));
            let part = feminine_ru(&cardinal(numerator, locale));
            format!(
                "{whole} {} {part} {}",
                if singular(integer) {
                    "целая"
                } else {
                    "целых"
                },
                if singular(numerator) { one } else { many }
            )
        }
        SpeechLocale::Ar if fraction.len() <= 4 => {
            let denominator = ["عشرة", "مئة", "ألف", "عشرة آلاف"][fraction.len() - 1];
            format!(
                "{} و{} من {denominator}",
                arabic_modern_spelling(&cardinal(integer, locale)),
                arabic_modern_spelling(&cardinal(numerator, locale))
            )
        }
        _ => return None,
    };
    Some(if value.negative {
        format!("{} {body}", sign_word(locale))
    } else {
        body
    })
}

fn arabic_modern_spelling(words: &str) -> String {
    let mut result = words.to_string();
    for (traditional, modern) in [
        ("ثلاثمائة", "ثلاث مئة"),
        ("أربعمائة", "أربع مئة"),
        ("خمسمائة", "خمس مئة"),
        ("ستمائة", "ست مئة"),
        ("سبعمائة", "سبع مئة"),
        ("ثمانمائة", "ثمان مئة"),
        ("تسعمائة", "تسع مئة"),
        ("مائتان", "مئتان"),
        ("مائتا", "مئتا"),
        ("مائة", "مئة"),
    ] {
        result = result.replace(traditional, modern);
    }
    result
}

fn japanese_written(value: &NumberValue, reading: NumberReading) -> Option<String> {
    let digits = |s: &str| {
        s.bytes()
            .map(|b| CJK_DIGITS[(b - b'0') as usize])
            .collect::<String>()
    };
    let mut words = if reading == NumberReading::Digits {
        digits(&value.integer)
    } else {
        japanese_kanji_cardinal(value.integer.parse().ok()?)
    };
    if let Some(fraction) = &value.fraction {
        words.push('点');
        words.push_str(&digits(fraction));
    }
    if value.negative {
        words = format!("マイナス{words}");
    }
    Some(words)
}

/// The Japanese written numeral system shares the base-10,000 grouping used
/// by the kana renderer; no phonetic-to-kanji substring replacement is used.
fn japanese_kanji_cardinal(n: u64) -> String {
    if n == 0 {
        return "零".into();
    }
    let mut words = String::new();
    for (base, scale) in [
        (1_000_000_000_000, "兆"),
        (100_000_000, "億"),
        (10_000, "万"),
        (1, ""),
    ] {
        let group = (n / base) % 10_000;
        if group == 0 {
            continue;
        }
        for (place, unit) in [(1000, "千"), (100, "百"), (10, "十"), (1, "")] {
            let digit = (group / place) % 10;
            if digit == 0 {
                continue;
            }
            if digit != 1 || place == 1 || (place == 1000 && group == 1000 && base > 1) {
                words.push_str(CJK_DIGITS[digit as usize]);
            }
            words.push_str(unit);
        }
        words.push_str(scale);
    }
    words
}

/// Feminine nominative forms for es/pt/ru/ar integers within their cardinal
/// bounds. Masculine scale nouns retain their own agreement; only counts
/// governed by the final noun change. Fractional and other inflections need
/// an explicit grammatical context and remain unsupported here.
/// Arabic final eight assumes a directly governed counted noun (`ثماني`),
/// while a conjoined nominative eight is `ثمان`. Compound counts ending in
/// one/two above ninety-nine need noun ordering and remain unsupported;
/// the same restriction applies to internal hundred-valued scale coefficients.
/// construct-state scale endings remain the quantity renderer's responsibility.
pub fn verbalize_number_feminine(
    value: &NumberValue,
    locale: SpeechLocale,
    reading: NumberReading,
) -> Option<String> {
    if reading == NumberReading::Digits {
        return verbalize_number(value, locale, reading);
    }
    if !matches!(
        locale,
        SpeechLocale::Es | SpeechLocale::Pt | SpeechLocale::Ru | SpeechLocale::Ar
    ) || value.fraction.is_some()
    {
        return None;
    }
    verbalize_number(value, locale, reading)?;
    let n = value.integer.parse::<u64>().ok()?;
    let words = feminine_cardinal(n, locale)?;
    Some(if value.negative {
        format!("{} {words}", sign_word(locale))
    } else {
        words
    })
}

fn feminine_cardinal(n: u64, locale: SpeechLocale) -> Option<String> {
    if locale == SpeechLocale::Ar {
        return arabic_feminine_cardinal(n);
    }
    if locale == SpeechLocale::Ru {
        return Some(feminine_ru(&cardinal(n, locale)));
    }
    if locale == SpeechLocale::Pt {
        return Some(portuguese_compose(n, true, true));
    }
    if n < 1000 {
        return feminine_small(cardinal(n, locale), locale);
    }
    let base = if n >= 1_000_000 { 1_000_000 } else { 1000 };
    let q = n / base;
    let r = n % base;
    let head = if base >= 1_000_000 {
        cardinal(n - r, locale)
    } else if q == 1 {
        "mil".into()
    } else {
        format!("{} mil", feminine_small(cardinal(q, locale), locale)?)
    };
    if r == 0 {
        return Some(head);
    }
    Some(format!("{head} {}", feminine_cardinal(r, locale)?))
}

/// Nominative feminine count morphology, reusing the same masculine lexical
/// data and scale composition. Rules: Al-Mustaqbal University, pp. 1–3;
/// AIOU Quranic Arabic (1952), number agreement, confirms إحدى for conjoined one.
/// This intentionally does not copy CLDR48's incorrect feminine 3/11 rules.
/// https://uomus.edu.iq/img/lectures21/MUCLecture_2025_6129126.pdf
/// https://online.aiou.edu.pk/LIVE_SITE/SoftBooks/1952.pdf
fn arabic_feminine_cardinal(n: u64) -> Option<String> {
    // In a conjoined higher compound, one/two keep their ordinary feminine
    // forms (مئة وواحدة / مئة واثنتان). Coefficients of thousand/million
    // scales are composed by the masculine scale grammar in `cardinal`, so
    // they do not inherit the counted noun's gender.
    if n >= 100 {
        // Hundreds and masculine thousand/million/billion groups keep their
        // existing agreement. Only the remainder counts the feminine noun.
        let base = if n >= 1000 { 1000 } else { 100 };
        let remainder = n % base;
        let head = cardinal(n - remainder, SpeechLocale::Ar);
        return if remainder == 0 {
            Some(head)
        } else {
            Some(format!("{head} و{}", arabic_feminine_cardinal(remainder)?))
        };
    }
    match n {
        0 => Some(cardinal(0, SpeechLocale::Ar)),
        1 => Some(format!("{}ة", cardinal(1, SpeechLocale::Ar))),
        2 => Some("اثنتان".into()),
        3..=10 => lexicon(SpeechLocale::Ar).small[n as usize]
            .strip_suffix('ة')
            .map(str::to_string),
        11 => Some(format!("إحدى {}", cardinal(10, SpeechLocale::Ar))),
        12 => Some(format!("اثنتا {}", cardinal(10, SpeechLocale::Ar))),
        13..=19 => Some(format!(
            "{} {}",
            arabic_feminine_cardinal(n - 10)?,
            cardinal(10, SpeechLocale::Ar)
        )),
        _ if n.is_multiple_of(10) => Some(cardinal(n, SpeechLocale::Ar)),
        _ => {
            let unit = n % 10;
            let first = match unit {
                1 => "إحدى".into(),
                // Outside إضافة, nominative ثمانٍ drops its final ي.
                8 => arabic_feminine_cardinal(8)?.strip_suffix('ي')?.to_string(),
                _ => arabic_feminine_cardinal(unit)?,
            };
            Some(format!("{first} و{}", cardinal(n - unit, SpeechLocale::Ar)))
        }
    }
}

fn feminine_small(mut words: String, locale: SpeechLocale) -> Option<String> {
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
    if n == 0
        && let Some(zero) = &lexicon(locale).cardinal_zero
    {
        return zero.clone();
    }
    if locale == SpeechLocale::Zh {
        return chinese(n);
    }
    if locale == SpeechLocale::Pt && n >= 1000 {
        return portuguese_compose(n, false, true);
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
pub(super) fn spanish_apocope(words: &str) -> String {
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
    if locale == Ar {
        return arabic_large(n);
    }
    if locale == Ta {
        return tamil_large(n);
    }
    if locale == Te {
        return telugu_large(n);
    }
    let scales: &[(u64, &str)] = match locale {
        Hi => &[(10_000_000, "करोड़"), (100_000, "लाख"), (1_000, "हज़ार")],
        Bn => &[(10_000_000, "কোটি"), (100_000, "লাখ"), (1_000, "হাজার")],
        Ur => &[(10_000_000, "کروڑ"), (100_000, "لاکھ"), (1_000, "ہزار")],
        Pa => &[(10_000_000, "ਕਰੋੜ"), (100_000, "ਲੱਖ"), (1_000, "ਹਜ਼ਾਰ")],
        Mr => &[(10_000_000, "कोटी"), (100_000, "लाख"), (1_000, "हजार")],
        Es => &[
            (1_000_000_000_000, "billón"),
            (1_000_000, "millón"),
            (1_000, "mil"),
        ],
        En => &[
            (1_000_000_000_000, "trillion"),
            (1_000_000_000, "billion"),
            (1_000_000, "million"),
            (1_000, "thousand"),
        ],
        Fr => &[
            (1_000_000_000_000, "billion"),
            (1_000_000_000, "milliard"),
            (1_000_000, "million"),
            (1_000, "mille"),
        ],
        De => &[
            (1_000_000_000_000, "Billion"),
            (1_000_000_000, "Milliarde"),
            (1_000_000, "Million"),
            (1_000, "tausend"),
        ],
        Ru => &[
            (1_000_000_000_000, "триллион"),
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
        Es if base >= 1_000_000 => {
            if q == 1 {
                format!("un {label}")
            } else {
                format!(
                    "{} {}",
                    spanish_apocope(&quantity),
                    if base == 1_000_000_000_000 {
                        "billones"
                    } else {
                        "millones"
                    }
                )
            }
        }
        Es | Fr | Tr if base == 1_000 && q == 1 => label.into(),
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
                if label.ends_with('e') { "n" } else { "en" }
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
                1_000_000_000 => russian_scale(q, "миллиард", "миллиарда", "миллиардов"),
                _ => russian_scale(q, "триллион", "триллиона", "триллионов"),
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
    } else {
        " "
    };
    format!("{head}{join}{rest}")
}

/// Nominal scale names; grammatical case before an arbitrary noun remains
/// the caller's responsibility. Singular/dual/plural selection is explicit.
fn arabic_large(n: u64) -> String {
    let (base, one, two, few) = if n >= 1_000_000_000 {
        (1_000_000_000, "مليار", "ملياران", "مليارات")
    } else if n >= 1_000_000 {
        (1_000_000, "مليون", "مليونان", "ملايين")
    } else {
        (1000, "ألف", "ألفان", "آلاف")
    };
    let q = n / base;
    let r = n % base;
    let head = match q {
        1 => one.into(),
        2 => two.into(),
        3..=10 => format!("{} {few}", cardinal(q, SpeechLocale::Ar)),
        _ => {
            // The dual hundred is إضافة before the scale just as before a
            // currency: مائتا ألف, not standalone مائتان ألف.
            let coefficient = arabic_construct_count(cardinal(q, SpeechLocale::Ar));
            format!("{coefficient} {one}")
        }
    };
    if r == 0 {
        head
    } else {
        format!("{head} و{}", cardinal(r, SpeechLocale::Ar))
    }
}

/// Apply the nominative construct ending only to a terminal reviewed dual
/// hundred/scale immediately governing a noun. Earlier duals before a
/// conjunction retain their ن; standalone cardinals never call this rule.
/// Al-Mustaqbal (2025), p. 3: حذف النون للإضافة.
pub(super) fn arabic_construct_count(mut words: String) -> String {
    if ["مائتان", "مئتان", "ألفان", "مليونان", "ملياران"]
        .iter()
        .any(|dual| words.ends_with(dual))
    {
        words.pop();
    }
    words
}

/// Indian lakh/crore grouping with explicit linking scale forms. Counts and
/// scales are separated rather than guessing optional Tamil sandhi variants.
fn tamil_large(n: u64) -> String {
    let (base, unit, link) = if n >= 10_000_000 {
        (10_000_000, "கோடி", "கோடியே")
    } else if n >= 100_000 {
        (100_000, "லட்சம்", "லட்சத்து")
    } else {
        (1000, "ஆயிரம்", "ஆயிரத்து")
    };
    let q = n / base;
    let r = n % base;
    let count = if q == 1 {
        "ஒரு".into()
    } else {
        cardinal(q, SpeechLocale::Ta)
    };
    let head = format!("{count} {}", if r == 0 { unit } else { link });
    if r == 0 {
        head
    } else {
        format!("{head} {}", cardinal(r, SpeechLocale::Ta))
    }
}

/// CLDR49 Telugu scale grammar: one lakh/crore and plural lakh/crore counts.
/// Existing small-number and thousand inflections remain shared with the data.
fn telugu_large(n: u64) -> String {
    let (base, one, many) = if n >= 10_000_000 {
        (10_000_000, "ఒక కోటి", "కోట్ల")
    } else if n >= 100_000 {
        (100_000, "ఒక లక్ష", "లక్షల")
    } else {
        (1000, "వెయ్యి", "వేల")
    };
    let q = n / base;
    let r = n % base;
    let head = if q == 1 {
        one.into()
    } else {
        format!("{} {many}", cardinal(q, SpeechLocale::Te))
    };
    if r == 0 {
        head
    } else {
        format!("{head} {}", cardinal(r, SpeechLocale::Te))
    }
}

fn chinese_under_10000(n: u64, first: bool) -> String {
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
            result.push_str(CJK_DIGITS[digit as usize]);
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
    for (base, unit) in [
        (1_000_000_000_000, "万亿"),
        (100_000_000, "亿"),
        (10_000, "万"),
        (1, ""),
    ] {
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
        [
            (1_000_000_000_000, "ちょう"),
            (100_000_000, "おく"),
            (10_000, "まん"),
            (1, ""),
        ]
    } else {
        [
            (1_000_000_000_000, "조"),
            (100_000_000, "억"),
            (10_000, "만"),
            (1, ""),
        ]
    };
    let mut groups = Vec::new();
    for (base, unit) in scales {
        let group = (n / base) % 10_000;
        if group != 0 {
            let mut quantity = if locale == SpeechLocale::Ko && group == 1 && base == 10_000 {
                String::new()
            } else {
                cardinal(group, locale)
            };
            if locale == SpeechLocale::Ja && base == 1_000_000_000_000 {
                for (plain, contracted) in
                    [("いち", "いっ"), ("はち", "はっ"), ("じゅう", "じゅっ")]
                {
                    if let Some(prefix) = quantity.strip_suffix(plain) {
                        quantity = format!("{prefix}{contracted}");
                        break;
                    }
                }
            }
            groups.push(format!("{quantity}{unit}"));
        }
    }
    groups.join(if locale == SpeechLocale::Ko { " " } else { "" })
}
