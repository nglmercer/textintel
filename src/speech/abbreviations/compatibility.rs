//! Context adapter for the versioned normalizer's established abbreviations.
//!
//! The catalogue owns expansions. This adapter retains the original field,
//! punctuation and same-line policy, including Unicode uppercase/lowercase
//! letters and ASCII-only digits. It does not broaden the typed parser policy.
use super::{LegacyContext, legacy_lookup};
use crate::speech::lexer::Field;

/// Return the exclusive end of the written abbreviation and its expansion.
/// `fields` must come from the shared lexer for `text`.
pub(crate) fn legacy_match(
    text: &str,
    fields: &[Field],
    index: usize,
    language: &str,
) -> Option<(usize, &'static str)> {
    let field = *fields.get(index)?;
    if field.protected || !field.has_core() || !text[field.core_end..field.end].starts_with('.') {
        return None;
    }
    let (expansion, context) = legacy_lookup(language, &text[field.core_start..field.core_end])?;
    let dot = field.core_end;
    let after_dot_empty = dot + 1 == field.end;
    let next_first = fields.get(index + 1).and_then(|next| {
        let gap = &text[field.end..next.start];
        (!gap.contains('\n') && next.bare_start())
            .then(|| text[next.start..].chars().next())
            .flatten()
    });
    let matches = match context {
        LegacyContext::CapitalizedWord => {
            after_dot_empty && next_first.is_some_and(char::is_uppercase)
        }
        LegacyContext::CapitalizedWordOrNumber => {
            after_dot_empty && next_first.is_some_and(|c| c.is_uppercase() || c.is_ascii_digit())
        }
        LegacyContext::NotSentenceEnd => period_continues(text, fields, index, dot),
    };
    matches.then_some((dot + 1, expansion))
}

/// The versioned classifier also uses this exact policy for dotted clocks.
/// A following comma, semicolon, colon or closing parenthesis continues the
/// sentence; otherwise require a lowercase letter or ASCII digit on the same
/// line. Quotes and line feeds do not provide continuation evidence.
pub(crate) fn period_continues(text: &str, fields: &[Field], index: usize, dot: usize) -> bool {
    let field = fields[index];
    let after = &text[dot + 1..field.end];
    if let Some(c) = after.chars().next() {
        return matches!(c, ',' | ';' | ':' | ')');
    }
    let Some(next) = fields.get(index + 1) else {
        return false;
    };
    let gap = &text[field.end..next.start];
    !gap.contains('\n')
        && text[next.start..]
            .chars()
            .next()
            .is_some_and(|c| c.is_lowercase() || c.is_ascii_digit())
}
