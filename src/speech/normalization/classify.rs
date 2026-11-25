//! Classification of fields into spoken-form changes and warnings.
//!
//! Precedence, per the spec's classification policy:
//! 1. protected regions and identifier-shaped cores (never touched);
//! 2. phone-like runs of space-separated digit groups (protected);
//! 3. contextual explicit types: currency, percentages, ordinals, dates,
//!    times, ranges, fractions/ratios (warned), units;
//! 4. plain cardinals/decimals, only when the locale parse is unambiguous.
//!
//! Every matcher requires the *whole* core to match. Anything else is left
//! byte-for-byte as written. Ambiguous shapes are preserved and reported.

use super::lexer::{Field, LexicalSource, is_protected_identifier};
use super::rules::{
    Amount, Conventions, Currency, LanguageRules, Meridiem, OrdinalMarker, UNIT_FORMS, Unit,
    unit_from_str, valid_date,
};
use super::{
    AbbreviationPolicy, ChangeKind, Confidence, NormalizationWarning, NormalizeOptions,
    ReadingStyle, TextChange, WarningCode,
};
use chrono::Datelike;

/// Largest integer read as a number (15 digits).
pub(crate) const MAX_INTEGER: u64 = 999_999_999_999_999;
/// Maximum fractional digits read.
pub(crate) const MAX_FRACTION_DIGITS: usize = 12;

/// Result of parsing a numeric core.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NumParse {
    Value(Amount),
    /// Number-shaped, but the locale reading is not provable.
    Ambiguous,
    /// Valid shape beyond [`MAX_INTEGER`] or [`MAX_FRACTION_DIGITS`].
    TooLarge,
    /// Not a plain number (identifier, version, IP, ...).
    NotNumber,
}

/// How much surrounding context may resolve an otherwise ambiguous number.
///
/// `Plain` is the conservative default: a leading-zero sequence or a single
/// ambiguous separator is preserved. `Currency` and `Quantity` only resolve
/// shapes that are provable in those contexts; they never reinterpret an
/// identifier-shaped core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NumberContext {
    /// Standalone prose; ambiguous shapes are preserved.
    Plain,
    /// A currency amount (a three-digit group is a thousands group).
    Currency,
    /// A high-confidence count before a noun (`007 errores`,
    /// `10.005 solicitudes`).
    Quantity,
}

fn strip_sign(raw: &str) -> (bool, &str) {
    if let Some(rest) = raw.strip_prefix('-') {
        (true, rest)
    } else if let Some(rest) = raw.strip_prefix('\u{2212}') {
        (true, rest)
    } else {
        (false, raw)
    }
}

/// Convert the shared exact parser's value to this API's bounded legacy amount.
/// Separator, grouping and decimal-field extraction live in `speech::core`;
/// this adapter retains legacy padding/context and 15/12-digit budgets.
fn shared_amount(negative: bool, raw: &str, decimal: char, canonical_count: bool) -> NumParse {
    let mut options = crate::speech::SpeechOptions::new(crate::speech::SpeechLocale::En);
    options.decimal_separator = Some(decimal);
    let Ok(mut value) = crate::speech::core::parse_number(raw, &options) else {
        return NumParse::NotNumber;
    };
    if canonical_count {
        value = crate::speech::core::canonical_count(&value);
    }
    if value.integer.len() > 15
        || value
            .fraction
            .as_ref()
            .is_some_and(|fraction| fraction.len() > MAX_FRACTION_DIGITS)
    {
        return NumParse::TooLarge;
    }
    let Ok(int) = value.integer.parse::<u64>() else {
        return NumParse::TooLarge;
    };
    NumParse::Value(Amount {
        negative,
        int,
        frac: value.fraction,
    })
}

fn shared_shape(raw: &str, decimal: char) -> bool {
    let mut options = crate::speech::SpeechOptions::new(crate::speech::SpeechLocale::En);
    options.decimal_separator = Some(decimal);
    crate::speech::core::parse_number(raw, &options).is_ok()
}

/// Parse a plain number written with the locale's separators.
///
/// `currency_ctx`: a single separator followed by exactly three digits is a
/// thousands group when it is the locale's group separator (a currency
/// subunit has two digits). Kept as a compatibility wrapper for callers and
/// tests that only distinguish currency from plain prose.
pub(crate) fn parse_number(raw: &str, conv: &Conventions, currency_ctx: bool) -> NumParse {
    let context = if currency_ctx {
        NumberContext::Currency
    } else {
        NumberContext::Plain
    };
    parse_number_in(raw, conv, context)
}

