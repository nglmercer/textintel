//! Exact-span and spoken-form evaluation, separate from source-code coverage.
use super::types::*;
use crate::TextIntelError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpeechCorpus {
    pub version: String,
    pub purpose: String,
    pub cases: Vec<SpeechCase>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpeechCase {
    pub id: String,
    pub locale: SpeechLocale,
    pub text: String,
    pub date_order: DateOrder,
    pub decimal_separator: Option<char>,
    pub category: String,
    pub expected_spans: Option<Vec<ExpectedSpeechSpan>>,
    /// Alternative independently annotated complete spoken strings.
    pub spoken: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ExpectedSpeechSpan {
    pub start: usize,
    pub end: usize,
    pub source: String,
    pub kind: String,
}
impl SpeechCorpus {
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, TextIntelError> {
        let source = std::fs::read_to_string(path)
            .map_err(|e| TextIntelError::Serialization(e.to_string()))?;
        let corpus: Self = serde_json::from_str(&source)?;
        corpus.validate()?;
        Ok(corpus)
    }
    pub fn validate(&self) -> Result<(), TextIntelError> {
        let invalid = |s| TextIntelError::InvalidConfiguration(s);
        if self.version.trim().is_empty() || self.cases.is_empty() {
            return Err(invalid("speech corpus needs a version and cases".into()));
        }
        let mut ids = std::collections::BTreeSet::new();
        for case in &self.cases {
            if case.id.is_empty() || !ids.insert(&case.id) {
                return Err(invalid(format!(
                    "duplicate/empty speech case id {:?}",
                    case.id
                )));
            }
            let mut end = 0;
            for span in case.expected_spans.iter().flatten() {
                if span.start < end
                    || span.start >= span.end
                    || case.text.get(span.start..span.end) != Some(span.source.as_str())
                    || !matches!(
                        span.kind.as_str(),
                        "number"
                            | "date"
                            | "time"
                            | "currency"
                            | "percent"
                            | "measure"
                            | "identifier"
                            | "ambiguous"
                    )
                {
                    return Err(invalid(format!("invalid gold span in {}", case.id)));
                }
                end = span.end;
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SpeechSliceMetrics {
    pub cases: usize,
    pub parse_cases: usize,
    pub parse_exact: usize,
    pub spoken_cases: usize,
    pub spoken_exact: usize,
    pub fully_supported: usize,
    pub expected_spans: usize,
    pub predicted_spans: usize,
    pub correct_spans: usize,
}
impl SpeechSliceMetrics {
    pub fn parse_accuracy(&self) -> f64 {
        ratio(self.parse_exact, self.parse_cases)
    }
    pub fn precision(&self) -> f64 {
        ratio(self.correct_spans, self.predicted_spans)
    }
    pub fn recall(&self) -> f64 {
        ratio(self.correct_spans, self.expected_spans)
    }
}
fn ratio(n: usize, d: usize) -> f64 {
    if d == 0 { 0.0 } else { n as f64 / d as f64 }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpeechFailure {
    pub id: String,
    pub expected: Option<Vec<ExpectedSpeechSpan>>,
    pub predicted: Vec<ExpectedSpeechSpan>,
    pub spoken: Option<String>,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpeechEvalReport {
    pub corpus_version: String,
    pub purpose: String,
    pub totals: SpeechSliceMetrics,
    pub by_locale: BTreeMap<String, SpeechSliceMetrics>,
    pub by_category: BTreeMap<String, SpeechSliceMetrics>,
    pub failures: Vec<SpeechFailure>,
    pub unresolved_by_reason: BTreeMap<String, usize>,
}
pub fn speech_kind(value: &SpeechValue) -> &'static str {
    match value {
        SpeechValue::Text => "text",
        SpeechValue::Number { .. } => "number",
        SpeechValue::Date { .. } => "date",
        SpeechValue::Time { .. } => "time",
        SpeechValue::Currency { .. } => "currency",
        SpeechValue::Percent { .. } => "percent",
        SpeechValue::Measure { .. } => "measure",
        SpeechValue::Identifier { .. } => "identifier",
        SpeechValue::Ambiguous { .. } => "ambiguous",
    }
}
/// Evaluate all fixtures, including unsupported and failed cases in denominators.
pub fn evaluate_speech(corpus: &SpeechCorpus) -> Result<SpeechEvalReport, TextIntelError> {
    corpus.validate()?;
    let mut report = SpeechEvalReport {
        corpus_version: corpus.version.clone(),
        purpose: corpus.purpose.clone(),
        totals: Default::default(),
        by_locale: BTreeMap::new(),
        by_category: BTreeMap::new(),
        failures: Vec::new(),
        unresolved_by_reason: BTreeMap::new(),
    };
    for case in &corpus.cases {
        let mut options = SpeechOptions::new(case.locale);
        options.date_order = case.date_order;
        options.decimal_separator = case.decimal_separator;
        let parsed = super::parser::parse_for_speech(&case.text, &options);
        let mut predicted = Vec::new();
        let mut spoken = None;
        let mut fully_supported = false;
        let mut error = None;
        let mut parsed_ok = false;
        match parsed {
            Ok(doc) => {
                parsed_ok = true;
                predicted = doc
                    .spans
                    .iter()
                    .filter(|s| !matches!(s.value, SpeechValue::Text))
                    .map(|s| ExpectedSpeechSpan {
                        start: s.start,
                        end: s.end,
                        source: s.source.clone(),
                        kind: speech_kind(&s.value).into(),
                    })
                    .collect();
                match super::verbalize::normalize_for_speech(&case.text, &options) {
                    Ok(result) => {
                        for reading in result.readings.iter().filter(|r| !r.supported) {
                            *report
                                .unresolved_by_reason
                                .entry(
                                    reading
                                        .reason
                                        .clone()
                                        .unwrap_or_else(|| "unsupported".into()),
                                )
                                .or_insert(0) += 1;
                        }
                        spoken = Some(result.text);
                        fully_supported = result.fully_supported;
                    }
                    Err(e) => error = Some(e.to_string()),
                }
            }
            Err(e) => error = Some(e.to_string()),
        }
        let parse_exact = parsed_ok
            && case
                .expected_spans
                .as_ref()
                .is_some_and(|expected| &predicted == expected);
        let spoken_exact = spoken.as_ref().is_some_and(|s| case.spoken.contains(s));
        let correct = predicted
            .iter()
            .filter(|p| case.expected_spans.as_ref().is_some_and(|v| v.contains(p)))
            .count();
        for metrics in [
            &mut report.totals,
            report
                .by_locale
                .entry(case.locale.code().into())
                .or_default(),
            report.by_category.entry(case.category.clone()).or_default(),
        ] {
            metrics.cases += 1;
            metrics.parse_cases += usize::from(case.expected_spans.is_some());
            metrics.parse_exact += usize::from(parse_exact);
            metrics.spoken_cases += usize::from(!case.spoken.is_empty());
            metrics.spoken_exact += usize::from(spoken_exact);
            metrics.fully_supported += usize::from(fully_supported);
            if let Some(expected) = &case.expected_spans {
                metrics.expected_spans += expected.len();
                metrics.predicted_spans += predicted.len();
                metrics.correct_spans += correct;
            }
        }
        if (case.expected_spans.is_some() && !parse_exact)
            || error.is_some()
            || (!case.spoken.is_empty() && !spoken_exact)
        {
            report.failures.push(SpeechFailure {
                id: case.id.clone(),
                expected: case.expected_spans.clone(),
                predicted,
                spoken,
                error,
            });
        }
    }
    Ok(report)
}
