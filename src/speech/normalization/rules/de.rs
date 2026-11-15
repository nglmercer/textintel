//! German (`de`; locale de-DE) rules.
//!
//! Number format: decimal comma, dot grouping (`1.234,56`). Bare `1.234`,
//! `1,234` and dot decimals such as `3.14` are ambiguous and preserved by
//! the classifier.
//!
//! Spelling: numbers below one million are one word
//! (`zweitausendsechsundzwanzig`, `einundzwanzig`, `hunderteins`);
//! `Million`, `Milliarde` and `Billion` are separate nouns (`eine Million`,
//! `zwei Millionen`). A final "one" is `eins` when standalone and `ein` /
//! `eine` before a noun (`ein Euro`, `eine Millisekunde`).
//!
//! Not converted, by design:
//! - Standalone integers. German writes ordinals as a digit plus a period
//!   (`am 3. Oktober`, `im 19. Jahrhundert`). The lexer strips the period,
//!   so the rules cannot tell `3.` (ordinal) from `3` at a sentence end.
//!   Integers inside currency, percentages, units and ranges are read;
//!   decimals (`3,14`) are read because they cannot be ordinals.
//! - Clock times. German writes `15:30 Uhr`; the time renderer cannot see
//!   the following `Uhr`, so a reading such as "fünfzehn Uhr dreißig" would
//!   produce "… Uhr dreißig Uhr". Times are preserved with `ambiguous_time`.
//! - Dot dates (`26.10.2026`, `26.10.`): the classifier has no hook for
//!   them; they are preserved.

use super::{Amount, Conventions, Currency, Feature, LanguageRules, Meridiem, OrdinalMarker, Unit};
use crate::speech::normalization::CoverageLevel;
use crate::speech::{numbers, types::SpeechLocale};

/// German rule set.
pub(crate) struct German;

/// Form of a final "one".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum One {
    /// Standalone: `eins`, `hunderteins`.
    Eins,
    /// Before a masculine or neuter noun: `ein Euro`.
    Ein,
    /// Before a feminine noun: `eine Millisekunde`.
    Eine,
}

/// `true` when `word` starts with exactly three digits followed by a
/// non-digit: the second half of a space-grouped number (`1 234,50`).
fn looks_like_group_tail(word: &str) -> bool {
    let bytes = word.as_bytes();
    bytes.len() > 3 && bytes[..3].iter().all(u8::is_ascii_digit) && !bytes[3].is_ascii_digit()
}

/// Shared integer composition with locale-specific presentation.
fn cardinal(n: u64, one: One) -> String {
    numbers::german_cardinal(
        n,
        match one {
            One::Eins => "eins",
            One::Ein => "ein",
            One::Eine => "eine",
        },
        false,
    )
    .expect("normalization cardinal range is checked")
}
fn below_100(n: u64, one: One) -> String {
    debug_assert!(n < 100);
    cardinal(n, one)
}

/// Decimals are read digit by digit after `Komma` (`drei Komma eins vier`).
fn amount_words(amount: &Amount, one: One) -> String {
    let mut out = String::new();
    if amount.negative {
        out.push_str("minus ");
    }
    match &amount.frac {
        None => out.push_str(&cardinal(amount.int, one)),
        Some(frac) => {
            out.push_str(&cardinal(amount.int, One::Eins));
            out.push_str(" Komma");
            for digit in frac.bytes() {
                out.push(' ');
                out.push_str(
                    numbers::digit_word(digit - b'0', SpeechLocale::De).expect("validated digit"),
                );
            }
        }
    }
    out
}

/// Dative/accusative ordinal of a day (`am sechsundzwanzigsten`).
fn day_ordinal(day: u32) -> Option<String> {
    Some(match day {
        1 => "ersten".to_string(),
        3 => "dritten".to_string(),
        7 => "siebten".to_string(),
        8 => "achten".to_string(),
        2..=19 => format!("{}ten", cardinal(u64::from(day), One::Eins)),
        20..=31 => format!("{}sten", cardinal(u64::from(day), One::Ein)),
        _ => return None,
    })
}