/// Parse a number in `context`. Only `Quantity` may discard leading zeros
/// from an integer or accept the locale's group separator without a currency
/// marker; both decisions are made before any rendering.
pub(crate) fn parse_number_in(raw: &str, conv: &Conventions, context: NumberContext) -> NumParse {
    let currency_ctx = context == NumberContext::Currency;
    let quantity_ctx = context == NumberContext::Quantity;
    let (negative, body) = strip_sign(raw);
    let bytes = body.as_bytes();
    if bytes.is_empty()
        || !bytes
            .iter()
            .all(|b| b.is_ascii_digit() || matches!(b, b'.' | b','))
        || !bytes[0].is_ascii_digit()
        || !bytes[bytes.len() - 1].is_ascii_digit()
    {
        return NumParse::NotNumber;
    }
    let separators: Vec<char> = body.chars().filter(|c| matches!(c, '.' | ',')).collect();
    if separators.len() > 8 {
        return NumParse::NotNumber;
    }
    let groups: Vec<&str> = body.split(['.', ',']).collect();
    if groups.iter().any(|g| g.is_empty()) {
        return NumParse::NotNumber;
    }
    // These are interpretation policies. The shared parser alone owns valid
    // grouped-digit shapes and extraction of exact integer/fraction strings.
    match separators.len() {
        0 => {
            if body.len() > 1 && body.starts_with('0') && !quantity_ctx {
                return NumParse::Ambiguous;
            }
            shared_amount(negative, body, conv.decimal, quantity_ctx)
        }
        1 => {
            let (first, tail, sep) = (groups[0], groups[1], separators[0]);
            if (1..=3).contains(&first.len()) && !first.starts_with('0') && tail.len() == 3 {
                if sep == conv.group
                    && (conv.separators_unambiguous || currency_ctx || quantity_ctx)
                {
                    return shared_amount(negative, body, conv.decimal, false);
                }
                if sep == conv.decimal && conv.separators_unambiguous && !currency_ctx {
                    return shared_amount(negative, body, conv.decimal, false);
                }
                return NumParse::Ambiguous;
            }
            if (sep == conv.decimal || first == "0" || tail.len() >= 4)
                && (first == "0" || !first.starts_with('0'))
            {
                return shared_amount(negative, body, sep, false);
            }
            NumParse::Ambiguous
        }
        count => {
            // Legacy grouped quantities require a nonzero initial group;
            // the shared parser intentionally also represents padded fields.
            if groups[0].starts_with('0') {
                return NumParse::NotNumber;
            }
            let last = separators[count - 1];
            let head = &separators[..count - 1];
            if separators.iter().all(|c| *c == conv.group)
                || last == conv.decimal && head.iter().all(|c| *c == conv.group)
            {
                return shared_amount(negative, body, conv.decimal, false);
            }
            if (separators.iter().all(|c| *c == conv.decimal)
                || last == conv.group && head.iter().all(|c| *c == conv.decimal))
                && shared_shape(body, conv.group)
            {
                return NumParse::Ambiguous;
            }
            NumParse::NotNumber
        }
    }
}

/// Adapt shared date semantics to the versioned API's existing warning policy.
/// Unambiguous day/month order wins before an explicit locale default; the
/// versioned four-digit-year profile remains restricted to years 1000–9999.
pub(crate) fn shared_date_fields(
    raw: &str,
    conv: &Conventions,
    locale_explicit: bool,
) -> Result<(u32, u32, u32), WarningCode> {
    use crate::speech::temporal::{TemporalValue, parse_numeric_date};
    use crate::speech::{DateOrder as Order, SpeechLocale, SpeechOptions};
    let mut options = SpeechOptions::new(SpeechLocale::En);
    let extract = |value: Option<TemporalValue>| match value {
        Some(TemporalValue::Date {
            year: Some(year),
            month: Some(month),
            day: Some(day),
            ..
        }) => year
            .parse::<u32>()
            .ok()
            .filter(|year| *year >= 1000)
            .map(|year| (year, month, day)),
        _ => None,
    };
    if let Some(fields) = extract(parse_numeric_date(raw, &options)) {
        return Ok(fields);
    }
    let year_width = raw
        .split(['/', '-'])
        .find(|part| part.len() == 4)
        .and_then(|s| s.parse::<u32>().ok());
    if year_width.is_some_and(|year| year < 1000) {
        return Err(WarningCode::InvalidDate);
    }
    // A strict DMY probe identifies a genuinely ambiguous shape after the
    // shared RejectAmbiguous pass failed. There is no second calendar parser.
    options.date_order = Order::DayMonthYear;
    if extract(parse_numeric_date(raw, &options)).is_none() {
        return Err(WarningCode::InvalidDate);
    }
    if !locale_explicit {
        return Err(WarningCode::AmbiguousDate);
    }
    options.date_order = conv.date_order;
    extract(parse_numeric_date(raw, &options)).ok_or(WarningCode::InvalidDate)
}

/// Parse `mantissa e exponent` / `mantissa E exponent` (scientific notation).
///
/// Scientific mantissas are unambiguous, so a single dot or comma is a
/// decimal separator even when it is not the locale convention. Returns
/// `None` for malformed shapes or an exponent with more than two digits
/// (preserved as written).
pub(crate) fn parse_scientific(raw: &str) -> Option<(Amount, i32)> {
    use crate::speech::shapes::{self, ShapePolicy};
    let options = crate::speech::SpeechOptions::new(crate::speech::SpeechLocale::En);
    let shape = shapes::scientific(raw, 0, &options, ShapePolicy::Versioned)?;
    if shape.end != raw.len() {
        return None;
    }
    Some((
        Amount {
            negative: shape.mantissa.negative,
            int: shape.mantissa.integer.parse().ok()?,
            frac: shape.mantissa.fraction,
        },
        shape.exponent,
    ))
}

/// Parse attached durations: `1h`, `1h30`, `1h30min`, `1h30min15s`, `30min`,
/// `45s`. The bare number after `h` is minutes. Returns
/// `(hours, minutes, seconds)`; at least one component is present.
pub(crate) fn parse_duration(raw: &str) -> Option<(u64, u64, u64)> {
    use crate::speech::shapes::{self, DurationUnit, ShapePolicy};
    let shape = shapes::duration(raw, 0, ShapePolicy::Versioned)?;
    if shape.end != raw.len() {
        return None;
    }
    let (mut hours, mut minutes, mut seconds) = (0, 0, 0);
    for component in shape.components {
        match component.unit {
            DurationUnit::Hours => hours = component.value,
            DurationUnit::Minutes => minutes = component.value,
            DurationUnit::Seconds => seconds = component.value,
        }
    }
    Some((hours, minutes, seconds))
}

