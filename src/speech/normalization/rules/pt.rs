//! Portuguese (`pt`; locales pt-BR, pt-PT) rules.
//!
//! Number format: decimal comma, dot grouping (`1.234,56`). Bare `1.234`,
//! `1,234` and dot decimals such as `3.14` are ambiguous and preserved by
//! the classifier, as in Spanish.
//!
//! Locale differences handled: teens (`dezesseis`/`dezasseis`,
//! `dezessete`/`dezassete`, `dezenove`/`dezanove`), the scale above a
//! million (pt-BR `bilhão` = 10^9; pt-PT `mil milhões` = 10^9 and `bilião`
//! = 10^12), `quilômetro`/`quilómetro`, the euro subunit (`centavo` /
//! `cêntimo`) and day 1 in dates (pt-BR `primeiro`, pt-PT `um`).
//!
//! Grammar handled: the connector `e` (`cento e vinte e três`, `mil e
//! quinhentos`, `mil duzentos e trinta e quatro`), feminine agreement for
//! feminine currency names and hours (`uma libra`, `duas horas`), `de`
//! after exact millions (`um milhão de reais`). A bare integer before an
//! unknown noun whose reading depends on gender (`1 casa`, `2 casas`,
//! `200 pessoas`) is preserved as ambiguous.
//!
//! pt-PT also groups thousands with spaces (`1 234,50 €`). The lexer splits
//! such numbers at the space, so under pt-PT a decimal amount whose integer
//! part has exactly three digits (`234,50`) is preserved: it may be the
//! tail of a space-grouped number.

use super::{Amount, Conventions, Currency, Feature, LanguageRules, Meridiem, OrdinalMarker, Unit};
use crate::speech::normalization::CoverageLevel;
use crate::speech::{numbers, types::SpeechLocale};

/// Portuguese rule set.
pub(crate) struct Portuguese;

/// Grammatical gender of the counted noun.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gender {
    Masculine,
    Feminine,
}

/// Masculine ordinal units 1..=9.
const ORD_UNITS: [&str; 10] = [
    "", "primeiro", "segundo", "terceiro", "quarto", "quinto", "sexto", "sétimo", "oitavo", "nono",
];

/// Masculine ordinal tens 10..=90.
const ORD_TENS: [&str; 10] = [
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
];

/// Function words after which a number keeps its standalone reading.
const STANDALONE_FOLLOWERS: &[&str] = &[
    "a", "à", "ao", "aos", "as", "às", "até", "com", "como", "contra", "da", "das", "de", "desde",
    "do", "dos", "e", "é", "em", "entre", "era", "eram", "foi", "foram", "mais", "menos", "na",
    "nas", "nem", "no", "nos", "o", "os", "ou", "para", "pela", "pelas", "pelo", "pelos", "por",
    "que", "se", "sem", "sobre", "são",
];

fn brazilian(locale: &str) -> bool {
    locale != "pt-PT"
}

/// `true` for locales that group thousands with spaces.
fn space_grouping(locale: &str) -> bool {
    locale == "pt-PT"
}

/// A decimal amount with a three-digit integer part may be the tail of a
/// space-grouped number (`1 234,50`) under a space-grouping locale.
fn possible_group_tail(int: u64, has_fraction: bool, locale: &str) -> bool {
    space_grouping(locale) && has_fraction && (100..=999).contains(&int)
}

/// `true` when `word` starts with exactly three digits followed by a
/// non-digit: the second half of a space-grouped number (`1 234,50`).
fn looks_like_group_tail(word: &str) -> bool {
    let bytes = word.as_bytes();
    bytes.len() > 3 && bytes[..3].iter().all(u8::is_ascii_digit) && !bytes[3].is_ascii_digit()
}

/// Shared integer composition with locale-specific presentation.
fn cardinal(n: u64, gender: Gender, locale: &str) -> String {
    numbers::portuguese_cardinal(n, gender == Gender::Feminine, brazilian(locale))
        .expect("normalization cardinal range is checked")
}

