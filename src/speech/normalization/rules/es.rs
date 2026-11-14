//! Spanish (`es`; locales es-ES, es-MX, es-PE) rules.
//!
//! Number format for every Spanish locale: decimal comma, dot grouping
//! (`1.234,56`). Bare `1.234`, `1,234` and dot decimals such as `3.14` are
//! ambiguous and preserved by the classifier. Note: everyday es-MX writing
//! often uses a decimal point; those forms are preserved with a warning
//! rather than read with the wrong convention.
//!
//! Grammar handled: apocope (`un sol`, `veintiún soles`, `primer piso`),
//! feminine agreement for feminine currency names (`una libra`,
//! `doscientas libras`), `de` after exact millions (`un millón de soles`).
//! A bare integer before an unknown noun whose reading depends on gender
//! (`21 casas`, `200 personas`) is preserved as ambiguous.

use super::{
    AbbrevContext, Amount, Conventions, Currency, Feature, LanguageRules, Meridiem, OrdinalMarker,
    Unit,
};
use crate::speech::normalization::{CoverageLevel, ReadingStyle};
use crate::speech::{
    numbers,
    types::{NumberReading, NumberValue, SpeechLocale},
};

/// Spanish rule set.
pub(crate) struct Spanish;

/// Grammatical form of the final "one" in a cardinal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Form {
    /// Standalone masculine: `uno`, `veintiuno`.
    Masculine,
    /// Before a masculine noun: `un`, `veintiún`.
    Apocope,
    /// Before a feminine noun: `una`, `veintiuna`, `doscientas`.
    Feminine,
}

/// Function words after which a number keeps its standalone reading.
const STANDALONE_FOLLOWERS: &[&str] = &[
    "a", "al", "como", "con", "cuando", "de", "del", "desde", "durante", "e", "el", "en", "entre",
    "era", "es", "eran", "fue", "fueron", "ha", "han", "hasta", "la", "las", "le", "les", "lo",
    "los", "más", "menos", "mientras", "ni", "o", "para", "pero", "por", "que", "se", "según",
    "sin", "sobre", "son", "su", "sus", "tras", "u", "y",
];

/// Exact Spanish cardinal; grammar selects a presentation form over the
/// shared integer composition and lexicon rather than recomposing numbers.
pub(crate) fn cardinal(n: u64, form: Form) -> String {
    let value = NumberValue {
        negative: false,
        integer: n.to_string(),
        fraction: None,
    };
    let words = match form {
        Form::Feminine => {
            numbers::verbalize_number_feminine(&value, SpeechLocale::Es, NumberReading::Cardinal)
        }
        Form::Masculine | Form::Apocope => {
            numbers::verbalize_number(&value, SpeechLocale::Es, NumberReading::Cardinal)
        }
    }
    .expect("normalization cardinal range is checked before rendering");
    if form == Form::Apocope {
        numbers::spanish_apocope(&words)
    } else {
        words
    }
}

/// `true` when a noun after `n` must be introduced by `de`
/// (`un millón de soles`).
fn needs_de(n: u64) -> bool {
    n >= 1_000_000 && n.is_multiple_of(1_000_000)
}

fn digit_word(d: u8) -> &'static str {
    numbers::digit_word(d - b'0', SpeechLocale::Es).expect("validated decimal digit")
}

/// Fractional part: one digit or two digits without a leading zero are read
/// as a number (`coma cincuenta y seis`); anything else digit by digit
/// (`coma cero cinco`, `coma uno cuatro cero`).
fn fraction_words(frac: &str) -> String {
    let bytes = frac.as_bytes();
    if bytes.len() == 1 || (bytes.len() == 2 && bytes[0] != b'0') {
        let value = frac.parse::<u64>().unwrap_or(0);
        return cardinal(value, Form::Masculine);
    }
    bytes
        .iter()
        .map(|d| digit_word(*d))
        .collect::<Vec<_>>()
        .join(" ")
}

