//! Per-language rule sets and dispatch.
//!
//! Every language implements [`LanguageRules`]. All rendering methods have a
//! default implementation that returns `None` ("not supported, preserve the
//! original"), so a pass-through language only has to report coverage. A
//! language opts in to classification by returning [`Conventions`] for a
//! locale; without conventions the classifier does not run at all.
//!
//! Adding a language: implement the methods for the features it supports,
//! report honest coverage, add golden fixtures, and keep `conventions`
//! returning `None` until at least one feature is implemented and tested.

use std::collections::BTreeMap;

use super::CoverageLevel;

mod de;
mod en;
mod es;
mod fr;
mod it;
mod ja;
mod ko;
mod pt;
mod ru;
mod zh;

/// Normalization features, mapped to the stable coverage keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Feature {
    CardinalNumbers,
    Decimals,
    Ordinals,
    Percentages,
    Currency,
    Dates,
    Times,
    Units,
    Abbreviations,
    Ranges,
    Fractions,
}

impl Feature {
    pub(crate) const ALL: [Feature; 11] = [
        Feature::CardinalNumbers,
        Feature::Decimals,
        Feature::Ordinals,
        Feature::Percentages,
        Feature::Currency,
        Feature::Dates,
        Feature::Times,
        Feature::Units,
        Feature::Abbreviations,
        Feature::Ranges,
        Feature::Fractions,
    ];

    /// Stable key used in coverage maps.
    pub(crate) fn key(self) -> &'static str {
        match self {
            Feature::CardinalNumbers => "cardinal_numbers",
            Feature::Decimals => "decimals",
            Feature::Ordinals => "ordinals",
            Feature::Percentages => "percentages",
            Feature::Currency => "currency",
            Feature::Dates => "dates",
            Feature::Times => "times",
            Feature::Units => "units",
            Feature::Abbreviations => "abbreviations",
            Feature::Ranges => "ranges",
            Feature::Fractions => "fractions",
        }
    }
}

/// Day/month order of numeric slash dates in a locale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DateOrder {
    DayMonth,
    MonthDay,
}

/// Locale conventions used by the classifier.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Conventions {
    /// Decimal separator (`','` or `'.'`).
    pub decimal: char,
    /// Thousands grouping separator (`'.'` or `','`).
    pub group: char,
    /// `true` when the separators are read strictly by convention even with
    /// exactly three digits after a single separator: `1,234` is an integer
    /// and `3.140` a decimal (English). `false` preserves both shapes as
    /// ambiguous outside a currency context (Spanish).
    pub separators_unambiguous: bool,
    /// Order used for slash dates whose two readings are both valid, only
    /// when the locale is explicit.
    pub date_order: DateOrder,
    /// Currency a bare `$` denotes, when it is unambiguous for the locale.
    pub dollar: Option<Currency>,
}

/// Currencies with explicit symbols or ISO codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Currency {
    Pen,
    Eur,
    Gbp,
    Usd,
    Brl,
    Jpy,
}

impl Currency {
    /// `true` when the currency has a two-digit subunit.
    pub(crate) fn has_subunit(self) -> bool {
        !matches!(self, Currency::Jpy)
    }
}

/// Measurement units (case-sensitive written forms).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unit {
    Kilogram,
    Gram,
    Kilometer,
    Meter,
    Centimeter,
    Millimeter,
    Liter,
    Milliliter,
    Celsius,
    Fahrenheit,
    KilometersPerHour,
    MetersPerSecond,
    Gigabyte,
    Megahertz,
    Millisecond,
}

/// Written unit forms, longest first so suffix matching is greedy.
pub(crate) const UNIT_FORMS: &[(&str, Unit)] = &[
    ("km/h", Unit::KilometersPerHour),
    ("m/s", Unit::MetersPerSecond),
    ("MHz", Unit::Megahertz),
    ("°C", Unit::Celsius),
    ("°F", Unit::Fahrenheit),
    ("kg", Unit::Kilogram),
    ("km", Unit::Kilometer),
    ("cm", Unit::Centimeter),
    ("mm", Unit::Millimeter),
    ("mL", Unit::Milliliter),
    ("GB", Unit::Gigabyte),
    ("ms", Unit::Millisecond),
    ("g", Unit::Gram),
    ("m", Unit::Meter),
    ("L", Unit::Liter),
];

/// Exact unit lookup.
pub(crate) fn unit_from_str(raw: &str) -> Option<Unit> {
    UNIT_FORMS
        .iter()
        .find(|(form, _)| *form == raw)
        .map(|(_, unit)| *unit)
}

/// A parsed numeric amount. `int` is exact; `frac` holds the fractional
/// digits verbatim (trailing zeros are significant).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Amount {
    pub negative: bool,
    pub int: u64,
    pub frac: Option<String>,
}