fn all_digits(raw: &str) -> bool {
    !raw.is_empty() && raw.bytes().all(|b| b.is_ascii_digit())
}

fn plain_integer(raw: &str) -> Option<u64> {
    if !all_digits(raw) || raw.len() > 15 || (raw.len() > 1 && raw.starts_with('0')) {
        return None;
    }
    raw.parse().ok().filter(|n| *n <= MAX_INTEGER)
}

fn is_vulgar_fraction(c: char) -> bool {
    matches!(c, '\u{00BC}'..='\u{00BE}' | '\u{2150}'..='\u{215F}' | '\u{2189}')
}

/// Written currency marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Symbol {
    Known(Currency),
    /// `$`: US dollars only where the locale says so.
    Dollar,
    /// Symbols shared by several currencies (`¥`).
    Ambiguous,
}

fn symbol(raw: &str) -> Option<Symbol> {
    Some(match raw {
        "S/" | "S/." | "PEN" => Symbol::Known(Currency::Pen),
        "€" | "EUR" => Symbol::Known(Currency::Eur),
        "£" | "GBP" => Symbol::Known(Currency::Gbp),
        "US$" | "USD" => Symbol::Known(Currency::Usd),
        "R$" | "BRL" => Symbol::Known(Currency::Brl),
        "JPY" => Symbol::Known(Currency::Jpy),
        "$" => Symbol::Dollar,
        "¥" => Symbol::Ambiguous,
        _ => return None,
    })
}

/// Prefix symbols attached to the amount, longest first.
const ATTACHED_PREFIXES: &[&str] = &["S/.", "S/", "US$", "R$", "$", "€", "£", "¥"];
/// Suffix symbols attached to the amount.
const ATTACHED_SUFFIXES: &[&str] = &["€", "£", "$", "¥"];

/// Words after an amount that make a currency reading unsafe
/// (`$1.5 million`).
const SCALE_WORDS: &[&str] = &[
    "thousand",
    "million",
    "millions",
    "billion",
    "billions",
    "trillion",
    "mil",
    "millón",
    "millones",
    "millardo",
    "millardos",
    "billón",
    "billones",
];

pub(crate) const MERIDIEMS: &[(&str, Meridiem)] = &[
    ("a.m", Meridiem::Am),
    ("p.m", Meridiem::Pm),
    ("A.M", Meridiem::Am),
    ("P.M", Meridiem::Pm),
    ("am", Meridiem::Am),
    ("pm", Meridiem::Pm),
    ("AM", Meridiem::Am),
    ("PM", Meridiem::Pm),
];

struct Classifier<'a> {
    text: &'a str,
    rules: &'a dyn LanguageRules,
    conv: Conventions,
    locale: &'a str,
    language: &'a str,
    abbreviation_policy: AbbreviationPolicy,
    locale_explicit: bool,
    style: ReadingStyle,
    fields: Vec<Field>,
    changes: Vec<TextChange>,
    warnings: Vec<NormalizationWarning>,
}

/// Classify the shared lexical source for the options' language and locale.
pub(crate) fn classify(
    source: &LexicalSource<'_>,
    rules: &dyn LanguageRules,
    opts: &NormalizeOptions,
) -> (Vec<TextChange>, Vec<NormalizationWarning>) {
    let Some(conv) = rules.conventions(&opts.locale) else {
        return (Vec::new(), Vec::new());
    };
    let mut classifier = Classifier {
        text: source.text(),
        rules,
        conv,
        locale: &opts.locale,
        language: &opts.language,
        abbreviation_policy: opts.abbreviation_policy,
        locale_explicit: opts.locale_explicit,
        style: opts.reading_style,
        fields: source.fields(),
        changes: Vec::new(),
        warnings: Vec::new(),
    };
    let mut index = 0;
    while index < classifier.fields.len() {
        let next = classifier.field(index);
        index = next.max(index + 1);
    }
    (classifier.changes, classifier.warnings)
}