/// Years 1100..=1999 are read in hundreds (`neunzehnhundertneunundneunzig`);
/// other years as cardinals (`zweitausendsechsundzwanzig`).
fn year_words(year: u32) -> String {
    let year = u64::from(year);
    if (1100..=1999).contains(&year) {
        let mut out = format!("{}hundert", below_100(year / 100, One::Eins));
        if year % 100 > 0 {
            out.push_str(&below_100(year % 100, One::Eins));
        }
        out
    } else {
        cardinal(year, One::Eins)
    }
}

/// (singular, plural, form of one). Measurement nouns stay singular after
/// a number in German except feminine ones (`zwei Millisekunden`).
fn unit_names(unit: Unit) -> (&'static str, &'static str, One) {
    match unit {
        Unit::Kilogram => ("Kilogramm", "Kilogramm", One::Ein),
        Unit::Gram => ("Gramm", "Gramm", One::Ein),
        Unit::Kilometer => ("Kilometer", "Kilometer", One::Ein),
        Unit::Meter => ("Meter", "Meter", One::Ein),
        Unit::Centimeter => ("Zentimeter", "Zentimeter", One::Ein),
        Unit::Millimeter => ("Millimeter", "Millimeter", One::Ein),
        Unit::Liter => ("Liter", "Liter", One::Ein),
        Unit::Milliliter => ("Milliliter", "Milliliter", One::Ein),
        Unit::Celsius => ("Grad Celsius", "Grad Celsius", One::Ein),
        Unit::Fahrenheit => ("Grad Fahrenheit", "Grad Fahrenheit", One::Ein),
        Unit::KilometersPerHour => ("Kilometer pro Stunde", "Kilometer pro Stunde", One::Ein),
        Unit::MetersPerSecond => ("Meter pro Sekunde", "Meter pro Sekunde", One::Ein),
        Unit::Gigabyte => ("Gigabyte", "Gigabyte", One::Ein),
        Unit::Megahertz => ("Megahertz", "Megahertz", One::Ein),
        Unit::Millisecond => ("Millisekunde", "Millisekunden", One::Eine),
    }
}

/// Currency name (invariable after numbers) and subunit, or `None`.
fn currency_names(currency: Currency) -> Option<(&'static str, &'static str)> {
    Some(match currency {
        Currency::Eur => ("Euro", "Cent"),
        Currency::Usd => ("Dollar", "Cent"),
        Currency::Gbp => ("Pfund", ""),
        Currency::Jpy => ("Yen", ""),
        // German names of the real and the sol are not verified.
        Currency::Brl | Currency::Pen => return None,
    })
}

impl LanguageRules for German {
    fn coverage(&self, feature: Feature) -> CoverageLevel {
        match feature {
            Feature::Decimals
            | Feature::Percentages
            | Feature::Currency
            | Feature::Dates
            | Feature::Units
            | Feature::Abbreviations
            | Feature::Ranges => CoverageLevel::Partial,
            Feature::CardinalNumbers | Feature::Ordinals | Feature::Times | Feature::Fractions => {
                CoverageLevel::None
            }
        }
    }

    fn conventions(&self, locale: &str) -> Option<Conventions> {
        Conventions::for_locale("de", locale)
    }

    fn number(&self, amount: &Amount, _locale: &str) -> Option<String> {
        // Positive integers may be ordinals written with a stripped period;
        // zero has no such ordinal reading.
        if amount.frac.is_none() && amount.int != 0 {
            return None;
        }
        Some(amount_words(amount, One::Eins))
    }

    fn digits(&self, digits: &str, _locale: &str) -> Option<String> {
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some(
            digits
                .bytes()
                .map(|digit| {
                    numbers::digit_word(digit - b'0', SpeechLocale::De).expect("validated digit")
                })
                .collect::<Vec<_>>()
                .join(" "),
        )
    }

