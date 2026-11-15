//! Adapter from the versioned API to the shared typed speech pipeline.
//! No lexer, decimal parser, or language-specific verbalizer lives here.
use super::{
    ChangeKind, Confidence, CoverageLevel, NormalizationWarning, NormalizeError, NormalizeOptions,
    TextChange, WarningCode,
};
use crate::speech::{AbbreviationPolicy, SpeechLocale, SpeechOptions, SpeechValue};
use std::collections::BTreeMap;

/// Languages covered by the shared parser that have no legacy rule adapter.
pub(super) fn uses_shared(language: &str) -> bool {
    crate::speech::locales::for_language(language).is_some_and(|entry| !entry.has_versioned_adapter)
}
pub(super) fn coverage(
    language: &str,
    policy: AbbreviationPolicy,
) -> BTreeMap<String, CoverageLevel> {
    super::FEATURE_KEYS
        .iter()
        .map(|key| {
            (
                (*key).into(),
                match *key {
                    "cardinal_numbers" | "decimals" | "percentages" | "currency" | "dates"
                    | "times" | "units" | "ordinals" | "ranges" | "fractions" => {
                        CoverageLevel::Partial
                    }
                    "abbreviations"
                        if language.parse::<SpeechLocale>().is_ok_and(|locale| {
                            crate::speech::abbreviations::enabled_coverage(locale, policy) > 0
                        }) =>
                    {
                        CoverageLevel::Partial
                    }
                    _ => CoverageLevel::None,
                },
            )
        })
        .collect()
}
pub(super) fn classify(
    source: &crate::speech::lexer::LexicalSource<'_>,
    opts: &NormalizeOptions,
) -> Result<(Vec<TextChange>, Vec<NormalizationWarning>), NormalizeError> {
    let locale: SpeechLocale = opts
        .language
        .parse()
        .map_err(|_| NormalizeError::InvalidLanguage)?;
    let mut options = SpeechOptions::new(locale);
    options.max_input_chars = opts.limits.max_input_chars.max(1);
    options.max_spans = opts.limits.max_input_chars.saturating_add(1).max(1);
    options.reading_style = opts.reading_style;
    options.abbreviation_policy = opts.abbreviation_policy;
    if let Some(profile) = crate::speech::locales::for_locale(&opts.locale) {
        options.decimal_separator = Some(profile.decimal_separator);
        if opts.locale_explicit {
            options.date_order = profile.date_order;
        }
    }
    let document = crate::speech::parser::parse_lexical_source(source, &options)
        .map_err(|_| NormalizeError::InvalidReadingOptions)?;
    let result = crate::speech::verbalize::verbalize_document_with_options(&document, &options)
        .map_err(|_| NormalizeError::InvalidReadingOptions)?;
    let mut changes = Vec::new();
    let mut warnings = Vec::new();
    for (span, reading) in document.spans.iter().zip(&result.readings) {
        let kind = match &span.value {
            SpeechValue::Text => continue,
            SpeechValue::Number { value, .. } => {
                if value.fraction.is_some() {
                    ChangeKind::Decimal
                } else {
                    ChangeKind::Number
                }
            }
            SpeechValue::Abbreviation { .. } => ChangeKind::Abbreviation,
            SpeechValue::Date { .. } => ChangeKind::Date,
            SpeechValue::Time { .. } => ChangeKind::Time,
            SpeechValue::Currency { .. } => ChangeKind::Currency,
            SpeechValue::Percent { .. } => ChangeKind::Percentage,
            SpeechValue::InflectedNumber { .. } => ChangeKind::Number,
            SpeechValue::MeasuredExpression { .. } => ChangeKind::Unit,
            SpeechValue::LabelledNumber { .. } => ChangeKind::Unit,
            SpeechValue::Measure { .. } => ChangeKind::Unit,
            SpeechValue::Expression { value } => match crate::speech::expressions::kind(value) {
                "ordinal" => ChangeKind::Ordinal,
                "fraction" => ChangeKind::Fraction,
                "range" => ChangeKind::Range,
                "duration" => ChangeKind::Time,
                _ => ChangeKind::Number,
            },
            SpeechValue::Temporal { value } => {
                if crate::speech::temporal::kind(value) == "time" {
                    ChangeKind::Time
                } else {
                    ChangeKind::Date
                }
            }
            SpeechValue::Identifier { .. } => continue,
            SpeechValue::Ambiguous { .. } => ChangeKind::Number,
        };
        if reading.supported {
            if reading.spoken != span.source {
                changes.push(TextChange {
                    kind,
                    start: span.start,
                    end: span.end,
                    replacement: reading.spoken.clone(),
                    confidence: if kind == ChangeKind::Abbreviation {
                        Confidence::Medium
                    } else {
                        Confidence::High
                    },
                });
            }
        } else {
            let code = match kind {
                ChangeKind::Date => WarningCode::AmbiguousDate,
                ChangeKind::Time => WarningCode::AmbiguousTime,
                ChangeKind::Currency => WarningCode::AmbiguousCurrency,
                _ => WarningCode::AmbiguousNumber,
            };
            warnings.push(NormalizationWarning {
                code,
                start: span.start,
                end: span.end,
            });
        }
    }
    Ok((changes, warnings))
}