/// Localized labels for the explicit components of an RFC 3339 timestamp.
pub(crate) struct TimestampLabels {
    pub hour: &'static str,
    pub minute: &'static str,
    pub second: &'static str,
    pub point: &'static str,
    pub plus: &'static str,
    pub minus: &'static str,
    pub suffix: bool,
}

/// Render the clock and offset as labelled quantities, preserving every
/// fractional digit. The labels and number renderer come from language rules.
pub(crate) fn timestamp_parts(
    date: String,
    value: &chrono::DateTime<chrono::FixedOffset>,
    fraction: &str,
    labels: &TimestampLabels,
    number: impl Fn(u64) -> String,
) -> String {
    use chrono::Timelike;
    let component = |label: &str, value: u64| {
        if labels.suffix {
            format!("{}{label}", number(value))
        } else {
            format!("{label}: {}", number(value))
        }
    };
    let mut out = format!(
        "{date}, {}, {}, ",
        component(labels.hour, value.hour().into()),
        component(labels.minute, value.minute().into())
    );
    if !labels.suffix {
        out.push_str(labels.second);
        out.push_str(": ");
    }
    out.push_str(&number(value.second().into()));
    if !fraction.is_empty() {
        out.push(' ');
        out.push_str(labels.point);
        for digit in fraction.bytes() {
            out.push(' ');
            out.push_str(&number(u64::from(digit - b'0')));
        }
    }
    if labels.suffix {
        out.push_str(labels.second);
    }
    out.push_str(", UTC");
    let offset = value.offset().local_minus_utc();
    if offset != 0 {
        let minutes = offset.unsigned_abs() / 60;
        out.push(' ');
        out.push_str(if offset < 0 {
            labels.minus
        } else {
            labels.plus
        });
        out.push(' ');
        out.push_str(&component(labels.hour, (minutes / 60).into()));
        if !minutes.is_multiple_of(60) {
            out.push_str(", ");
            out.push_str(&component(labels.minute, (minutes % 60).into()));
        }
    }
    out
}

impl Amount {
    /// `true` for exactly one (singular agreement).
    pub(crate) fn is_one(&self) -> bool {
        self.int == 1 && self.frac.is_none()
    }
}

/// Written ordinal marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OrdinalMarker<'a> {
    /// English letter suffix (`st`, `nd`, `rd`, `th`), verbatim.
    Suffix(&'a str),
    /// Spanish/Romance masculine indicator `º`.
    Masculine,
    /// Spanish/Romance feminine indicator `ª`.
    Feminine,
}

/// 12-hour clock marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Meridiem {
    Am,
    Pm,
}

/// Context an abbreviation requires before it is expanded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AbbrevContext {
    /// Followed by a capitalized word (titles: `Dr. García`).
    CapitalizedWord,
    /// Followed by a capitalized word or a number (`Av. Arequipa`, `Av. 28`).
    CapitalizedWordOrNumber,
    /// Not at a sentence end (`etc.` followed by a lowercase word, a digit
    /// or `, ; : )`).
    NotSentenceEnd,
}

/// Language rules. Every renderer returns `None` when the form is not
/// supported, which preserves the original text.
pub(crate) trait LanguageRules: Sync {
    /// Coverage level of one feature.
    fn coverage(&self, feature: Feature) -> CoverageLevel;

    /// Conventions for `locale`, or `None` to disable classification.
    fn conventions(&self, _locale: &str) -> Option<Conventions> {
        None
    }

    /// Standalone cardinal or decimal reading (`-3,5` -> "menos tres coma
    /// cinco").
    fn number(&self, _amount: &Amount, _locale: &str) -> Option<String> {
        None
    }

    /// Digit-by-digit reading of an integer's ASCII digits (`105` ->
    /// "uno cero cinco"). `None` when the language has no digit renderer.
    fn digits(&self, _digits: &str, _locale: &str) -> Option<String> {
        None
    }

    /// `true` when reading `int` before `next_word` needs a grammatical
    /// gender the rules cannot know (Spanish `21 casas`). The number is then
    /// preserved with `ambiguous_number`.
    fn number_gender_ambiguous(&self, _int: u64, _next_word: &str) -> bool {
        false
    }

    /// `true` when `next_word` makes a preceding number a high-confidence
    /// count (`007 errores`, `10.005 solicitudes`). This is only used to
    /// resolve numeric shapes that are otherwise preserved; it never
    /// overrides identifier protection. Languages where a following word
    /// does not prove a count keep the default.
    fn quantity_context(&self, _next_word: &str) -> bool {
        false
    }

    /// Ordinal reading. `next_word` enables apocope (`1º piso` -> "primer").
    fn ordinal(
        &self,
        _n: u64,
        _marker: OrdinalMarker<'_>,
        _next_word: Option<&str>,
        _locale: &str,
    ) -> Option<String> {
        None
    }

