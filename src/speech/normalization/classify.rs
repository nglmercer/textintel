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

use super::lexer::{Field, is_protected_identifier, scan_fields};
use super::rules::{
    AbbrevContext, Amount, Conventions, Currency, DateOrder, LanguageRules, Meridiem,
    OrdinalMarker, UNIT_FORMS, Unit, unit_from_str, valid_date,
};
use super::{
    ChangeKind, Confidence, NormalizationWarning, NormalizeOptions, ReadingStyle, TextChange,
    WarningCode,
};
use chrono::{Datelike, Timelike};

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

fn valid_first_group(group: &str) -> bool {
    (1..=3).contains(&group.len()) && !group.starts_with('0')
}

fn grouped(groups: &[&str]) -> bool {
    groups.len() >= 2 && valid_first_group(groups[0]) && groups[1..].iter().all(|g| g.len() == 3)
}

fn integer(negative: bool, digits: &str) -> NumParse {
    if digits.len() > 15 {
        return NumParse::TooLarge;
    }
    match digits.parse::<u64>() {
        Ok(int) if int <= MAX_INTEGER => NumParse::Value(Amount {
            negative,
            int,
            frac: None,
        }),
        _ => NumParse::TooLarge,
    }
}

/// Read a zero-padded count in a quantity context. Leading zeros are only
/// dropped here, after the caller has established that the token is a count;
/// the original span is still what the change replaces.
fn quantity_integer(negative: bool, digits: &str) -> NumParse {
    let significant = digits.trim_start_matches('0');
    integer(
        negative,
        if significant.is_empty() {
            "0"
        } else {
            significant
        },
    )
}

fn decimal(negative: bool, int_digits: &str, frac: &str) -> NumParse {
    if frac.len() > MAX_FRACTION_DIGITS {
        return NumParse::TooLarge;
    }
    match integer(negative, int_digits) {
        NumParse::Value(mut amount) => {
            amount.frac = Some(frac.to_string());
            NumParse::Value(amount)
        }
        other => other,
    }
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
            .all(|b| b.is_ascii_digit() || *b == b'.' || *b == b',')
        || !bytes[0].is_ascii_digit()
        || !bytes[bytes.len() - 1].is_ascii_digit()
    {
        return NumParse::NotNumber;
    }
    let separators: Vec<char> = body.chars().filter(|c| *c == '.' || *c == ',').collect();
    if separators.len() > 8 {
        return NumParse::NotNumber;
    }
    let groups: Vec<&str> = body.split(['.', ',']).collect();
    if groups.iter().any(|g| g.is_empty()) {
        return NumParse::NotNumber;
    }
    match separators.len() {
        0 => {
            if body.len() > 1 && body.starts_with('0') {
                // Leading-zero sequences are IDs in plain prose. A count
                // before a noun is the one context where they are provably
                // padded integers (`007 errores`).
                return if quantity_ctx {
                    quantity_integer(negative, body)
                } else {
                    NumParse::Ambiguous
                };
            }
            integer(negative, body)
        }
        1 => {
            let (int_part, rest) = (groups[0], groups[1]);
            let sep = separators[0];
            if valid_first_group(int_part) && rest.len() == 3 {
                if sep == conv.group
                    && (conv.separators_unambiguous || currency_ctx || quantity_ctx)
                {
                    return integer(negative, &format!("{int_part}{rest}"));
                }
                if sep == conv.decimal && conv.separators_unambiguous && !currency_ctx {
                    return decimal(negative, int_part, rest);
                }
                return NumParse::Ambiguous;
            }
            if sep == conv.decimal && (int_part == "0" || !int_part.starts_with('0')) {
                return decimal(negative, int_part, rest);
            }
            // The separator is not the locale decimal. With no valid
            // three-digit grouping it cannot be a group separator, so two
            // shapes are unambiguous even in a comma/dot locale:
            // a leading-zero decimal (`0.05`, `-0.0001`) and a high-precision
            // decimal (`3.1415926535`, four or more fractional digits).
            // `3.14`, `12.5` and `1.234` stay ambiguous on purpose.
            let leading_zero = int_part == "0";
            let high_precision = rest.len() >= 4;
            if (leading_zero || high_precision) && (int_part == "0" || !int_part.starts_with('0')) {
                return decimal(negative, int_part, rest);
            }
            NumParse::Ambiguous
        }
        count => {
            let last = separators[count - 1];
            let head = &separators[..count - 1];
            if separators.iter().all(|c| *c == conv.group) {
                return if grouped(&groups) {
                    integer(negative, &groups.concat())
                } else {
                    NumParse::NotNumber
                };
            }
            let int_groups = &groups[..groups.len() - 1];
            let frac = groups[groups.len() - 1];
            if last == conv.decimal && head.iter().all(|c| *c == conv.group) {
                return if grouped(int_groups) {
                    decimal(negative, &int_groups.concat(), frac)
                } else {
                    NumParse::NotNumber
                };
            }
            // Other convention (`1,234.56` under es, `1.234.567` under en).
            if separators.iter().all(|c| *c == conv.decimal) {
                return if grouped(&groups) {
                    NumParse::Ambiguous
                } else {
                    NumParse::NotNumber
                };
            }
            if last == conv.group && head.iter().all(|c| *c == conv.decimal) && grouped(int_groups)
            {
                return NumParse::Ambiguous;
            }
            NumParse::NotNumber
        }
    }
}