    fn number_gender_ambiguous(&self, _int: u64, next_word: &str) -> bool {
        // Integers are never converted; this only adds a warning to the
        // first group of a space-grouped number (`1 234,50`).
        looks_like_group_tail(next_word)
    }

    fn percent(&self, amount: &Amount, _locale: &str) -> Option<String> {
        Some(format!("{} Prozent", amount_words(amount, One::Ein)))
    }

    fn currency(
        &self,
        currency: Currency,
        int: u64,
        cents: Option<u32>,
        _locale: &str,
    ) -> Option<String> {
        let (name, sub) = currency_names(currency)?;
        let main = || format!("{} {name}", cardinal(int, One::Ein));
        Some(match cents.filter(|c| *c > 0) {
            None => main(),
            Some(c) => {
                if sub.is_empty() {
                    return None;
                }
                let sub = format!("{} {sub}", cardinal(u64::from(c), One::Ein));
                if int == 0 {
                    sub
                } else {
                    format!("{} und {sub}", main())
                }
            }
        })
    }

    fn date(&self, year: u32, month: u32, day: u32, _locale: &str) -> Option<String> {
        let month_name = crate::speech::lexicons::month(SpeechLocale::De, month)?;
        Some(format!(
            "{} {month_name} {}",
            day_ordinal(day)?,
            year_words(year)
        ))
    }