    /// Percentage reading.
    fn percent(&self, _amount: &Amount, _locale: &str) -> Option<String> {
        None
    }

    /// Money amount; `cents` is the two-digit subunit when present.
    fn currency(
        &self,
        _currency: Currency,
        _int: u64,
        _cents: Option<u32>,
        _locale: &str,
    ) -> Option<String> {
        None
    }

    /// Validated Gregorian date.
    fn date(&self, _year: u32, _month: u32, _day: u32, _locale: &str) -> Option<String> {
        None
    }

    /// Validated clock time.
    fn time(
        &self,
        _hour: u32,
        _minute: u32,
        _meridiem: Option<Meridiem>,
        _locale: &str,
    ) -> Option<String> {
        None
    }

    /// Clock time rendered for a reading style. The default keeps the
    /// original `time` wording for `precise` and `natural`; languages that
    /// implement natural times override it.
    fn time_styled(
        &self,
        hour: u32,
        minute: u32,
        meridiem: Option<Meridiem>,
        style: super::ReadingStyle,
        locale: &str,
    ) -> Option<String> {
        match style {
            super::ReadingStyle::Precise | super::ReadingStyle::Natural => {
                self.time(hour, minute, meridiem, locale)
            }
            super::ReadingStyle::Digits => None,
        }
    }

    /// Simple fraction `numerator/denominator` (natural style only).
    fn fraction(&self, _numerator: u64, _denominator: u64, _locale: &str) -> Option<String> {
        None
    }

    /// Scientific notation: `mantissa * 10^exponent`.
    fn scientific(&self, _mantissa: &Amount, _exponent: i32, _locale: &str) -> Option<String> {
        None
    }

    /// Attached duration components in natural style (`1h30min`).
    fn duration(&self, _hours: u64, _minutes: u64, _seconds: u64, _locale: &str) -> Option<String> {
        None
    }

    /// RFC 3339 timestamp with its original fractional digits and offset.
    fn datetime(
        &self,
        _value: &chrono::DateTime<chrono::FixedOffset>,
        _fraction: &str,
        _locale: &str,
    ) -> Option<String> {
        None
    }

    /// Quantity with unit.
    fn unit(&self, _amount: &Amount, _unit: Unit, _locale: &str) -> Option<String> {
        None
    }

    /// Integer range `a–b`, optionally followed by a unit.
    fn range(&self, _from: u64, _to: u64, _unit: Option<Unit>, _locale: &str) -> Option<String> {
        None
    }

    /// Abbreviation expansion for a word written before a period (`"Dr"`).
    fn abbreviation(&self, _word: &str) -> Option<(&'static str, AbbrevContext)> {
        None
    }
}

/// Rules for a language with no implementation (well-formed but unknown).
struct PassThrough;

impl LanguageRules for PassThrough {
    fn coverage(&self, _feature: Feature) -> CoverageLevel {
        CoverageLevel::None
    }
}

/// Rule set for a canonical lowercase language code.
pub(crate) fn for_language(language: &str) -> &'static dyn LanguageRules {
    match language {
        "es" => &es::Spanish,
        "en" => &en::English,
        "pt" => &pt::Portuguese,
        "fr" => &fr::French,
        "de" => &de::German,
        "it" => &it::Italian,
        "ru" => &ru::Russian,
        "zh" => &zh::Chinese,
        "ja" => &ja::Japanese,
        "ko" => &ko::Korean,
        _ => &PassThrough,
    }
}

/// Coverage map with every stable feature key.
pub(crate) fn coverage_map(rules: &dyn LanguageRules) -> BTreeMap<String, CoverageLevel> {
    Feature::ALL
        .iter()
        .map(|feature| (feature.key().to_string(), rules.coverage(*feature)))
        .collect()
}

/// Overall level from per-feature levels.
pub(crate) fn overall(levels: impl Iterator<Item = CoverageLevel>) -> CoverageLevel {
    let mut any_full_or_partial = false;
    let mut all_full = true;
    for level in levels {
        match level {
            CoverageLevel::Full => any_full_or_partial = true,
            CoverageLevel::Partial => {
                any_full_or_partial = true;
                all_full = false;
            }
            CoverageLevel::None => all_full = false,
        }
    }
    if !any_full_or_partial {
        CoverageLevel::None
    } else if all_full {
        CoverageLevel::Full
    } else {
        CoverageLevel::Partial
    }
}

/// Days in a Gregorian month, `None` for an invalid month.
pub(crate) fn days_in_month(year: u32, month: u32) -> Option<u32> {
    let leap = (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400);
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 => Some(if leap { 29 } else { 28 }),
        _ => None,
    }
}

/// `true` for a valid Gregorian date with a four-digit year.
pub(crate) fn valid_date(year: u32, month: u32, day: u32) -> bool {
    (1000..=9999).contains(&year)
        && day >= 1
        && days_in_month(year, month).is_some_and(|max| day <= max)
}