fn amount_words(amount: &Amount, form: Form) -> String {
    let mut out = String::new();
    if amount.negative {
        out.push_str("menos ");
    }
    match &amount.frac {
        None => out.push_str(&cardinal(amount.int, form)),
        Some(frac) => {
            out.push_str(&cardinal(amount.int, Form::Masculine));
            out.push_str(" coma ");
            out.push_str(&fraction_words(frac));
        }
    }
    out
}

/// Amount followed by a masculine noun with number agreement.
fn quantity(amount: &Amount, singular: &str, plural: &str) -> String {
    let mut out = amount_words(amount, Form::Apocope);
    if amount.frac.is_none() && needs_de(amount.int) {
        out.push_str(" de");
    }
    out.push(' ');
    out.push_str(if amount.is_one() { singular } else { plural });
    out
}

fn gender_sensitive_group(group: u64) -> bool {
    (group % 10 == 1 && group % 100 != 11) || group / 100 >= 2
}

/// Masculine ordinal stems 1..=9.
const ORD_UNITS: [&str; 10] = [
    "", "primero", "segundo", "tercero", "cuarto", "quinto", "sexto", "séptimo", "octavo", "noveno",
];
/// Ordinals 10..=19.
const ORD_TEENS: [&str; 10] = [
    "décimo",
    "undécimo",
    "duodécimo",
    "decimotercero",
    "decimocuarto",
    "decimoquinto",
    "decimosexto",
    "decimoséptimo",
    "decimoctavo",
    "decimonoveno",
];
/// Ordinal tens 20..=90.
const ORD_TENS: [&str; 10] = [
    "",
    "",
    "vigésimo",
    "trigésimo",
    "cuadragésimo",
    "quincuagésimo",
    "sexagésimo",
    "septuagésimo",
    "octogésimo",
    "nonagésimo",
];

/// Masculine ordinal for 1..=100.
fn ordinal_masculine(n: u64) -> Option<String> {
    Some(match n {
        1..=9 => ORD_UNITS[n as usize].to_string(),
        10..=19 => ORD_TEENS[(n - 10) as usize].to_string(),
        20..=99 => {
            let tens = ORD_TENS[(n / 10) as usize];
            match n % 10 {
                0 => tens.to_string(),
                unit => format!("{tens} {}", ORD_UNITS[unit as usize]),
            }
        }
        100 => "centésimo".to_string(),
        _ => return None,
    })
}

