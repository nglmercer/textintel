//! Japanese (`ja`; locale ja-JP) rules.
//!
//! Number format: decimal point, comma grouping (`1,234.5`), read strictly
//! by convention. Readings use kanji numerals with 万/億/兆 sections and no
//! zero placeholders: `十` (not `一十`), `百`, `千`, `一万`, `二千二十六`,
//! `千一`. `千` takes `一` only as an exact thousand before a section unit
//! (`一千万`, but `千二百万`).
//!
//! Chosen readings (documented, not reviewed by a native speaker):
//! - years in dates are cardinals (`二千二十六年十月二十六日`);
//! - decimals are read digit by digit after `点` (`三点一四`), zero is `零`;
//! - `°C` reads as `度` (`二十五度`), `°F` as `華氏…度`, speeds with the
//!   prefixes `時速` / `秒速`.
//!
//! Kanji numerals keep the counter reading to the synthesizer
//! (`三本` is still read さんぼん). The rules do not decide counter
//! readings: a number followed by a separate kanji/kana word (a counter
//! or noun) is preserved with `ambiguous_number`.
//!
//! Limits: Japanese text usually has no spaces, and the classifier only
//! looks at whitespace-delimited fields, so numbers attached to kanji
//! (`2026年10月26日`, `500円`, `3人`) are preserved unchanged. `¥` is
//! ambiguous (yen or yuan) and preserved by the classifier; the code `JPY`
//! converts. Amounts with cents are preserved. Numeric slash dates are not
//! a Japanese convention; when both day/month readings are valid under an
//! explicit locale, month/day is used.

use super::{Amount, Conventions, Currency, Feature, LanguageRules, Meridiem, Unit};
use crate::speech::normalization::CoverageLevel;
use crate::speech::{numbers, types::SpeechLocale};

/// Japanese rule set.
pub(crate) struct Japanese;

/// `true` when `word` starts with exactly three digits followed by a
/// non-digit: the second half of a space-grouped number.
fn looks_like_group_tail(word: &str) -> bool {
    let bytes = word.as_bytes();
    bytes.len() > 3 && bytes[..3].iter().all(u8::is_ascii_digit) && !bytes[3].is_ascii_digit()
}

/// Han, kana or Hangul letter.
fn is_cjk_letter(c: char) -> bool {
    matches!(c,
        '\u{3040}'..='\u{30FF}'
        | '\u{3400}'..='\u{4DBF}'
        | '\u{4E00}'..='\u{9FFF}'
        | '\u{AC00}'..='\u{D7AF}'
        | '\u{F900}'..='\u{FAFF}'
    )
}

fn cardinal(n: u64) -> String {
    let value = crate::speech::types::NumberValue {
        negative: false,
        integer: n.to_string(),
        fraction: None,
    };
    numbers::verbalize_number_with_options(
        &value,
        SpeechLocale::Ja,
        crate::speech::types::NumberReading::Cardinal,
        crate::speech::types::ReadingStyle::Precise,
        crate::speech::types::NumberNotation::Orthographic,
    )
    .expect("normalization cardinal range is checked")
}
fn digit_word(digit: u8) -> &'static str {
    numbers::cjk_digit_word(digit).expect("validated digit")
}

fn unsigned_words(amount: &Amount) -> String {
    let mut out = cardinal(amount.int);
    if let Some(frac) = &amount.frac {
        out.push('点');
        out.extend(frac.bytes().map(|d| digit_word(d - b'0')));
    }
    out
}

fn amount_words(amount: &Amount) -> String {
    let words = unsigned_words(amount);
    if amount.negative {
        format!("マイナス{words}")
    } else {
        words
    }
}

impl LanguageRules for Japanese {
    fn coverage(&self, feature: Feature) -> CoverageLevel {
        match feature {
            Feature::CardinalNumbers
            | Feature::Decimals
            | Feature::Percentages
            | Feature::Currency
            | Feature::Dates
            | Feature::Times
            | Feature::Units => CoverageLevel::Partial,
            Feature::Ordinals | Feature::Abbreviations | Feature::Ranges | Feature::Fractions => {
                CoverageLevel::None
            }
        }
    }

    fn conventions(&self, locale: &str) -> Option<Conventions> {
        Conventions::for_locale("ja", locale)
    }

    fn number(&self, amount: &Amount, _locale: &str) -> Option<String> {
        Some(amount_words(amount))
    }

    fn digits(&self, digits: &str, _locale: &str) -> Option<String> {
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some(
            digits
                .bytes()
                .map(|digit| digit_word(digit - b'0').to_string())
                .collect(),
        )
    }

    /// A following kanji/kana word is usually a counter or a noun whose
    /// reading the rules do not decide; preserved.
    fn number_gender_ambiguous(&self, _int: u64, next_word: &str) -> bool {
        looks_like_group_tail(next_word) || next_word.chars().next().is_some_and(is_cjk_letter)
    }

    fn percent(&self, amount: &Amount, _locale: &str) -> Option<String> {
        Some(format!("{}パーセント", amount_words(amount)))
    }

    fn currency(
        &self,
        currency: Currency,
        int: u64,
        cents: Option<u32>,
        _locale: &str,
    ) -> Option<String> {
        if cents.is_some_and(|c| c > 0) {
            return None;
        }
        let name = match currency {
            Currency::Jpy => "円",
            Currency::Usd => "ドル",
            Currency::Eur => "ユーロ",
            Currency::Gbp => "ポンド",
            Currency::Brl | Currency::Pen => return None,
        };
        Some(format!("{}{name}", cardinal(int)))
    }