/// Parse `mantissa e exponent` / `mantissa E exponent` (scientific notation).
///
/// Scientific mantissas are unambiguous, so a single dot or comma is a
/// decimal separator even when it is not the locale convention. Returns
/// `None` for malformed shapes or an exponent with more than two digits
/// (preserved as written).
fn parse_scientific(raw: &str) -> Option<(Amount, i32)> {
    let (mantissa_raw, exponent_raw) = raw.split_once(['e', 'E'])?;
    let (negative, body) = strip_sign(mantissa_raw);
    let (exp_negative, exp_digits) = strip_sign(exponent_raw);
    if !all_digits(exp_digits) || exp_digits.len() > 2 {
        return None;
    }
    let exponent: i32 = exp_digits.parse().ok()?;
    let exponent = if exp_negative { -exponent } else { exponent };
    let groups: Vec<&str> = body.split(['.', ',']).collect();
    let separators = body.chars().filter(|c| matches!(c, '.' | ',')).count();
    let amount = match (groups.as_slice(), separators) {
        ([digits], 0) => match integer(negative, digits) {
            NumParse::Value(value) => value,
            _ => return None,
        },
        ([int_part, frac], 1) => {
            if !all_digits(int_part)
                || !all_digits(frac)
                || (int_part.len() > 1 && int_part.starts_with('0'))
            {
                return None;
            }
            match decimal(negative, int_part, frac) {
                NumParse::Value(value) => value,
                _ => return None,
            }
        }
        _ => return None,
    };
    Some((amount, exponent))
}