/// `true` when a noun after `n` is introduced by `de` (`um milhão de reais`).
fn needs_de(n: u64) -> bool {
    n >= 1_000_000 && n.is_multiple_of(1_000_000)
}

/// Fractional part: one digit, or two digits without a leading zero, read
/// as a number; anything else digit by digit.
fn fraction_words(frac: &str, br: bool) -> String {
    let bytes = frac.as_bytes();
    if bytes.len() == 1 || (bytes.len() == 2 && bytes[0] != b'0') {
        let value = frac.parse::<u64>().unwrap_or(0);
        return numbers::portuguese_cardinal(value, false, br)
            .expect("fraction digits are bounded");
    }
    bytes
        .iter()
        .map(|d| numbers::digit_word(d - b'0', SpeechLocale::Pt).expect("validated digit"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn amount_words(amount: &Amount, gender: Gender, locale: &str) -> String {
    let mut out = String::new();
    if amount.negative {
        out.push_str("menos ");
    }
    match &amount.frac {
        None => out.push_str(&cardinal(amount.int, gender, locale)),
        Some(frac) => {
            out.push_str(&cardinal(amount.int, Gender::Masculine, locale));
            out.push_str(" vírgula ");
            out.push_str(&fraction_words(frac, brazilian(locale)));
        }
    }
    out
}

/// Masculine ordinal for 1..=100.
fn ordinal_masculine(n: u64) -> Option<String> {
    Some(match n {
        1..=9 => ORD_UNITS[n as usize].to_string(),
        10..=99 => {
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

fn noun_like(next_word: &str) -> bool {
    next_word.chars().next().is_some_and(char::is_alphabetic) && !is_standalone_follower(next_word)
}

/// `true` when the three-digit group changes with gender
/// (`um`/`uma`, `dois`/`duas`, `duzentos`/`duzentas`).
fn gender_sensitive_group(group: u64) -> bool {
    let last_two = group % 100;
    (matches!(group % 10, 1 | 2) && !matches!(last_two, 11 | 12)) || group / 100 >= 2
}

/// (singular, plural, gender, subunit singular, subunit plural). An empty
/// subunit means amounts with cents are preserved.
type CurrencyNames = (
    &'static str,
    &'static str,
    Gender,
    &'static str,
    &'static str,
);

fn currency_names(currency: Currency, br: bool) -> Option<CurrencyNames> {
    Some(match currency {
        Currency::Brl => ("real", "reais", Gender::Masculine, "centavo", "centavos"),
        Currency::Eur if br => ("euro", "euros", Gender::Masculine, "centavo", "centavos"),
        Currency::Eur => ("euro", "euros", Gender::Masculine, "cêntimo", "cêntimos"),
        Currency::Usd => ("dólar", "dólares", Gender::Masculine, "centavo", "centavos"),
        Currency::Gbp => (
            "libra esterlina",
            "libras esterlinas",
            Gender::Feminine,
            "",
            "",
        ),
        Currency::Jpy => ("iene", "ienes", Gender::Masculine, "", ""),
        // The Portuguese name of the Peruvian sol is not verified.
        Currency::Pen => return None,
    })
}

fn unit_names(unit: Unit, br: bool) -> (&'static str, &'static str) {
    match (unit, br) {
        (Unit::Kilogram, _) => ("quilograma", "quilogramas"),
        (Unit::Gram, _) => ("grama", "gramas"),
        (Unit::Kilometer, true) => ("quilômetro", "quilômetros"),
        (Unit::Kilometer, false) => ("quilómetro", "quilómetros"),
        (Unit::Meter, _) => ("metro", "metros"),
        (Unit::Centimeter, _) => ("centímetro", "centímetros"),
        (Unit::Millimeter, _) => ("milímetro", "milímetros"),
        (Unit::Liter, _) => ("litro", "litros"),
        (Unit::Milliliter, _) => ("mililitro", "mililitros"),
        (Unit::Celsius, _) => ("grau Celsius", "graus Celsius"),
        (Unit::Fahrenheit, _) => ("grau Fahrenheit", "graus Fahrenheit"),
        (Unit::KilometersPerHour, true) => ("quilômetro por hora", "quilômetros por hora"),
        (Unit::KilometersPerHour, false) => ("quilómetro por hora", "quilómetros por hora"),
        (Unit::MetersPerSecond, _) => ("metro por segundo", "metros por segundo"),
        (Unit::Gigabyte, _) => ("gigabyte", "gigabytes"),
        (Unit::Megahertz, _) => ("megahertz", "megahertz"),
        (Unit::Millisecond, _) => ("milissegundo", "milissegundos"),
    }
}

impl LanguageRules for Portuguese {
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
            Feature::Fractions => CoverageLevel::None,
        }
    }

    fn conventions(&self, locale: &str) -> Option<Conventions> {
        Conventions::for_locale("pt", locale)
    }

    fn number(&self, amount: &Amount, locale: &str) -> Option<String> {
        if possible_group_tail(amount.int, amount.frac.is_some(), locale) {
            return None;
        }
        Some(amount_words(amount, Gender::Masculine, locale))
    }

    fn digits(&self, digits: &str, _locale: &str) -> Option<String> {
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some(
            digits
                .bytes()
                .map(|digit| {
                    numbers::digit_word(digit - b'0', SpeechLocale::Pt).expect("validated digit")
                })
                .collect::<Vec<_>>()
                .join(" "),
        )
    }

    fn number_gender_ambiguous(&self, int: u64, next_word: &str) -> bool {
        // `15 h` / `15 h 30` is a clock time the classifier does not
        // recognize; the hour is preserved.
        if looks_like_group_tail(next_word) || next_word == "h" {
            return true;
        }
        if !noun_like(next_word) {
            return false;
        }
        let units = int % 1000;
        let thousands = (int / 1000) % 1000;
        (units > 0 && gender_sensitive_group(units))
            || (thousands > 0 && gender_sensitive_group(thousands))
    }

    fn ordinal(
        &self,
        n: u64,
        marker: OrdinalMarker<'_>,
        _next_word: Option<&str>,
        _locale: &str,
    ) -> Option<String> {
        let masculine = ordinal_masculine(n)?;
        match marker {
            OrdinalMarker::Masculine => Some(masculine),
            OrdinalMarker::Feminine => Some(feminize(&masculine)),
            OrdinalMarker::Suffix(_) => None,
        }
    }

    fn percent(&self, amount: &Amount, locale: &str) -> Option<String> {
        if possible_group_tail(amount.int, amount.frac.is_some(), locale) {
            return None;
        }
        Some(format!(
            "{} por cento",
            amount_words(amount, Gender::Masculine, locale)
        ))
    }

    fn currency(
        &self,
        currency: Currency,
        int: u64,
        cents: Option<u32>,
        locale: &str,
    ) -> Option<String> {
        if possible_group_tail(int, cents.is_some(), locale) {
            return None;
        }
        let br = brazilian(locale);
        let (singular, plural, gender, sub_singular, sub_plural) = currency_names(currency, br)?;
        let main = || {
            let mut out = cardinal(int, gender, locale);
            if needs_de(int) {
                out.push_str(" de");
            }
            out.push(' ');
            out.push_str(if int == 1 { singular } else { plural });
            out
        };
        Some(match cents.filter(|c| *c > 0) {
            None => main(),
            Some(c) => {
                if sub_singular.is_empty() {
                    return None;
                }
                let sub = format!(
                    "{} {}",
                    cardinal(u64::from(c), Gender::Masculine, locale),
                    if c == 1 { sub_singular } else { sub_plural }
                );
                if int == 0 {
                    sub
                } else {
                    format!("{} e {sub}", main())
                }
            }
        })
    }

    fn date(&self, year: u32, month: u32, day: u32, locale: &str) -> Option<String> {
        let month_name = crate::speech::lexicons::month(SpeechLocale::Pt, month)?;
        let day_words = if day == 1 && brazilian(locale) {
            "primeiro".to_string()
        } else {
            cardinal(u64::from(day), Gender::Masculine, locale)
        };
        Some(format!(
            "{day_words} de {month_name} de {}",
            cardinal(u64::from(year), Gender::Masculine, locale)
        ))
    }

    fn datetime(
        &self,
        value: &chrono::DateTime<chrono::FixedOffset>,
        fraction: &str,
        locale: &str,
    ) -> Option<String> {
        use chrono::Datelike;
        let number = |n: u64| cardinal(n, Gender::Masculine, locale);
        let date = self.date(
            value.year().try_into().ok()?,
            value.month(),
            value.day(),
            locale,
        )?;
        Some(super::timestamp_parts(
            date,
            value,
            fraction,
            &super::TimestampLabels {
                hour: "horas",
                minute: "minutos",
                second: "segundos",
                point: "vírgula",
                plus: "mais",
                minus: "menos",
                suffix: false,
            },
            number,
        ))
    }

    fn time(
        &self,
        hour: u32,
        minute: u32,
        meridiem: Option<Meridiem>,
        locale: &str,
    ) -> Option<String> {
        if meridiem.is_some() || !(1..=23).contains(&hour) || minute > 59 {
            return None;
        }
        let hour_words = cardinal(u64::from(hour), Gender::Feminine, locale);
        Some(match minute {
            0 if hour == 1 => format!("{hour_words} hora"),
            0 => format!("{hour_words} horas"),
            _ => format!(
                "{hour_words} e {}",
                cardinal(u64::from(minute), Gender::Masculine, locale)
            ),
        })
    }

    fn unit(&self, amount: &Amount, unit: Unit, locale: &str) -> Option<String> {
        // Singular or plural for 0,5 or 1,5 is a matter of style; preserve.
        if amount.frac.is_some() && amount.int < 2 {
            return None;
        }
        if possible_group_tail(amount.int, amount.frac.is_some(), locale) {
            return None;
        }
        let (singular, plural) = unit_names(unit, brazilian(locale));
        let mut out = amount_words(amount, Gender::Masculine, locale);
        if amount.frac.is_none() && needs_de(amount.int) {
            out.push_str(" de");
        }
        out.push(' ');
        out.push_str(if amount.is_one() { singular } else { plural });
        Some(out)
    }

    fn range(&self, from: u64, to: u64, unit: Option<Unit>, locale: &str) -> Option<String> {
        let mut out = format!(
            "{} a {}",
            cardinal(from, Gender::Masculine, locale),
            cardinal(to, Gender::Masculine, locale)
        );
        if let Some(unit) = unit {
            let (singular, plural) = unit_names(unit, brazilian(locale));
            out.push(' ');
            out.push_str(if to == 1 { singular } else { plural });
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn amount(int: u64, frac: Option<&str>) -> Amount {
        Amount {
            negative: false,
            int,
            frac: frac.map(str::to_string),
        }
    }

    #[test]
    fn cardinals_brazil() {
        let cases: &[(u64, &str)] = &[
            (0, "zero"),
            (1, "um"),
            (2, "dois"),
            (16, "dezesseis"),
            (17, "dezessete"),
            (19, "dezenove"),
            (21, "vinte e um"),
            (99, "noventa e nove"),
            (100, "cem"),
            (101, "cento e um"),
            (123, "cento e vinte e três"),
            (200, "duzentos"),
            (1000, "mil"),
            (1001, "mil e um"),
            (1100, "mil e cem"),
            (1234, "mil duzentos e trinta e quatro"),
            (1500, "mil e quinhentos"),
            (2026, "dois mil e vinte e seis"),
            (21_000, "vinte e um mil"),
            (1_000_000, "um milhão"),
            (1_200_000, "um milhão e duzentos mil"),
            (
                1_234_567,
                "um milhão duzentos e trinta e quatro mil quinhentos e sessenta e sete",
            ),
            (2_000_026, "dois milhões e vinte e seis"),
            (1_000_000_000, "um bilhão"),
            (3_000_000_000_000, "três trilhões"),
        ];
        for (n, expected) in cases {
            assert_eq!(cardinal(*n, Gender::Masculine, "pt-BR"), *expected, "{n}");
        }
    }

    #[test]
    fn cardinals_portugal() {
        assert_eq!(cardinal(16, Gender::Masculine, "pt-PT"), "dezasseis");
        assert_eq!(cardinal(17, Gender::Masculine, "pt-PT"), "dezassete");
        assert_eq!(cardinal(19, Gender::Masculine, "pt-PT"), "dezanove");
        assert_eq!(
            cardinal(1_000_000_000, Gender::Masculine, "pt-PT"),
            "mil milhões"
        );
        assert_eq!(
            cardinal(2_500_000_000, Gender::Masculine, "pt-PT"),
            "dois mil e quinhentos milhões"
        );
        assert_eq!(
            cardinal(1_000_000_000_000, Gender::Masculine, "pt-PT"),
            "um bilião"
        );
    }

    #[test]
    fn feminine() {
        assert_eq!(cardinal(1, Gender::Feminine, "pt-BR"), "uma");
        assert_eq!(cardinal(2, Gender::Feminine, "pt-BR"), "duas");
        assert_eq!(cardinal(22, Gender::Feminine, "pt-BR"), "vinte e duas");
        assert_eq!(cardinal(200, Gender::Feminine, "pt-BR"), "duzentas");
        assert_eq!(cardinal(100, Gender::Feminine, "pt-BR"), "cem");
        assert_eq!(
            cardinal(150, Gender::Feminine, "pt-BR"),
            "cento e cinquenta"
        );
        assert_eq!(
            cardinal(2_000_000, Gender::Feminine, "pt-BR"),
            "dois milhões"
        );
    }

    #[test]
    fn decimals_and_percent() {
        let rules = Portuguese;
        assert_eq!(
            rules.number(&amount(12, Some("5")), "pt-BR").unwrap(),
            "doze vírgula cinco"
        );
        assert_eq!(
            rules.number(&amount(3, Some("05")), "pt-BR").unwrap(),
            "três vírgula zero cinco"
        );
        assert_eq!(
            rules.percent(&amount(12, Some("5")), "pt-BR").unwrap(),
            "doze vírgula cinco por cento"
        );
        let negative = Amount {
            negative: true,
            int: 3,
            frac: None,
        };
        assert_eq!(
            rules.percent(&negative, "pt-PT").unwrap(),
            "menos três por cento"
        );
        // Possible tail of a space-grouped number under pt-PT.
        assert_eq!(rules.number(&amount(234, Some("5")), "pt-PT"), None);
        assert!(rules.number(&amount(234, Some("5")), "pt-BR").is_some());
    }

    #[test]
    fn currency() {
        let rules = Portuguese;
        let cur = |c, i, cents, l| rules.currency(c, i, cents, l);
        assert_eq!(
            cur(Currency::Brl, 12, Some(50), "pt-BR").unwrap(),
            "doze reais e cinquenta centavos"
        );
        assert_eq!(cur(Currency::Brl, 1, None, "pt-BR").unwrap(), "um real");
        assert_eq!(
            cur(Currency::Brl, 0, Some(1), "pt-BR").unwrap(),
            "um centavo"
        );
        assert_eq!(
            cur(Currency::Brl, 1_000_000, None, "pt-BR").unwrap(),
            "um milhão de reais"
        );
        assert_eq!(
            cur(Currency::Eur, 2, Some(10), "pt-PT").unwrap(),
            "dois euros e dez cêntimos"
        );
        assert_eq!(
            cur(Currency::Gbp, 2, None, "pt-BR").unwrap(),
            "duas libras esterlinas"
        );
        assert_eq!(cur(Currency::Gbp, 2, Some(5), "pt-BR"), None);
        assert_eq!(cur(Currency::Pen, 2, None, "pt-BR"), None);
        assert_eq!(
            cur(Currency::Jpy, 500, None, "pt-BR").unwrap(),
            "quinhentos ienes"
        );
        assert_eq!(cur(Currency::Eur, 234, Some(50), "pt-PT"), None);
    }

    #[test]
    fn dates_and_times() {
        let rules = Portuguese;
        assert_eq!(
            rules.date(2026, 10, 26, "pt-BR").unwrap(),
            "vinte e seis de outubro de dois mil e vinte e seis"
        );
        assert_eq!(
            rules.date(2026, 5, 1, "pt-BR").unwrap(),
            "primeiro de maio de dois mil e vinte e seis"
        );
        assert_eq!(
            rules.date(2026, 5, 1, "pt-PT").unwrap(),
            "um de maio de dois mil e vinte e seis"
        );
        assert_eq!(
            rules.time(15, 30, None, "pt-BR").unwrap(),
            "quinze e trinta"
        );
        assert_eq!(rules.time(9, 5, None, "pt-BR").unwrap(), "nove e cinco");
        assert_eq!(rules.time(1, 0, None, "pt-BR").unwrap(), "uma hora");
        assert_eq!(
            rules.time(22, 0, None, "pt-BR").unwrap(),
            "vinte e duas horas"
        );
        assert_eq!(rules.time(0, 30, None, "pt-BR"), None);
        assert_eq!(rules.time(3, 30, Some(Meridiem::Pm), "pt-BR"), None);
    }

    #[test]
    fn ordinals_units_ranges() {
        let rules = Portuguese;
        let ord = |n, m| rules.ordinal(n, m, None, "pt-BR");
        assert_eq!(ord(1, OrdinalMarker::Masculine).unwrap(), "primeiro");
        assert_eq!(ord(1, OrdinalMarker::Feminine).unwrap(), "primeira");
        assert_eq!(
            ord(21, OrdinalMarker::Masculine).unwrap(),
            "vigésimo primeiro"
        );
        assert_eq!(ord(12, OrdinalMarker::Feminine).unwrap(), "décima segunda");
        assert_eq!(ord(101, OrdinalMarker::Masculine), None);
        assert_eq!(ord(1, OrdinalMarker::Suffix("st")), None);
        assert_eq!(
            rules
                .unit(&amount(5, None), Unit::Kilometer, "pt-BR")
                .unwrap(),
            "cinco quilômetros"
        );
        assert_eq!(
            rules
                .unit(&amount(5, None), Unit::Kilometer, "pt-PT")
                .unwrap(),
            "cinco quilómetros"
        );
        assert_eq!(
            rules
                .unit(&amount(1, None), Unit::Kilogram, "pt-BR")
                .unwrap(),
            "um quilograma"
        );
        assert_eq!(
            rules.unit(&amount(1, Some("5")), Unit::Liter, "pt-BR"),
            None
        );
        assert_eq!(
            rules
                .unit(&amount(2, Some("5")), Unit::Liter, "pt-BR")
                .unwrap(),
            "dois vírgula cinco litros"
        );
        assert_eq!(
            rules.range(10, 15, Some(Unit::Kilogram), "pt-BR").unwrap(),
            "dez a quinze quilogramas"
        );
    }

    /// Deterministic fuzz of the whole pipeline for the eight languages
    /// added with these rules (pt, fr, de, it, ru, zh, ja, ko); the
    /// framework fuzz covers es, en and ja only. Checks: no panic, only
    /// budget errors, bytes outside changes preserved, idempotence for
    /// digit-free output.
    #[test]
    fn multilingual_pipeline_fuzz() {
        use crate::speech::normalization::{
            NormalizationMode, NormalizeError, NormalizeLimits, NormalizeOptions, normalize,
        };
        const PIECES: &[&str] = &[
            "0",
            "1",
            "2",
            "7",
            "12",
            "21",
            "71",
            "80",
            "99",
            "100",
            "234",
            "1000",
            "2026",
            "999999999999999",
            "1000000000000000",
            ".",
            ",",
            ":",
            "/",
            "-",
            "–",
            "%",
            " %",
            "\u{202f}%",
            "€",
            "R$",
            "$",
            "£",
            "¥",
            "USD",
            "JPY",
            "°C",
            "km",
            "kg",
            "km/h",
            "ms",
            "º",
            "ª",
            " ",
            " ",
            "\n",
            "\u{a0}",
            "рублей",
            "рубль",
            "год",
            "лет",
            "км",
            "个",
            "人",
            "円",
            "원",
            "개",
            "년",
            "월",
            "日本",
            "年",
            "Uhr",
            "h",
            "casas",
            "maison",
            "Dr.",
            "Sr.",
            "Nr.",
            "Dott.",
            "😊",
            "👨\u{200D}👩",
            "e\u{301}",
            "`",
            "<",
            "@",
            "x",
        ];
        let mut state: u64 = 0x0DDB_A11C_0FFE_E001;
        let mut next = |n: usize| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state % n as u64) as usize
        };
        let configs: Vec<NormalizeOptions> = [
            ("pt", Some("pt-BR")),
            ("pt", Some("pt-PT")),
            ("fr", None),
            ("de", None),
            ("it", None),
            ("ru", None),
            ("zh", Some("zh-CN")),
            ("zh", Some("zh-TW")),
            ("ja", None),
            ("ko", None),
        ]
        .iter()
        .map(|(language, locale)| {
            NormalizeOptions::resolve(
                NormalizationMode::Auto,
                language,
                *locale,
                NormalizeLimits::default(),
                true,
            )
            .expect("valid options")
        })
        .collect();
        for round in 0..4000 {
            let len = next(20);
            let text: String = (0..len).map(|_| PIECES[next(PIECES.len())]).collect();
            let opts = &configs[round % configs.len()];
            let result = match normalize(&text, opts) {
                Ok(result) => result,
                Err(NormalizeError::ExpansionTooLarge | NormalizeError::OutputTooLong { .. }) => {
                    continue;
                }
                Err(other) => panic!("unexpected error {other:?} for {text:?}"),
            };
            let mut rebuilt = String::new();
            let mut spoken_cursor = 0usize;
            let mut original_cursor = 0usize;
            for change in &result.changes {
                assert!(change.start >= original_cursor && change.end > change.start);
                let keep = change.start - original_cursor;
                rebuilt.push_str(&result.spoken[spoken_cursor..spoken_cursor + keep]);
                rebuilt.push_str(&text[change.start..change.end]);
                spoken_cursor += keep + change.replacement.len();
                original_cursor = change.end;
            }
            rebuilt.push_str(&result.spoken[spoken_cursor..]);
            assert_eq!(rebuilt, text, "bytes outside changes modified");
            if !result.spoken.bytes().any(|b| b.is_ascii_digit()) {
                let again = normalize(&result.spoken, opts).expect("second pass");
                assert_eq!(again.spoken, result.spoken, "not idempotent for {text:?}");
            }
        }
    }

    #[test]
    fn gender_ambiguity() {
        let rules = Portuguese;
        assert!(rules.number_gender_ambiguous(1, "casa"));
        assert!(rules.number_gender_ambiguous(2, "casas"));
        assert!(rules.number_gender_ambiguous(22, "pessoas"));
        assert!(rules.number_gender_ambiguous(200, "pessoas"));
        assert!(!rules.number_gender_ambiguous(12, "casas"));
        assert!(!rules.number_gender_ambiguous(5, "casas"));
        assert!(!rules.number_gender_ambiguous(2, "de"));
        assert!(rules.number_gender_ambiguous(1, "234,50"));
    }
}