    fn date(&self, year: u32, month: u32, day: u32, _locale: &str) -> Option<String> {
        if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
            return None;
        }
        Some(format!(
            "{}年{}月{}日",
            cardinal(u64::from(year)),
            cardinal(u64::from(month)),
            cardinal(u64::from(day))
        ))
    }

    fn datetime(
        &self,
        value: &chrono::DateTime<chrono::FixedOffset>,
        fraction: &str,
        locale: &str,
    ) -> Option<String> {
        use chrono::Datelike;
        let number = |n: u64| cardinal(n);
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
                hour: "時",
                minute: "分",
                second: "秒",
                point: "点",
                plus: "プラス",
                minus: "マイナス",
                suffix: true,
            },
            number,
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
        let mut out = format!("{}時", cardinal(u64::from(hour)));
        if minute > 0 {
            out.push_str(&cardinal(u64::from(minute)));
            out.push('分');
        }
        Some(out)
    }

    fn unit(&self, amount: &Amount, unit: Unit, _locale: &str) -> Option<String> {
        let prefixed = matches!(unit, Unit::KilometersPerHour | Unit::MetersPerSecond);
        if prefixed && amount.negative {
            return None;
        }
        let words = amount_words(amount);
        Some(match unit {
            Unit::Kilogram => format!("{words}キログラム"),
            Unit::Gram => format!("{words}グラム"),
            Unit::Kilometer => format!("{words}キロメートル"),
            Unit::Meter => format!("{words}メートル"),
            Unit::Centimeter => format!("{words}センチメートル"),
            Unit::Millimeter => format!("{words}ミリメートル"),
            Unit::Liter => format!("{words}リットル"),
            Unit::Milliliter => format!("{words}ミリリットル"),
            Unit::Celsius => format!("{words}度"),
            Unit::Fahrenheit => format!("華氏{words}度"),
            Unit::KilometersPerHour => format!("時速{words}キロメートル"),
            Unit::MetersPerSecond => format!("秒速{words}メートル"),
            Unit::Gigabyte => format!("{words}ギガバイト"),
            Unit::Megahertz => format!("{words}メガヘルツ"),
            Unit::Millisecond => format!("{words}ミリ秒"),
        })
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
            (0, "零"),
            (1, "一"),
            (10, "十"),
            (11, "十一"),
            (20, "二十"),
            (100, "百"),
            (110, "百十"),
            (1000, "千"),
            (1001, "千一"),
            (2026, "二千二十六"),
            (10_000, "一万"),
            (10_001, "一万一"),
            (100_000, "十万"),
            (1_000_000, "百万"),
            (10_000_000, "一千万"),
            (12_000_000, "千二百万"),
            (12_345_678, "千二百三十四万五千六百七十八"),
            (100_000_000, "一億"),
            (1_000_000_000_000, "一兆"),
        ];
        for (n, expected) in cases {
            assert_eq!(cardinal(*n), *expected, "{n}");
        }
    }

    #[test]
    fn decimals_percent_currency() {
        let rules = Japanese;
        assert_eq!(
            rules.number(&amount(3, Some("14")), "ja-JP").unwrap(),
            "三点一四"
        );
        assert_eq!(
            rules.number(&amount(0, Some("5")), "ja-JP").unwrap(),
            "零点五"
        );
        assert_eq!(
            rules.percent(&amount(12, Some("5")), "ja-JP").unwrap(),
            "十二点五パーセント"
        );
        let negative = Amount {
            negative: true,
            int: 5,
            frac: None,
        };
        assert_eq!(rules.number(&negative, "ja-JP").unwrap(), "マイナス五");
        assert_eq!(
            rules.currency(Currency::Jpy, 500, None, "ja-JP").unwrap(),
            "五百円"
        );
        assert_eq!(
            rules.currency(Currency::Usd, 20, None, "ja-JP").unwrap(),
            "二十ドル"
        );
        assert_eq!(rules.currency(Currency::Usd, 12, Some(50), "ja-JP"), None);
        assert_eq!(rules.currency(Currency::Pen, 12, None, "ja-JP"), None);
    }

    #[test]
    fn dates_times_units() {
        let rules = Japanese;
        assert_eq!(
            rules.date(2026, 10, 26, "ja-JP").unwrap(),
            "二千二十六年十月二十六日"
        );
        assert_eq!(rules.time(15, 30, None, "ja-JP").unwrap(), "十五時三十分");
        assert_eq!(rules.time(9, 5, None, "ja-JP").unwrap(), "九時五分");
        assert_eq!(rules.time(15, 0, None, "ja-JP").unwrap(), "十五時");
        assert_eq!(rules.time(3, 0, Some(Meridiem::Am), "ja-JP"), None);
        assert_eq!(
            rules
                .unit(&amount(5, None), Unit::Kilometer, "ja-JP")
                .unwrap(),
            "五キロメートル"
        );
        assert_eq!(
            rules
                .unit(&amount(100, None), Unit::KilometersPerHour, "ja-JP")
                .unwrap(),
            "時速百キロメートル"
        );
        assert_eq!(
            rules
                .unit(&amount(25, None), Unit::Celsius, "ja-JP")
                .unwrap(),
            "二十五度"
        );
    }

    #[test]
    fn counter_ambiguity() {
        let rules = Japanese;
        assert!(rules.number_gender_ambiguous(3, "人"));
        assert!(rules.number_gender_ambiguous(3, "本"));
        assert!(rules.number_gender_ambiguous(500, "円"));
        assert!(rules.number_gender_ambiguous(3, "つ"));
        assert!(!rules.number_gender_ambiguous(3, "apples"));
    }
}