/// Parse attached durations: `1h`, `1h30`, `1h30min`, `1h30min15s`, `30min`,
/// `45s`. The bare number after `h` is minutes. Returns
/// `(hours, minutes, seconds)`; at least one component is present.
fn parse_duration(raw: &str) -> Option<(u64, u64, u64)> {
    // `1h`, `1h30`, `1h30min`, `1h30min15s`, `1h30s`.
    if let Some((hours, rest)) = raw.split_once('h') {
        if !all_digits(hours) {
            return None;
        }
        let hours = hours.parse::<u64>().ok()?;
        if rest.is_empty() {
            return Some((hours, 0, 0));
        }
        let (minutes, rest) = leading_digits(rest)?;
        let minutes = minutes.parse::<u64>().ok()?;
        if rest.is_empty() {
            return Some((hours, minutes, 0));
        }
        // `1h30s` reads the bare seconds after `h`.
        if matches!(rest, "s" | "sec") {
            return Some((hours, 0, minutes));
        }
        // An explicit `min` (followed by optional seconds) or a bare `s`.
        let rest = rest.strip_prefix("min").unwrap_or(rest);
        if rest.is_empty() {
            return Some((hours, minutes, 0));
        }
        let seconds = rest
            .strip_suffix("sec")
            .or_else(|| rest.strip_suffix('s'))
            .filter(|n| all_digits(n))?;
        return Some((hours, minutes, seconds.parse().ok()?));
    }
    if let Some(number) = raw.strip_suffix("min").filter(|n| all_digits(n)) {
        return Some((0, number.parse().ok()?, 0));
    }
    if let Some(number) = raw
        .strip_suffix("sec")
        .or_else(|| raw.strip_suffix('s'))
        .filter(|n| all_digits(n))
    {
        return Some((0, 0, number.parse().ok()?));
    }
    None
}