impl<'a> Classifier<'a> {
    fn core(&self, index: usize) -> &'a str {
        let field = self.fields[index];
        &self.text[field.core_start..field.core_end]
    }

    fn trail(&self, index: usize) -> &'a str {
        let field = self.fields[index];
        &self.text[field.core_end..field.end]
    }

    /// Next field joined to `index` by a single (possibly non-breaking)
    /// space with no punctuation in between.
    fn joinable(&self, index: usize) -> Option<usize> {
        let current = self.fields[index];
        let next_index = index + 1;
        let next = self.fields.get(next_index)?;
        let gap = &self.text[current.end..next.start];
        (current.bare_end()
            && next.bare_start()
            && !next.protected
            && next.has_core()
            && matches!(gap, " " | "\u{a0}" | "\u{202f}"))
        .then_some(next_index)
    }

    /// Word following `index` on the same line with no punctuation between.
    fn next_word(&self, index: usize) -> Option<&'a str> {
        let current = self.fields[index];
        let next = self.fields.get(index + 1)?;
        let gap = &self.text[current.end..next.start];
        (current.bare_end() && next.bare_start() && next.has_core() && !gap.contains('\n'))
            .then(|| self.core(index + 1))
    }

    /// Whether a period at byte `dot` (inside field `index`) continues the
    /// sentence: followed by `, ; : )`, or by a lowercase word or digit on
    /// the same line.
    fn period_continues(&self, index: usize, dot: usize) -> bool {
        crate::speech::abbreviations::compatibility::period_continues(
            self.text,
            &self.fields,
            index,
            dot,
        )
    }

    fn change(&mut self, kind: ChangeKind, start: usize, end: usize, replacement: String) {
        let confidence = match kind {
            ChangeKind::Time | ChangeKind::Abbreviation => Confidence::Medium,
            _ => Confidence::High,
        };
        self.changes.push(TextChange {
            kind,
            start,
            end,
            replacement,
            confidence,
        });
    }

    fn warn(&mut self, code: WarningCode, start: usize, end: usize) {
        self.warnings
            .push(NormalizationWarning { code, start, end });
    }

    /// Classify field `index`; returns the index of the next unconsumed
    /// field.
    fn field(&mut self, index: usize) -> usize {
        let field = self.fields[index];
        if field.protected || !field.has_core() {
            return index + 1;
        }
        let core = self.core(index);
        if let Some(next) = self.abbreviation(index) {
            return next;
        }
        if let Some(next) = self.currency_symbol_field(index) {
            return next;
        }
        if core.chars().any(is_vulgar_fraction) {
            let (_, body) = strip_sign(core);
            if body
                .chars()
                .all(|c| c.is_ascii_digit() || is_vulgar_fraction(c))
            {
                self.warn(
                    WarningCode::AmbiguousFraction,
                    field.core_start,
                    field.core_end,
                );
            }
            return index + 1;
        }
        if !core.bytes().any(|b| b.is_ascii_digit()) || is_protected_identifier(core) {
            return index + 1;
        }
        if let Some(next) = self.digit_run(index) {
            return next;
        }
        self.numeric(index)
    }

    fn abbreviation(&mut self, index: usize) -> Option<usize> {
        if self.abbreviation_policy == AbbreviationPolicy::Preserve {
            return None;
        }
        let (end, expansion) = crate::speech::abbreviations::compatibility::legacy_match(
            self.text,
            &self.fields,
            index,
            self.language,
        )?;
        self.change(
            ChangeKind::Abbreviation,
            self.fields[index].core_start,
            end,
            expansion.to_string(),
        );
        Some(index + 1)
    }

    /// Phone-like runs: two or more space-separated pure digit groups
    /// (`987 654 321`, `+51 987 654 321`) are protected.
    fn digit_run(&mut self, index: usize) -> Option<usize> {
        let core = self.core(index);
        let digits = core.strip_prefix('+').unwrap_or(core);
        if !all_digits(digits) {
            return None;
        }
        let mut last = index;
        while let Some(next) = self.joinable(last) {
            if !all_digits(self.core(next)) {
                break;
            }
            last = next;
        }
        (last > index).then_some(last + 1)
    }

    /// Standalone symbol field followed by an amount (`S/ 150,50`,
    /// `USD 12`).
    fn currency_symbol_field(&mut self, index: usize) -> Option<usize> {
        let field = self.fields[index];
        let core = self.core(index);
        let trail = self.trail(index);
        let (sym, joined_ok) = if core == "S/" && trail == "." {
            (symbol("S/."), true)
        } else {
            (symbol(core), field.bare_end())
        };
        let sym = sym?;
        if !joined_ok {
            return None;
        }
        let next_index = index + 1;
        let next = *self.fields.get(next_index)?;
        let gap = &self.text[field.end..next.start];
        if !matches!(gap, " " | "\u{a0}" | "\u{202f}") || !next.bare_start() || next.protected {
            return None;
        }
        let amount_raw = self.core(next_index);
        self.money(sym, amount_raw, field.core_start, next.core_end, next_index)
    }

    /// Shared currency handling. Returns `None` when `amount_raw` is not
    /// number-shaped (no match), otherwise consumes through `last_index`.
    fn money(
        &mut self,
        sym: Symbol,
        amount_raw: &str,
        start: usize,
        end: usize,
        last_index: usize,
    ) -> Option<usize> {
        let parsed = parse_number(amount_raw, &self.conv, true);
        if parsed == NumParse::NotNumber {
            return None;
        }
        let consumed = Some(last_index + 1);
        let currency = match sym {
            Symbol::Known(currency) => currency,
            Symbol::Dollar => match self.conv.dollar {
                Some(currency) => currency,
                None => {
                    self.warn(WarningCode::AmbiguousCurrency, start, end);
                    return consumed;
                }
            },
            Symbol::Ambiguous => {
                self.warn(WarningCode::AmbiguousCurrency, start, end);
                return consumed;
            }
        };
        let amount = match parsed {
            NumParse::Value(amount) => amount,
            NumParse::TooLarge => {
                self.warn(WarningCode::NumberTooLarge, start, end);
                return consumed;
            }
            _ => {
                self.warn(WarningCode::AmbiguousNumber, start, end);
                return consumed;
            }
        };
        let followed_by_scale = self
            .next_word(last_index)
            .is_some_and(|w| SCALE_WORDS.contains(&w.to_lowercase().as_str()));
        if followed_by_scale {
            self.warn(WarningCode::AmbiguousCurrency, start, end);
            return consumed;
        }
        let cents = match &amount.frac {
            None => None,
            Some(frac) if frac.len() == 2 && currency.has_subunit() => frac.parse::<u32>().ok(),
            Some(_) => {
                self.warn(WarningCode::AmbiguousNumber, start, end);
                return consumed;
            }
        };
        if amount.negative {
            return consumed;
        }
        if let Some(spoken) = self
            .rules
            .currency(currency, amount.int, cents, self.locale)
        {
            self.change(ChangeKind::Currency, start, end, spoken);
        }
        consumed
    }

    fn numeric(&mut self, index: usize) -> usize {
        let field = self.fields[index];
        let core = self.core(index);
        let (start, end) = (field.core_start, field.core_end);
        let done = index + 1;

        // A timestamp is one span: never partially rewrite its date or offset.
        // Typed locales consume the shared timestamp IR through the compatibility
        // adapter below. Italian remains on this legacy projection until it has
        // a typed locale implementation.
        if self.language == "it"
            && core.len() >= 11
            && core.as_bytes().get(4) == Some(&b'-')
            && core
                .as_bytes()
                .get(10)
                .is_some_and(|b| *b == b'T' || *b == b't')
        {
            let fraction = core
                .get(19..)
                .and_then(|s| s.strip_prefix('.'))
                .map(|s| s.split(|c: char| !c.is_ascii_digit()).next().unwrap_or(""))
                .unwrap_or("");
            let shared_valid = matches!(
                crate::speech::temporal::parse_timestamp_at(core, 0),
                Some((end, crate::speech::temporal::TemporalValue::Timestamp { .. }))
                    if end == core.len()
            );
            match chrono::DateTime::parse_from_rfc3339(core) {
                Ok(value)
                    if shared_valid
                        && valid_date(
                            value.date_naive().year() as u32,
                            value.date_naive().month(),
                            value.date_naive().day(),
                        ) =>
                {
                    if let Some(spoken) = self.rules.datetime(&value, fraction, self.locale) {
                        self.change(ChangeKind::Date, start, end, spoken);
                    } else {
                        self.warn(WarningCode::AmbiguousTime, start, end);
                    }
                }
                _ => self.warn(WarningCode::AmbiguousTime, start, end),
            }
            return done;
        }

        // Scientific notation and attached durations use the shared shape
        // grammar. Typed locales render them through the compatibility adapter
        // in shared.rs; Italian remains on this legacy projection until it has
        // a typed locale implementation.
        if self.language == "it" {
            if let Some((mantissa, exponent)) = parse_scientific(core) {
                if let Some(spoken) = self.rules.scientific(&mantissa, exponent, self.locale) {
                    self.change(ChangeKind::Decimal, start, end, spoken);
                }
                return done;
            }
            if self.style == ReadingStyle::Natural
                && let Some((hours, minutes, seconds)) = parse_duration(core)
                && [hours, minutes, seconds]
                    .iter()
                    .all(|value| *value <= MAX_INTEGER)
                && let Some(spoken) = self.rules.duration(hours, minutes, seconds, self.locale)
            {
                self.change(ChangeKind::Unit, start, end, spoken);
                return done;
            }
        }

        // Attached currency prefix / suffix.
        for prefix in ATTACHED_PREFIXES {
            if let Some(rest) = core.strip_prefix(prefix)
                && rest.starts_with(|c: char| c.is_ascii_digit())
                && let Some(sym) = symbol(prefix)
            {
                return self.money(sym, rest, start, end, index).unwrap_or(done);
            }
        }
        for suffix in ATTACHED_SUFFIXES {
            if let Some(rest) = core.strip_suffix(suffix)
                && rest.ends_with(|c: char| c.is_ascii_digit())
                && let Some(sym) = symbol(suffix)
            {
                return self.money(sym, rest, start, end, index).unwrap_or(done);
            }
        }
        if let Some(rest) = core.strip_suffix('%') {
            self.percent(rest, start, end);
            return done;
        }
        if self.language == "it"
            && let Some(next) = self.ordinal(index)
        {
            return next;
        }
        for (form, unit) in UNIT_FORMS {
            if let Some(rest) = core.strip_suffix(form)
                && rest.ends_with(|c: char| c.is_ascii_digit())
            {
                self.quantity(rest, *unit, start, end);
                return done;
            }
        }
        if core.contains('/') {
            if self.language == "it" {
                self.slash(core, start, end);
            }
            return done;
        }
        if core.contains(':') {
            if self.language == "it" {
                return self.clock(index);
            }
            return done;
        }
        if core.contains('–') {
            if self.language == "it" {
                return self.range(index);
            }
            return done;
        }
        if core.matches('-').count() == 2 && !core.starts_with('-') {
            if self.language == "it" {
                self.iso_date(core, start, end);
            }
            return done;
        }
        self.plain_number(index)
    }

    fn percent(&mut self, raw: &str, start: usize, end: usize) {
        match parse_number(raw, &self.conv, false) {
            NumParse::Value(amount) => {
                if let Some(spoken) = self.rules.percent(&amount, self.locale) {
                    self.change(ChangeKind::Percentage, start, end, spoken);
                }
            }
            NumParse::Ambiguous => self.warn(WarningCode::AmbiguousNumber, start, end),
            NumParse::TooLarge => self.warn(WarningCode::NumberTooLarge, start, end),
            NumParse::NotNumber => {}
        }
    }

    fn quantity(&mut self, raw: &str, unit: Unit, start: usize, end: usize) {
        match parse_number(raw, &self.conv, false) {
            NumParse::Value(amount) => {
                if let Some(spoken) = self.rules.unit(&amount, unit, self.locale) {
                    self.change(ChangeKind::Unit, start, end, spoken);
                }
            }
            NumParse::Ambiguous => self.warn(WarningCode::AmbiguousNumber, start, end),
            NumParse::TooLarge => self.warn(WarningCode::NumberTooLarge, start, end),
            NumParse::NotNumber => {}
        }
    }

    fn ordinal(&mut self, index: usize) -> Option<usize> {
        let field = self.fields[index];
        let core = self.core(index);
        let digit_end = core.find(|c: char| !c.is_ascii_digit())?;
        let (digits, suffix) = core.split_at(digit_end);
        let marker = match suffix {
            "st" | "nd" | "rd" | "th" => OrdinalMarker::Suffix(suffix),
            "º" | ".º" => OrdinalMarker::Masculine,
            "ª" | ".ª" => OrdinalMarker::Feminine,
            _ => return None,
        };
        let n = plain_integer(digits)?;
        let next_word = self.next_word(index);
        if let Some(spoken) = self.rules.ordinal(n, marker, next_word, self.locale) {
            self.change(
                ChangeKind::Ordinal,
                field.core_start,
                field.core_end,
                spoken,
            );
        }
        Some(index + 1)
    }

    /// Slash forms: dates (`26/10/2026`) and fractions (`3/4`).
    fn slash(&mut self, core: &str, start: usize, end: usize) {
        let parts: Vec<&str> = core.split('/').collect();
        if !parts.iter().all(|p| all_digits(p)) {
            return;
        }
        match parts.as_slice() {
            [_, _] if self.language == "it" => {
                use crate::speech::shapes::{self, RelationValue, ShapePolicy};
                let options = crate::speech::SpeechOptions::new(crate::speech::SpeechLocale::En);
                let values = shapes::relation(core, 0, &options, ShapePolicy::Versioned)
                    .filter(|shape| shape.end == core.len())
                    .and_then(|shape| match shape.value {
                        RelationValue::Fraction {
                            numerator,
                            denominator,
                        } => Some((
                            numerator.integer.parse::<u64>().ok()?,
                            denominator.integer.parse::<u64>().ok()?,
                        )),
                        _ => None,
                    });
                if self.style == ReadingStyle::Natural
                    && let Some((numerator, denominator)) = values
                    && numerator <= MAX_INTEGER
                    && denominator <= MAX_INTEGER
                    && let Some(spoken) = self.rules.fraction(numerator, denominator, self.locale)
                {
                    self.change(ChangeKind::Fraction, start, end, spoken);
                } else {
                    self.warn(WarningCode::AmbiguousFraction, start, end);
                }
            }
            // Typed locales receive the same versioned compatibility through
            // shared.rs; `[_, _]` date disambiguation never reaches this arm.
            [_, _] => {}
            [year, month, day] if year.len() == 4 && month.len() <= 2 && day.len() <= 2 => {
                self.calendar_date(core, start, end)
            }
            [a, b, year] if a.len() <= 2 && b.len() <= 2 => match year.len() {
                4 => self.slash_date(a, b, year, start, end),
                2 => self.warn(WarningCode::AmbiguousDate, start, end),
                _ => {}
            },
            _ => {}
        }
    }

    fn slash_date(&mut self, a: &str, b: &str, year: &str, start: usize, end: usize) {
        // Field joining and four-digit-year acceptance remain API policies;
        // calendar structure/order/validity are decoded by the shared core.
        self.calendar_date(&format!("{a}/{b}/{year}"), start, end);
    }

    fn calendar_date(&mut self, raw: &str, start: usize, end: usize) {
        match shared_date_fields(raw, &self.conv, self.locale_explicit) {
            Ok((year, month, day)) => {
                if let Some(spoken) = self.rules.date(year, month, day, self.locale) {
                    self.change(ChangeKind::Date, start, end, spoken);
                }
            }
            Err(warning) => self.warn(warning, start, end),
        }
    }

    /// ISO `YYYY-MM-DD`; other hyphenated digit groups are protected.
    fn iso_date(&mut self, core: &str, start: usize, end: usize) {
        let parts: Vec<&str> = core.split('-').collect();
        let [year, month, day] = parts.as_slice() else {
            return;
        };
        if !(year.len() == 4 && month.len() == 2 && day.len() == 2)
            || !parts.iter().all(|p| all_digits(p))
        {
            return;
        }
        self.calendar_date(core, start, end);
    }

    /// Clock times (`15:30`, `3:30 PM`) and ratios (`1:2`, warned).
    fn clock(&mut self, index: usize) -> usize {
        let field = self.fields[index];
        let core = self.core(index);
        let start = field.core_start;
        let mut end = field.core_end;
        let mut consumed = index + 1;

        let mut clock = core;
        let mut meridiem = None;
        for (form, value) in MERIDIEMS {
            if let Some(rest) = core.strip_suffix(form)
                && rest.ends_with(|c: char| c.is_ascii_digit())
            {
                clock = rest;
                meridiem = Some(*value);
                if form.contains('.')
                    && self.trail(index).starts_with('.')
                    && self.period_continues(index, field.core_end)
                {
                    end = field.core_end + 1;
                }
                break;
            }
        }
        let parts: Vec<&str> = clock.split(':').collect();
        if !parts.iter().all(|p| all_digits(p)) {
            return consumed;
        }
        match parts.as_slice() {
            [hour, minute] if minute.len() == 2 && hour.len() <= 2 => {
                if meridiem.is_none()
                    && let Some(next) = self.joinable(index)
                {
                    let next_core = self.core(next);
                    if let Some((form, value)) = MERIDIEMS.iter().find(|(f, _)| *f == next_core) {
                        meridiem = Some(*value);
                        let next_field = self.fields[next];
                        end = next_field.core_end;
                        if form.contains('.')
                            && self.trail(next).starts_with('.')
                            && self.period_continues(next, next_field.core_end)
                        {
                            end += 1;
                        }
                        consumed = next + 1;
                    }
                }
                // The legacy API owns joining/punctuation and wording; the
                // shared clock decoder owns numeric fields and AM/PM bounds.
                let clock_source = format!(
                    "{clock}{}",
                    match meridiem {
                        Some(Meridiem::Am) => " am",
                        Some(Meridiem::Pm) => " pm",
                        None => "",
                    }
                );
                let options = crate::speech::SpeechOptions::new(crate::speech::SpeechLocale::En);
                let (_, parsed) = crate::speech::temporal::parse_numeric_time(
                    &clock_source,
                    0,
                    clock_source.len(),
                    &options,
                );
                let crate::speech::temporal::TemporalValue::Clock {
                    hour: h,
                    minute: Some(m),
                    second: None,
                    ..
                } = parsed
                else {
                    self.warn(WarningCode::AmbiguousTime, start, end);
                    return consumed;
                };
                match self
                    .rules
                    .time_styled(h, m, meridiem, self.style, self.locale)
                {
                    Some(spoken) => self.change(ChangeKind::Time, start, end, spoken),
                    None => self.warn(WarningCode::AmbiguousTime, start, end),
                }
            }
            [_, _] if meridiem.is_none() => {
                self.warn(WarningCode::AmbiguousFraction, start, end);
            }
            [hour, minute, second] if hour.len() <= 2 && minute.len() == 2 && second.len() == 2 => {
                self.warn(WarningCode::AmbiguousTime, start, end);
            }
            _ => {}
        }
        consumed
    }

    /// Integer ranges with an en dash (`10–15`, `10–15 kg`).
    fn range(&mut self, index: usize) -> usize {
        let field = self.fields[index];
        let core = self.core(index);
        use crate::speech::shapes::{self, RelationValue, ShapePolicy};
        let options = crate::speech::SpeechOptions::new(crate::speech::SpeechLocale::En);
        let Some(shape) = shapes::relation(core, 0, &options, ShapePolicy::Versioned)
            .filter(|shape| shape.end == core.len())
        else {
            return index + 1;
        };
        let RelationValue::Range { from, to } = shape.value else {
            return index + 1;
        };
        let (Ok(from), Ok(to)) = (from.integer.parse::<u64>(), to.integer.parse::<u64>()) else {
            return index + 1;
        };
        let mut end = field.core_end;
        let mut consumed = index + 1;
        let mut unit = None;
        if let Some(next) = self.joinable(index)
            && let Some(found) = unit_from_str(self.core(next))
        {
            unit = Some(found);
            end = self.fields[next].core_end;
            consumed = next + 1;
        }
        if let Some(spoken) = self.rules.range(from, to, unit, self.locale) {
            self.change(ChangeKind::Range, field.core_start, end, spoken);
        }
        consumed
    }

    fn plain_number(&mut self, index: usize) -> usize {
        let field = self.fields[index];
        let core = self.core(index);
        let (start, end) = (field.core_start, field.core_end);
        let quantity_context = self
            .next_word(index)
            .is_some_and(|word| self.rules.quantity_context(word));
        let context = if quantity_context {
            NumberContext::Quantity
        } else {
            NumberContext::Plain
        };
        let parsed = parse_number_in(core, &self.conv, context);
        let (negative, body) = strip_sign(core);
        let pure_digits = !negative && all_digits(body) && !is_protected_identifier(core);

        if let Some(next) = self.joinable(index) {
            let next_core = self.core(next);
            let next_end = self.fields[next].core_end;
            if next_core == "%" {
                self.percent(core, start, next_end);
                return next + 1;
            }
            if let Some(sym) = symbol(next_core)
                && next_core != "S/"
                && next_core != "S/."
            {
                return self
                    .money(sym, core, start, next_end, next)
                    .unwrap_or(next + 1);
            }
            if let Some(unit) = unit_from_str(next_core) {
                match parsed {
                    NumParse::Value(amount) => {
                        if let Some(spoken) = self.rules.unit(&amount, unit, self.locale) {
                            self.change(ChangeKind::Unit, start, next_end, spoken);
                        }
                    }
                    NumParse::Ambiguous => {
                        self.warn(WarningCode::AmbiguousNumber, start, end);
                    }
                    NumParse::TooLarge => {
                        self.warn(WarningCode::NumberTooLarge, start, end);
                    }
                    NumParse::NotNumber => {}
                }
                return next + 1;
            }
        }

        // `digits` reads the written sequence, including leading zeros. It
        // bypasses cardinal parsing (and gender agreement) only for a pure
        // unsigned digit run, so identifiers and versions keep their
        // existing protection.
        if self.style == ReadingStyle::Digits
            && pure_digits
            && let Some(spoken) = self.rules.digits(body, self.locale)
        {
            self.change(ChangeKind::Number, start, end, spoken);
            return index + 1;
        }

        let amount = match parsed {
            NumParse::Value(amount) => amount,
            NumParse::Ambiguous => {
                self.warn(WarningCode::AmbiguousNumber, start, end);
                return index + 1;
            }
            NumParse::TooLarge => {
                self.warn(WarningCode::NumberTooLarge, start, end);
                return index + 1;
            }
            NumParse::NotNumber => return index + 1,
        };
        if amount.frac.is_none()
            && let Some(word) = self.next_word(index)
            && self.rules.number_gender_ambiguous(amount.int, word)
        {
            self.warn(WarningCode::AmbiguousNumber, start, end);
            return index + 1;
        }
        if self.style == ReadingStyle::Digits && amount.frac.is_none() {
            // Grouped and signed integers still use their written digits
            // (without separators or the sign), never the parsed value.
            let digits: String = body
                .bytes()
                .filter(u8::is_ascii_digit)
                .map(char::from)
                .collect();
            if let Some(spoken) = self.rules.digits(&digits, self.locale) {
                self.change(ChangeKind::Number, start, end, spoken);
                return index + 1;
            }
        }
        let kind = if amount.frac.is_some() {
            ChangeKind::Decimal
        } else {
            ChangeKind::Number
        };
        if let Some(spoken) = self.rules.number(&amount, self.locale) {
            self.change(kind, start, end, spoken);
        }
        index + 1
    }
}