pub(super) fn feature(kind: ChangeKind) -> super::rules::Feature {
    use super::rules::Feature;
    match kind {
        ChangeKind::Number => Feature::CardinalNumbers,
        ChangeKind::Decimal => Feature::Decimals,
        ChangeKind::Date => Feature::Dates,
        ChangeKind::Time => Feature::Times,
        ChangeKind::Currency => Feature::Currency,
        ChangeKind::Percentage => Feature::Percentages,
        ChangeKind::Unit => Feature::Units,
        ChangeKind::Ordinal => Feature::Ordinals,
        ChangeKind::Abbreviation => Feature::Abbreviations,
        ChangeKind::Range => Feature::Ranges,
        ChangeKind::Fraction => Feature::Fractions,
    }
}
/// Retain established presentation rules while filling missing feature adapters
/// from the same typed core. Existing replacements and preservation diagnostics win.
pub(super) fn augment(
    source: &crate::speech::lexer::LexicalSource<'_>,
    opts: &NormalizeOptions,
    legacy: &dyn super::rules::LanguageRules,
    changes: &mut Vec<TextChange>,
    warnings: &mut Vec<NormalizationWarning>,
) -> Result<(), NormalizeError> {
    if opts.language.parse::<SpeechLocale>().is_err() {
        return Ok(());
    }
    let text = source.text();
    let (extra, _) = classify(source, opts)?;
    for change in extra {
        if legacy.coverage(feature(change.kind)) != CoverageLevel::None
            && !matches!(change.kind, ChangeKind::Date | ChangeKind::Time)
        {
            continue;
        }
        // A terminal dot can denote a German ordinal; this adapter must not
        // resolve that ambiguity by stripping the marker and reading a count.
        if opts.language == "de"
            && change.kind == ChangeKind::Number
            && text[change.end..].starts_with('.')
        {
            continue;
        }
        if change.kind == ChangeKind::Number && opts.reading_style != super::ReadingStyle::Digits {
            let raw = &text[change.start..change.end];
            if raw.len() > 1 && raw.starts_with('0') && raw.bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
        }
        if changes
            .iter()
            .any(|old| old.start < change.end && change.start < old.end)
        {
            continue;
        }
        if warnings.iter().any(|old| {
            old.start < change.end
                && change.start < old.end
                && !(old.start == change.start && old.end == change.end)
        }) {
            continue;
        }
        // A previously unsupported feature can now resolve its entire span.
        warnings.retain(|old| !(old.start == change.start && old.end == change.end));
        changes.push(change);
    }
    changes.sort_by_key(|v| v.start);
    Ok(())
}