/// Split a leading run of ASCII digits: `("30", "min15s")`. `None` when the
/// string does not start with a digit.
fn leading_digits(raw: &str) -> Option<(&str, &str)> {
    let end = raw.find(|c: char| !c.is_ascii_digit()).unwrap_or(raw.len());
    (end > 0).then(|| raw.split_at(end))
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

const MERIDIEMS: &[(&str, Meridiem)] = &[
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
    locale_explicit: bool,
    style: ReadingStyle,
    fields: Vec<Field>,
    changes: Vec<TextChange>,
    warnings: Vec<NormalizationWarning>,
}

/// Classify `text` for the options' language and locale.
pub(crate) fn classify(
    text: &str,
    rules: &dyn LanguageRules,
    opts: &NormalizeOptions,
) -> (Vec<TextChange>, Vec<NormalizationWarning>) {
    let Some(conv) = rules.conventions(&opts.locale) else {
        return (Vec::new(), Vec::new());
    };
    let mut classifier = Classifier {
        text,
        rules,
        conv,
        locale: &opts.locale,
        locale_explicit: opts.locale_explicit,
        style: opts.reading_style,
        fields: scan_fields(text),
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
        let field = self.fields[index];
        let after = &self.text[dot + 1..field.end];
        if let Some(c) = after.chars().next() {
            return matches!(c, ',' | ';' | ':' | ')');
        }
        let Some(next) = self.fields.get(index + 1) else {
            return false;
        };
        let gap = &self.text[field.end..next.start];
        !gap.contains('\n')
            && self.text[next.start..]
                .chars()
                .next()
                .is_some_and(|c| c.is_lowercase() || c.is_ascii_digit())
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
        let field = self.fields[index];
        let core = self.core(index);
        if !self.trail(index).starts_with('.') {
            return None;
        }
        let (expansion, context) = self.rules.abbreviation(core)?;
        let dot = field.core_end;
        let after_dot_empty = dot + 1 == field.end;
        let next_first = self.fields.get(index + 1).and_then(|next| {
            let gap = &self.text[field.end..next.start];
            (!gap.contains('\n') && next.bare_start())
                .then(|| self.text[next.start..].chars().next())
                .flatten()
        });
        let ok = match context {
            AbbrevContext::CapitalizedWord => {
                after_dot_empty && next_first.is_some_and(char::is_uppercase)
            }
            AbbrevContext::CapitalizedWordOrNumber => {
                after_dot_empty
                    && next_first.is_some_and(|c| c.is_uppercase() || c.is_ascii_digit())
            }
            AbbrevContext::NotSentenceEnd => self.period_continues(index, dot),
        };
        if !ok {
            return None;
        }
        self.change(
            ChangeKind::Abbreviation,
            field.core_start,
            dot + 1,
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
        if core.len() >= 11
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
            match chrono::DateTime::parse_from_rfc3339(core) {
                Ok(value)
                    if fraction.len() <= 9
                        && valid_date(
                            value.date_naive().year() as u32,
                            value.date_naive().month(),
                            value.date_naive().day(),
                        )
                        && value.time().nanosecond() < 1_000_000_000 =>
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

        // Scientific notation is unambiguous and is read in every style.
        if let Some((mantissa, exponent)) = parse_scientific(core) {
            if let Some(spoken) = self.rules.scientific(&mantissa, exponent, self.locale) {
                self.change(ChangeKind::Decimal, start, end, spoken);
            }
            return done;
        }

        // Attached durations are opt-in natural speech. `precise` keeps the
        // original byte-for-byte so existing callers do not change.
        if self.style == ReadingStyle::Natural
            && let Some((hours, minutes, seconds)) = parse_duration(core)
            && let Some(spoken) = self.rules.duration(hours, minutes, seconds, self.locale)
        {
            self.change(ChangeKind::Unit, start, end, spoken);
            return done;
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
        if let Some(next) = self.ordinal(index) {
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
            self.slash(core, start, end);
            return done;
        }
        if core.contains(':') {
            return self.clock(index);
        }
        if core.contains('–') {
            return self.range(index);
        }
        if core.matches('-').count() == 2 && !core.starts_with('-') {
            self.iso_date(core, start, end);
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
            [numerator, denominator] => {
                if self.style == ReadingStyle::Natural
                    && let (Ok(numerator), Ok(denominator)) =
                        (numerator.parse::<u64>(), denominator.parse::<u64>())
                    && denominator > 0
                    && let Some(spoken) = self.rules.fraction(numerator, denominator, self.locale)
                {
                    self.change(ChangeKind::Fraction, start, end, spoken);
                } else {
                    self.warn(WarningCode::AmbiguousFraction, start, end);
                }
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
        let (Ok(a), Ok(b), Ok(year)) = (a.parse::<u32>(), b.parse::<u32>(), year.parse::<u32>())
        else {
            return;
        };
        let day_month = valid_date(year, b, a);
        let month_day = valid_date(year, a, b);
        let (month, day) = match (day_month, month_day) {
            (false, false) => {
                self.warn(WarningCode::InvalidDate, start, end);
                return;
            }
            (true, false) => (b, a),
            (false, true) => (a, b),
            (true, true) if a == b => (a, a),
            (true, true) if self.locale_explicit => match self.conv.date_order {
                DateOrder::DayMonth => (b, a),
                DateOrder::MonthDay => (a, b),
            },
            (true, true) => {
                self.warn(WarningCode::AmbiguousDate, start, end);
                return;
            }
        };
        if let Some(spoken) = self.rules.date(year, month, day, self.locale) {
            self.change(ChangeKind::Date, start, end, spoken);
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
        let (Ok(year), Ok(month), Ok(day)) = (
            year.parse::<u32>(),
            month.parse::<u32>(),
            day.parse::<u32>(),
        ) else {
            return;
        };
        if !valid_date(year, month, day) {
            self.warn(WarningCode::InvalidDate, start, end);
            return;
        }
        if let Some(spoken) = self.rules.date(year, month, day, self.locale) {
            self.change(ChangeKind::Date, start, end, spoken);
        }
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
                let (Ok(h), Ok(m)) = (hour.parse::<u32>(), minute.parse::<u32>()) else {
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
        let parts: Vec<&str> = core.split('–').collect();
        let [from, to] = parts.as_slice() else {
            return index + 1;
        };
        let (Some(from), Some(to)) = (plain_integer(from), plain_integer(to)) else {
            return index + 1;
        };
        if from >= to {
            return index + 1;
        }
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
    use super::*;

    const ES: Conventions = Conventions {
        decimal: ',',
        group: '.',
        separators_unambiguous: false,
        date_order: DateOrder::DayMonth,
        dollar: None,
    };
    const EN: Conventions = Conventions {
        decimal: '.',
        group: ',',
        separators_unambiguous: true,
        date_order: DateOrder::MonthDay,
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