#[cfg(test)]
mod tests {
    use super::super::rules::DateOrder;
    use super::*;

    const ES: Conventions = Conventions {
        decimal: ',',
        group: '.',
        separators_unambiguous: false,
        date_order: DateOrder::DayMonthYear,
        dollar: None,
    };
    const EN: Conventions = Conventions {
        decimal: '.',
        group: ',',
        separators_unambiguous: true,
        date_order: DateOrder::MonthDayYear,
        dollar: Some(Currency::Usd),
    };

    fn value(negative: bool, int: u64, frac: Option<&str>) -> NumParse {
        NumParse::Value(Amount {
            negative,
            int,
            frac: frac.map(str::to_string),
        })
    }

    #[test]
    fn spanish_number_parsing() {
        assert_eq!(parse_number("1234", &ES, false), value(false, 1234, None));
        assert_eq!(parse_number("-5", &ES, false), value(true, 5, None));
        assert_eq!(parse_number("\u{2212}5", &ES, false), value(true, 5, None));
        assert_eq!(
            parse_number("12,5", &ES, false),
            value(false, 12, Some("5"))
        );
        assert_eq!(
            parse_number("1.234,56", &ES, false),
            value(false, 1234, Some("56"))
        );
        assert_eq!(
            parse_number("1.234.567", &ES, false),
            value(false, 1_234_567, None)
        );
        assert_eq!(parse_number("1.234", &ES, false), NumParse::Ambiguous);
        assert_eq!(parse_number("1,234", &ES, false), NumParse::Ambiguous);
        assert_eq!(parse_number("3.14", &ES, false), NumParse::Ambiguous);
        assert_eq!(parse_number("1,234.56", &ES, false), NumParse::Ambiguous);
        assert_eq!(parse_number("007", &ES, false), NumParse::Ambiguous);
        assert_eq!(parse_number("1.21.11", &ES, false), NumParse::NotNumber);
        assert_eq!(parse_number("10.0.0.1", &ES, false), NumParse::NotNumber);
        assert_eq!(parse_number("1.500", &ES, true), value(false, 1500, None));
        assert_eq!(parse_number("1,500", &ES, true), NumParse::Ambiguous);
        assert_eq!(parse_number("0,5", &ES, false), value(false, 0, Some("5")));
        assert_eq!(parse_number("1,", &ES, false), NumParse::NotNumber);
        assert_eq!(parse_number(",5", &ES, false), NumParse::NotNumber);
        assert_eq!(parse_number("1..2", &ES, false), NumParse::NotNumber);
    }