fn feminize(masculine: &str) -> String {
    masculine
        .split(' ')
        .map(|word| match word.strip_suffix('o') {
            Some(stem) => format!("{stem}a"),
            None => word.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_standalone_follower(word: &str) -> bool {
    let lower = word.to_lowercase();
    STANDALONE_FOLLOWERS.contains(&lower.as_str())
}

/// `true` when `next_word` is a word that may be a noun.
fn noun_like(next_word: &str) -> bool {
    next_word.chars().next().is_some_and(char::is_alphabetic) && !is_standalone_follower(next_word)
}

/// A common noun after a number is strong evidence of a count. Proper nouns
/// start uppercase, which keeps identifiers and names (`007 Bond`,
/// `28002 Madrid`) out of the quantity context.
fn count_noun(next_word: &str) -> bool {
    next_word.chars().next().is_some_and(|c| c.is_lowercase())
        && next_word.chars().all(char::is_alphabetic)
        && !is_standalone_follower(next_word)
}

fn currency_names(
    currency: Currency,
) -> (&'static str, &'static str, Form, &'static str, &'static str) {
    // (singular, plural, number form, subunit singular, subunit plural)
    match currency {
        Currency::Pen => ("sol", "soles", Form::Apocope, "céntimo", "céntimos"),
        Currency::Eur => ("euro", "euros", Form::Apocope, "céntimo", "céntimos"),
        Currency::Usd => ("dólar", "dólares", Form::Apocope, "centavo", "centavos"),
        Currency::Gbp => (
            "libra esterlina",
            "libras esterlinas",
            Form::Feminine,
            "penique",
            "peniques",
        ),
        Currency::Brl => ("real", "reales", Form::Apocope, "centavo", "centavos"),
        Currency::Jpy => ("yen", "yenes", Form::Apocope, "", ""),
    }
}

/// Conversational Spanish clock reading. Hours keep the written 24-hour
/// value; exact midnight/noon and quarter/half hours get their idiomatic
/// names. AM/PM forms stay unsupported (preserved with a warning).
fn natural_time(hour: u32, minute: u32, meridiem: Option<Meridiem>) -> Option<String> {
    if meridiem.is_some() || hour > 23 || minute > 59 {
        return None;
    }
    if hour == 0 && minute == 0 {
        return Some("medianoche".to_string());
    }
    if hour == 12 && minute == 0 {
        return Some("mediodía".to_string());
    }
    let hour_words = cardinal(u64::from(hour), Form::Feminine);
    Some(match minute {
        0 => format!("{hour_words} en punto"),
        15 => format!("{hour_words} y cuarto"),
        30 => format!("{hour_words} y media"),
        _ => format!(
            "{hour_words} y {}",
            cardinal(u64::from(minute), Form::Masculine)
        ),
    })
}

fn fraction_names(denominator: u64) -> Option<(&'static str, &'static str)> {
    Some(match denominator {
        2 => ("medio", "medios"),
        3 => ("tercio", "tercios"),
        4 => ("cuarto", "cuartos"),
        5 => ("quinto", "quintos"),
        6 => ("sexto", "sextos"),
        7 => ("séptimo", "séptimos"),
        8 => ("octavo", "octavos"),
        9 => ("noveno", "novenos"),
        10 => ("décimo", "décimos"),
        100 => ("centésimo", "centésimos"),
        1000 => ("milésimo", "milésimos"),
        _ => return None,
    })
}

fn duration_part(value: u64, form: Form, singular: &str, plural: &str) -> String {
    format!(
        "{} {}",
        cardinal(value, form),
        if value == 1 { singular } else { plural }
    )
}

fn unit_names(unit: Unit) -> (&'static str, &'static str) {
    match unit {
        Unit::Kilogram => ("kilogramo", "kilogramos"),
        Unit::Gram => ("gramo", "gramos"),
        Unit::Kilometer => ("kilómetro", "kilómetros"),
        Unit::Meter => ("metro", "metros"),
        Unit::Centimeter => ("centímetro", "centímetros"),
        Unit::Millimeter => ("milímetro", "milímetros"),
        Unit::Liter => ("litro", "litros"),
        Unit::Milliliter => ("mililitro", "mililitros"),
        Unit::Celsius => ("grado Celsius", "grados Celsius"),
        Unit::Fahrenheit => ("grado Fahrenheit", "grados Fahrenheit"),
        Unit::KilometersPerHour => ("kilómetro por hora", "kilómetros por hora"),
        Unit::MetersPerSecond => ("metro por segundo", "metros por segundo"),
        Unit::Gigabyte => ("gigabyte", "gigabytes"),
        Unit::Megahertz => ("megahercio", "megahercios"),
        Unit::Millisecond => ("milisegundo", "milisegundos"),
    }
}

impl LanguageRules for Spanish {
    fn coverage(&self, feature: Feature) -> CoverageLevel {
        match feature {
            Feature::CardinalNumbers | Feature::Percentages => CoverageLevel::Full,
            Feature::Decimals
            | Feature::Ordinals
            | Feature::Currency
            | Feature::Dates
            | Feature::Times
            | Feature::Units
            | Feature::Abbreviations
            | Feature::Ranges => CoverageLevel::Partial,
            Feature::Fractions => CoverageLevel::Partial,
        }
    }

    fn conventions(&self, locale: &str) -> Option<Conventions> {
        Conventions::for_locale("es", locale)
    }

    fn number(&self, amount: &Amount, _locale: &str) -> Option<String> {
        Some(amount_words(amount, Form::Masculine))
    }

    fn digits(&self, digits: &str, _locale: &str) -> Option<String> {
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some(digits.bytes().map(digit_word).collect::<Vec<_>>().join(" "))
    }

    fn number_gender_ambiguous(&self, int: u64, next_word: &str) -> bool {
        if !noun_like(next_word) {
            return false;
        }
        let units = int % 1000;
        let thousands = (int / 1000) % 1000;
        (units > 0 && gender_sensitive_group(units))
            || (thousands > 0 && gender_sensitive_group(thousands))
    }

    fn quantity_context(&self, next_word: &str) -> bool {
        count_noun(next_word) || super::unit_from_str(next_word).is_some()
    }

    fn ordinal(
        &self,
        n: u64,
        marker: OrdinalMarker<'_>,
        next_word: Option<&str>,
        _locale: &str,
    ) -> Option<String> {
        let masculine = ordinal_masculine(n)?;
        match marker {
            OrdinalMarker::Feminine => Some(feminize(&masculine)),
            OrdinalMarker::Masculine => {
                let before_noun = next_word.is_some_and(noun_like);
                if before_noun {
                    if let Some(stem) = masculine.strip_suffix("primero") {
                        return Some(format!("{stem}primer"));
                    }
                    if let Some(stem) = masculine.strip_suffix("tercero") {
                        return Some(format!("{stem}tercer"));
                    }
                }
                Some(masculine)
            }
            OrdinalMarker::Suffix(_) => None,
        }
    }

    fn percent(&self, amount: &Amount, _locale: &str) -> Option<String> {
        Some(format!(
            "{} por ciento",
            amount_words(amount, Form::Masculine)
        ))
    }

    fn currency(
        &self,
        currency: Currency,
        int: u64,
        cents: Option<u32>,
        _locale: &str,
    ) -> Option<String> {
        let (singular, plural, form, sub_singular, sub_plural) = currency_names(currency);
        let cents = cents.filter(|c| *c > 0);
        let main = || {
            let mut out = cardinal(int, form);
            if needs_de(int) {
                out.push_str(" de");
            }
            out.push(' ');
            out.push_str(if int == 1 { singular } else { plural });
            out
        };
        Some(match cents {
            None => main(),
            Some(c) => {
                if !currency.has_subunit() {
                    return None;
                }
                let sub = format!(
                    "{} {}",
                    cardinal(u64::from(c), Form::Apocope),
                    if c == 1 { sub_singular } else { sub_plural }
                );
                if int == 0 {
                    sub
                } else {
                    format!("{} con {sub}", main())
                }
            }
        })
    }

    fn date(&self, year: u32, month: u32, day: u32, _locale: &str) -> Option<String> {
        let month_name = crate::speech::lexicons::month(SpeechLocale::Es, month)?;
        let day_words = if day == 1 {
            "primero".to_string()
        } else {
            cardinal(u64::from(day), Form::Masculine)
        };
        Some(format!(
            "{day_words} de {month_name} de {}",
            cardinal(u64::from(year), Form::Masculine)
        ))
    }

    fn time(
        &self,
        hour: u32,
        minute: u32,
        meridiem: Option<Meridiem>,
        _locale: &str,
    ) -> Option<String> {
        if meridiem.is_some() || hour > 23 || minute > 59 {
            return None;
        }
        let hour_words = cardinal(u64::from(hour), Form::Feminine);
        Some(match minute {
            0 => format!("{hour_words} en punto"),
            _ => format!(
                "{hour_words} {}",
                cardinal(u64::from(minute), Form::Masculine)
            ),
        })
    }

    fn time_styled(
        &self,
        hour: u32,
        minute: u32,
        meridiem: Option<Meridiem>,
        style: ReadingStyle,
        locale: &str,
    ) -> Option<String> {
        match style {
            ReadingStyle::Precise => self.time(hour, minute, meridiem, locale),
            ReadingStyle::Natural => natural_time(hour, minute, meridiem),
            ReadingStyle::Digits => {
                if meridiem.is_some() || hour > 23 || minute > 59 {
                    return None;
                }
                self.digits(&format!("{hour:02}{minute:02}"), locale)
            }
        }
    }

    fn fraction(&self, numerator: u64, denominator: u64, _locale: &str) -> Option<String> {
        if numerator == 0 || denominator == 0 {
            return None;
        }
        let (singular, plural) = fraction_names(denominator)?;
        if numerator == 1 {
            Some(format!("un {singular}"))
        } else {
            Some(format!("{} {plural}", cardinal(numerator, Form::Masculine)))
        }
    }

    fn scientific(&self, mantissa: &Amount, exponent: i32, _locale: &str) -> Option<String> {
        let power = cardinal(u64::from(exponent.unsigned_abs()), Form::Masculine);
        Some(if exponent < 0 {
            format!(
                "{} por diez elevado a menos {power}",
                amount_words(mantissa, Form::Masculine)
            )
        } else {
            format!(
                "{} por diez elevado a {power}",
                amount_words(mantissa, Form::Masculine)
            )
        })
    }

    fn duration(&self, hours: u64, minutes: u64, seconds: u64, _locale: &str) -> Option<String> {
        let mut parts = Vec::with_capacity(3);
        if hours > 0 {
            parts.push(duration_part(hours, Form::Feminine, "hora", "horas"));
        }
        if minutes > 0 {
            parts.push(duration_part(minutes, Form::Masculine, "minuto", "minutos"));
        }
        if seconds > 0 {
            parts.push(duration_part(
                seconds,
                Form::Masculine,
                "segundo",
                "segundos",
            ));
        }
        if parts.is_empty() {
            return None;
        }
        Some(match parts.as_slice() {
            [only] => only.clone(),
            [first, second] => format!("{first} y {second}"),
            [first, second, third] => format!("{first}, {second} y {third}"),
            _ => unreachable!(),
        })
    }

    fn datetime(
        &self,
        value: &chrono::DateTime<chrono::FixedOffset>,
        fraction: &str,
        locale: &str,
    ) -> Option<String> {
        use chrono::{Datelike, Timelike};
        let date = self.date(
            value.year().try_into().ok()?,
            value.month(),
            value.day(),
            locale,
        )?;
        let hour = value.hour();
        let minute = value.minute();
        let second = value.second();
        let mut result = format!(
            "{date}, {} {}, {} {} y {}",
            cardinal(u64::from(hour), Form::Feminine),
            if hour == 1 { "hora" } else { "horas" },
            cardinal(u64::from(minute), Form::Masculine),
            if minute == 1 { "minuto" } else { "minutos" },
            cardinal(u64::from(second), Form::Masculine)
        );
        if !fraction.is_empty() {
            result.push_str(" coma ");
            result.push_str(
                &fraction
                    .bytes()
                    .map(digit_word)
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        }
        result.push_str(if second == 1 && fraction.is_empty() {
            " segundo"
        } else {
            " segundos"
        });
        let offset = value.offset().local_minus_utc();
        result.push_str(" UTC");
        if offset != 0 {
            result.push_str(if offset < 0 { " menos " } else { " más " });
            let minutes = offset.unsigned_abs() / 60;
            result.push_str(&cardinal(u64::from(minutes / 60), Form::Masculine));
            result.push_str(if minutes / 60 == 1 { " hora" } else { " horas" });
            if !minutes.is_multiple_of(60) {
                result.push_str(" y ");
                result.push_str(&cardinal(u64::from(minutes % 60), Form::Masculine));
                result.push_str(if minutes % 60 == 1 {
                    " minuto"
                } else {
                    " minutos"
                });
            }
        }
        Some(result)
    }

    fn unit(&self, amount: &Amount, unit: Unit, _locale: &str) -> Option<String> {
        let (singular, plural) = unit_names(unit);
        Some(quantity(amount, singular, plural))
    }

    fn range(&self, from: u64, to: u64, unit: Option<Unit>, _locale: &str) -> Option<String> {
        Some(match unit {
            None => format!(
                "{} a {}",
                cardinal(from, Form::Masculine),
                cardinal(to, Form::Masculine)
            ),
            Some(unit) => {
                let (singular, plural) = unit_names(unit);
                let to_amount = Amount {
                    negative: false,
                    int: to,
                    frac: None,
                };
                format!(
                    "{} a {}",
                    cardinal(from, Form::Apocope),
                    quantity(&to_amount, singular, plural)
                )
            }
        })
    }

    fn abbreviation(&self, word: &str) -> Option<(&'static str, AbbrevContext)> {
        Some(match word {
            "Dr" => ("doctor", AbbrevContext::CapitalizedWord),
            "Dra" => ("doctora", AbbrevContext::CapitalizedWord),
            "Sr" => ("señor", AbbrevContext::CapitalizedWord),
            "Sra" => ("señora", AbbrevContext::CapitalizedWord),
            "Srta" => ("señorita", AbbrevContext::CapitalizedWord),
            "Av" => ("avenida", AbbrevContext::CapitalizedWordOrNumber),
            "etc" => ("etcétera", AbbrevContext::NotSentenceEnd),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cardinals() {
        let cases: &[(u64, &str)] = &[
            (0, "cero"),
            (1, "uno"),
            (15, "quince"),
            (16, "dieciséis"),
            (21, "veintiuno"),
            (22, "veintidós"),
            (30, "treinta"),
            (31, "treinta y uno"),
            (99, "noventa y nueve"),
            (100, "cien"),
            (101, "ciento uno"),
            (150, "ciento cincuenta"),
            (200, "doscientos"),
            (500, "quinientos"),
            (999, "novecientos noventa y nueve"),
            (1000, "mil"),
            (1001, "mil uno"),
            (1234, "mil doscientos treinta y cuatro"),
            (2026, "dos mil veintiséis"),
            (21_000, "veintiún mil"),
            (100_000, "cien mil"),
            (101_000, "ciento un mil"),
            (1_000_000, "un millón"),
            (2_500_000, "dos millones quinientos mil"),
            (21_000_000, "veintiún millones"),
            (1_000_000_000, "mil millones"),
            (1_001_000_000, "mil un millones"),
            (1_000_000_000_000, "un billón"),
            (
                999_999_999_999_999,
                "novecientos noventa y nueve billones novecientos noventa y nueve mil \
                 novecientos noventa y nueve millones novecientos noventa y nueve mil \
                 novecientos noventa y nueve",
            ),
        ];
        for (n, expected) in cases {
            assert_eq!(cardinal(*n, Form::Masculine), *expected, "{n}");
        }
    }

    #[test]
    fn apocope_and_feminine() {
        assert_eq!(cardinal(1, Form::Apocope), "un");
        assert_eq!(cardinal(21, Form::Apocope), "veintiún");
        assert_eq!(cardinal(31, Form::Apocope), "treinta y un");
        assert_eq!(cardinal(1, Form::Feminine), "una");
        assert_eq!(cardinal(21, Form::Feminine), "veintiuna");
        assert_eq!(cardinal(200, Form::Feminine), "doscientas");
        assert_eq!(cardinal(201_000, Form::Feminine), "doscientas una mil");
        assert_eq!(cardinal(200_000_000, Form::Feminine), "doscientos millones");
    }

    #[test]
    fn decimals_and_fractions() {
        let amount = |int, frac: &str| Amount {
            negative: false,
            int,
            frac: Some(frac.to_string()),
        };
        assert_eq!(
            amount_words(&amount(12, "5"), Form::Masculine),
            "doce coma cinco"
        );
        assert_eq!(
            amount_words(&amount(1234, "56"), Form::Masculine),
            "mil doscientos treinta y cuatro coma cincuenta y seis"
        );
        assert_eq!(
            amount_words(&amount(3, "05"), Form::Masculine),
            "tres coma cero cinco"
        );
        assert_eq!(
            amount_words(&amount(3, "140"), Form::Masculine),
            "tres coma uno cuatro cero"
        );
    }

    #[test]
    fn ordinals() {
        let rules = Spanish;
        let ord = |n, marker, next| rules.ordinal(n, marker, next, "es-ES");
        assert_eq!(
            ord(1, OrdinalMarker::Masculine, None).as_deref(),
            Some("primero")
        );
        assert_eq!(
            ord(1, OrdinalMarker::Masculine, Some("piso")).as_deref(),
            Some("primer")
        );
        assert_eq!(
            ord(1, OrdinalMarker::Masculine, Some("de")).as_deref(),
            Some("primero")
        );
        assert_eq!(
            ord(3, OrdinalMarker::Masculine, Some("lugar")).as_deref(),
            Some("tercer")
        );
        assert_eq!(
            ord(1, OrdinalMarker::Feminine, None).as_deref(),
            Some("primera")
        );
        assert_eq!(
            ord(13, OrdinalMarker::Feminine, None).as_deref(),
            Some("decimotercera")
        );
        assert_eq!(
            ord(21, OrdinalMarker::Masculine, Some("piso")).as_deref(),
            Some("vigésimo primer")
        );
        assert_eq!(
            ord(100, OrdinalMarker::Masculine, None).as_deref(),
            Some("centésimo")
        );
        assert_eq!(ord(101, OrdinalMarker::Masculine, None), None);
        assert_eq!(ord(0, OrdinalMarker::Masculine, None), None);
    }

    #[test]
    fn currency_agreement() {
        let rules = Spanish;
        let cur = |c, i, cents| rules.currency(c, i, cents, "es-PE").unwrap();
        assert_eq!(
            cur(Currency::Pen, 150, Some(50)),
            "ciento cincuenta soles con cincuenta céntimos"
        );
        assert_eq!(cur(Currency::Pen, 1, None), "un sol");
        assert_eq!(cur(Currency::Pen, 21, None), "veintiún soles");
        assert_eq!(cur(Currency::Pen, 0, Some(1)), "un céntimo");
        assert_eq!(cur(Currency::Pen, 1_000_000, None), "un millón de soles");
        assert_eq!(cur(Currency::Gbp, 1, None), "una libra esterlina");
        assert_eq!(
            cur(Currency::Gbp, 200, None),
            "doscientas libras esterlinas"
        );
        assert_eq!(cur(Currency::Usd, 2, Some(1)), "dos dólares con un centavo");
        assert_eq!(rules.currency(Currency::Jpy, 2, Some(5), "es-ES"), None);
    }

    #[test]
    fn dates_and_times() {
        let rules = Spanish;
        assert_eq!(
            rules.date(2026, 10, 26, "es-PE").unwrap(),
            "veintiséis de octubre de dos mil veintiséis"
        );
        assert_eq!(
            rules.date(2026, 2, 1, "es-PE").unwrap(),
            "primero de febrero de dos mil veintiséis"
        );
        assert_eq!(rules.time(15, 30, None, "es-PE").unwrap(), "quince treinta");
        assert_eq!(rules.time(9, 5, None, "es-PE").unwrap(), "nueve cinco");
        assert_eq!(rules.time(1, 0, None, "es-PE").unwrap(), "una en punto");
        assert_eq!(
            rules.time(21, 21, None, "es-PE").unwrap(),
            "veintiuna veintiuno"
        );
        assert_eq!(rules.time(3, 30, Some(Meridiem::Pm), "es-PE"), None);
    }

    #[test]
    fn quantity_context_requires_a_common_noun_or_unit() {
        let rules = Spanish;
        for word in [
            "mensajes",
            "notificaciones",
            "errores",
            "registros",
            "solicitudes",
            // Units are counts even when the symbol is not alphabetic.
            "kg",
            "°C",
            "km/h",
        ] {
            assert!(rules.quantity_context(word), "{word}");
        }
        for word in ["de", "Bond", "Madrid", "007", ""] {
            assert!(!rules.quantity_context(word), "{word}");
        }
    }

    #[test]
    fn gender_ambiguity() {
        let rules = Spanish;
        assert!(rules.number_gender_ambiguous(21, "casas"));
        assert!(rules.number_gender_ambiguous(200, "personas"));
        assert!(rules.number_gender_ambiguous(1, "casa"));
        assert!(!rules.number_gender_ambiguous(21, "de"));
        assert!(!rules.number_gender_ambiguous(25, "casas"));
        assert!(!rules.number_gender_ambiguous(2026, "hubo"));
        assert!(!rules.number_gender_ambiguous(11, "casas"));
        assert!(!rules.number_gender_ambiguous(21, "."));
    }
}