    fn datetime(
        &self,
        value: &chrono::DateTime<chrono::FixedOffset>,
        fraction: &str,
        locale: &str,
    ) -> Option<String> {
        use chrono::Datelike;
        let number = |n: u64| cardinal(n, One::Eins);
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
                hour: "Stunden",
                minute: "Minuten",
                second: "Sekunden",
                point: "Komma",
                plus: "plus",
                minus: "minus",
                suffix: false,
            },
            number,
        ))
    }

    fn time(
        &self,
        _hour: u32,
        _minute: u32,
        _meridiem: Option<Meridiem>,
        _locale: &str,
    ) -> Option<String> {
        // `15:30 Uhr` would read "… Uhr dreißig Uhr"; preserved.
        None
    }

    fn unit(&self, amount: &Amount, unit: Unit, _locale: &str) -> Option<String> {
        let (singular, plural, one) = unit_names(unit);
        Some(format!(
            "{} {}",
            amount_words(amount, one),
            if amount.is_one() { singular } else { plural }
        ))
    }

    fn range(&self, from: u64, to: u64, unit: Option<Unit>, _locale: &str) -> Option<String> {
        let mut out = format!(
            "{} bis {}",
            cardinal(from, One::Eins),
            cardinal(to, One::Eins)
        );
        if let Some(unit) = unit {
            let (singular, plural, _) = unit_names(unit);
            out.push(' ');
            out.push_str(if to == 1 { singular } else { plural });
        }
        Some(out)
    }

    fn ordinal(
        &self,
        _n: u64,
        _marker: OrdinalMarker<'_>,
        _next_word: Option<&str>,
        _locale: &str,
    ) -> Option<String> {
        // German ordinals are written `3.`; the markers the classifier
        // recognizes (`º`, `ª`, `st`) are not German.
        None
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
    fn cardinals() {
        let cases: &[(u64, &str)] = &[
            (0, "null"),
            (1, "eins"),
            (11, "elf"),
            (16, "sechzehn"),
            (17, "siebzehn"),
            (21, "einundzwanzig"),
            (30, "dreißig"),
            (99, "neunundneunzig"),
            (100, "hundert"),
            (101, "hunderteins"),
            (200, "zweihundert"),
            (1000, "tausend"),
            (1001, "tausendeins"),
            (2026, "zweitausendsechsundzwanzig"),
            (201_000, "zweihunderteintausend"),
            (1_000_000, "eine Million"),
            (2_000_000, "zwei Millionen"),
            (
                1_234_567,
                "eine Million zweihundertvierunddreißigtausendfünfhundertsiebenundsechzig",
            ),
            (1_000_000_000, "eine Milliarde"),
            (3_000_000_000_000, "drei Billionen"),
        ];
        for (n, expected) in cases {
            assert_eq!(cardinal(*n, One::Eins), *expected, "{n}");
        }
        assert_eq!(cardinal(1, One::Ein), "ein");
        assert_eq!(cardinal(101, One::Ein), "hundertein");
        assert_eq!(cardinal(1, One::Eine), "eine");
    }

    #[test]
    fn integers_are_preserved_decimals_read() {
        let rules = German;
        assert_eq!(rules.number(&amount(3, None), "de-DE"), None);
        assert_eq!(
            rules.number(&amount(3, Some("14")), "de-DE").unwrap(),
            "drei Komma eins vier"
        );
        assert_eq!(
            rules.number(&amount(1, Some("5")), "de-DE").unwrap(),
            "eins Komma fünf"
        );
    }

    #[test]
    fn percent_currency() {
        let rules = German;
        assert_eq!(
            rules.percent(&amount(12, Some("5")), "de-DE").unwrap(),
            "zwölf Komma fünf Prozent"
        );
        assert_eq!(
            rules.percent(&amount(1, None), "de-DE").unwrap(),
            "ein Prozent"
        );
        let cur = |c, i, cents| rules.currency(c, i, cents, "de-DE");
        assert_eq!(
            cur(Currency::Eur, 12, Some(50)).unwrap(),
            "zwölf Euro und fünfzig Cent"
        );
        assert_eq!(cur(Currency::Eur, 1, None).unwrap(), "ein Euro");
        assert_eq!(cur(Currency::Eur, 0, Some(1)).unwrap(), "ein Cent");
        assert_eq!(
            cur(Currency::Eur, 1_000_000, None).unwrap(),
            "eine Million Euro"
        );
        assert_eq!(cur(Currency::Gbp, 5, None).unwrap(), "fünf Pfund");
        assert_eq!(cur(Currency::Gbp, 5, Some(20)), None);
        assert_eq!(cur(Currency::Pen, 5, None), None);
    }

    #[test]
    fn dates() {
        let rules = German;
        assert_eq!(
            rules.date(2026, 10, 26, "de-DE").unwrap(),
            "sechsundzwanzigsten Oktober zweitausendsechsundzwanzig"
        );
        assert_eq!(
            rules.date(1999, 3, 1, "de-DE").unwrap(),
            "ersten März neunzehnhundertneunundneunzig"
        );
        assert_eq!(day_ordinal(7).unwrap(), "siebten");
        assert_eq!(day_ordinal(8).unwrap(), "achten");
        assert_eq!(day_ordinal(16).unwrap(), "sechzehnten");
        assert_eq!(day_ordinal(21).unwrap(), "einundzwanzigsten");
        assert_eq!(day_ordinal(30).unwrap(), "dreißigsten");
        assert_eq!(day_ordinal(32), None);
        assert_eq!(year_words(2001), "zweitausendeins");
        assert_eq!(year_words(1901), "neunzehnhunderteins");
        assert_eq!(rules.time(15, 30, None, "de-DE"), None);
    }

    #[test]
    fn units_ranges() {
        let rules = German;
        assert_eq!(
            rules
                .unit(&amount(1, None), Unit::Kilogram, "de-DE")
                .unwrap(),
            "ein Kilogramm"
        );
        assert_eq!(
            rules
                .unit(&amount(5, None), Unit::Kilometer, "de-DE")
                .unwrap(),
            "fünf Kilometer"
        );
        assert_eq!(
            rules
                .unit(&amount(1, None), Unit::Millisecond, "de-DE")
                .unwrap(),
            "eine Millisekunde"
        );
        assert_eq!(
            rules
                .unit(&amount(2, Some("5")), Unit::Millisecond, "de-DE")
                .unwrap(),
            "zwei Komma fünf Millisekunden"
        );
        assert_eq!(
            rules.range(10, 15, None, "de-DE").unwrap(),
            "zehn bis fünfzehn"
        );
        assert!(rules.number_gender_ambiguous(1, "234,50"));
        assert!(!rules.number_gender_ambiguous(1, "Hund"));
    }
}