    #[test]
    fn quantity_context_resolves_padding_and_grouping() {
        let quantity = |raw| parse_number_in(raw, &ES, NumberContext::Quantity);
        assert_eq!(quantity("0"), value(false, 0, None));
        assert_eq!(quantity("00"), value(false, 0, None));
        assert_eq!(quantity("007"), value(false, 7, None));
        assert_eq!(quantity("00042"), value(false, 42, None));
        assert_eq!(quantity("-00042"), value(true, 42, None));
        assert_eq!(quantity("10.005"), value(false, 10_005, None));
        assert_eq!(quantity("1.234"), value(false, 1234, None));
        // Still ambiguous: the separator has a non-grouping tail, or the
        // token has an identifier shape.
        assert_eq!(quantity("3.14"), NumParse::Ambiguous);
        assert_eq!(quantity("1.21.11"), NumParse::NotNumber);
        assert_eq!(quantity("10.0.0.1"), NumParse::NotNumber);
        // Plain prose keeps leading zeros and single groups ambiguous.
        assert_eq!(
            parse_number_in("007", &ES, NumberContext::Plain),
            NumParse::Ambiguous
        );
        assert_eq!(
            parse_number_in("10.005", &ES, NumberContext::Plain),
            NumParse::Ambiguous
        );
    }

    #[test]
    fn english_number_parsing() {
        assert_eq!(parse_number("1,234", &EN, false), value(false, 1234, None));
        assert_eq!(
            parse_number("3.140", &EN, false),
            value(false, 3, Some("140"))
        );
        assert_eq!(
            parse_number("0.123", &EN, false),
            value(false, 0, Some("123"))
        );
        assert_eq!(
            parse_number("1.234", &EN, false),
            value(false, 1, Some("234"))
        );
        assert_eq!(parse_number("1.500", &EN, true), NumParse::Ambiguous);
        assert_eq!(parse_number("3,14", &EN, false), NumParse::Ambiguous);
        assert_eq!(parse_number("1.234,56", &EN, false), NumParse::Ambiguous);
        assert_eq!(
            parse_number("1,234,567.89", &EN, false),
            value(false, 1_234_567, Some("89"))
        );
    }

    #[test]
    fn number_limits() {
        assert_eq!(
            parse_number("999999999999999", &EN, false),
            value(false, MAX_INTEGER, None)
        );
        assert_eq!(
            parse_number("1000000000000000", &EN, false),
            NumParse::TooLarge
        );
        assert_eq!(
            parse_number(&"9".repeat(10_000), &EN, false),
            NumParse::TooLarge
        );
        assert_eq!(
            parse_number("1.1234567890123", &EN, false),
            NumParse::TooLarge
        );
        assert_eq!(
            parse_number("1.123456789012", &EN, false),
            value(false, 1, Some("123456789012"))
        );
    }
}
