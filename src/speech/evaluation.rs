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
    /// Complete independently annotated non-Text spans, compared field for
    /// field. None leaves semantic evaluation unmeasured; Some([]) asserts
    /// that the source contains no structured spans.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_values: Option<Vec<SpeechSpan>>,
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
                            | "labelled_number"
                            | "scientific"
                            | "fraction"
                            | "range"
                            | "ordinal"
                            | "duration"
                            | "date"
                            | "time"
                            | "currency"
                            | "percent"
                            | "measure"
                            | "identifier"
                            | "ambiguous"
                            | "invalid_date"
                    )
                {
                    return Err(invalid(format!("invalid gold span in {}", case.id)));
                }
                end = span.end;
            }
            let mut end = 0;
            for span in case.expected_values.iter().flatten() {
                if span.start < end
                    || span.start >= span.end
                    || case.text.get(span.start..span.end) != Some(span.source.as_str())
                    || matches!(span.value, SpeechValue::Text)
                {
                    return Err(invalid(format!(
                        "invalid semantic gold span in {}",
                        case.id
                    )));
                }
                end = span.end;
            }
            if let (Some(spans), Some(values)) = (&case.expected_spans, &case.expected_values)
                && (spans.len() != values.len()
                    || spans.iter().zip(values).any(|(span, value)| {
                        span.start != value.start
                            || span.end != value.end
                            || span.source != value.source
                            || span.kind != speech_kind(&value.value)
                    }))
            {
                return Err(invalid(format!(
                    "contradictory span and semantic gold in {}",
                    case.id
                )));
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
    #[serde(default)]
    pub spoken_variant: usize,
    #[serde(default)]
    pub spoken_accepted: usize,
    pub fully_supported: usize,
    pub expected_spans: usize,
    pub predicted_spans: usize,
    pub correct_spans: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic: Option<SpeechSemanticMetrics>,
}

/// Exact semantic IR evaluation under the selected parsing options. No numeric
/// projection, floating-point conversion or spoken-variant rule changes gold.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SpeechSemanticMetrics {
    pub cases: usize,
    pub exact: usize,
    pub expected_spans: usize,
    pub predicted_spans: usize,
    pub correct_spans: usize,
}
impl SpeechSemanticMetrics {
    pub fn accuracy(&self) -> f64 {
        ratio(self.exact, self.cases)
    }
    pub fn precision(&self) -> f64 {
        ratio(self.correct_spans, self.predicted_spans)
    }
    pub fn recall(&self) -> f64 {
        ratio(self.correct_spans, self.expected_spans)
    }
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
/// Explicit evaluation conventions, kept separate from immutable reference gold.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SpeechEvaluationPolicy {
    pub reading_style: Option<ReadingStyle>,
    pub number_notation: Option<NumberNotation>,
    pub currency_naming: Option<CurrencyNaming>,
    pub clock_policy: Option<ClockPolicy>,
    pub invalid_date_policy: Option<InvalidDatePolicy>,
    /// Use the caller-supplied corpus category as a semantic say-as hint.
    pub category_hints: bool,
    pub locales: BTreeMap<String, SpeechLocaleEvaluationPolicy>,
}
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SpeechLocaleEvaluationPolicy {
    pub infer_decimal_separator: bool,
    pub date_order: Option<DateOrder>,
    pub decimal_separator: Option<char>,
    pub currency_symbols: BTreeMap<String, String>,
    pub unit_aliases: BTreeMap<String, String>,
}
impl SpeechEvaluationPolicy {
    pub fn validate(&self) -> Result<(), TextIntelError> {
        for (code, local) in &self.locales {
            let locale = code
                .parse::<SpeechLocale>()
                .map_err(TextIntelError::InvalidConfiguration)?;
            if locale.code() != code {
                return Err(TextIntelError::InvalidConfiguration(
                    "evaluation locale keys must be canonical base-language codes".into(),
                ));
            }
            let mut options = SpeechOptions::new(locale);
            options.decimal_separator = local.decimal_separator;
            options.currency_symbols = local.currency_symbols.clone();
            options.unit_aliases = local.unit_aliases.clone();
            options.validate()?;
        }
        Ok(())
    }
}
/// Identical inputs/options whose reference outputs have no common alternative.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpeechReferenceConflict {
    pub locale: SpeechLocale,
    pub text: String,
    pub case_ids: Vec<String>,
    pub unavoidable_exact_failures: usize,
}
impl SpeechCorpus {
    /// Audit contradictory spoken gold without dropping or rewriting any cases.
    pub fn reference_conflicts(&self) -> Vec<SpeechReferenceConflict> {
        self.reference_conflicts_with_policy(&SpeechEvaluationPolicy::default())
    }
    pub fn reference_conflicts_with_policy(
        &self,
        policy: &SpeechEvaluationPolicy,
    ) -> Vec<SpeechReferenceConflict> {
        let mut groups: BTreeMap<String, Vec<&SpeechCase>> = BTreeMap::new();
        for case in self.cases.iter().filter(|c| !c.spoken.is_empty()) {
            let key = serde_json::to_string(&(&case.text, options_for_case(case, policy)))
                .expect("serializable reference fields");
            groups.entry(key).or_default().push(case);
        }
        let mut conflicts = Vec::new();
        for cases in groups.values().filter(|v| v.len() > 1) {
            let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
            for case in cases {
                let alternatives: std::collections::BTreeSet<_> =
                    case.spoken.iter().map(String::as_str).collect();
                for alternative in alternatives {
                    *counts.entry(alternative).or_default() += 1;
                }
            }
            let best = counts.values().max().copied().unwrap_or(0);
            if best < cases.len() {
                conflicts.push(SpeechReferenceConflict {
                    locale: cases[0].locale,
                    text: cases[0].text.clone(),
                    case_ids: cases.iter().map(|c| c.id.clone()).collect(),
                    unavoidable_exact_failures: cases.len() - best,
                });
            }
        }
        conflicts
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpeechFailure {
    pub id: String,
    pub locale: SpeechLocale,
    pub category: String,
    pub expected_spoken: Vec<String>,
    pub expected: Option<Vec<ExpectedSpeechSpan>>,
    pub predicted: Vec<ExpectedSpeechSpan>,
    pub spoken: Option<String>,
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_values: Option<Vec<SpeechSpan>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predicted_values: Option<Vec<SpeechSpan>>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpeechVariantAcceptance {
    pub id: String,
    pub reference: String,
    pub rules: Vec<String>,
}
/// A retained source diagnostic, including literal readings that generated
/// speech successfully. Rendering support does not imply calendar validity.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpeechCaseDiagnostic {
    pub case_id: String,
    pub locale: SpeechLocale,
    pub kind: String,
    pub start: usize,
    pub end: usize,
    pub source: String,
    pub spoken: String,
    pub reading_supported: bool,
    pub reason: String,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpeechEvalReport {
    #[serde(default)]
    pub policy: SpeechEvaluationPolicy,
    #[serde(default)]
    pub reference_conflicts: Vec<SpeechReferenceConflict>,
    #[serde(default)]
    pub exact_spoken_ceiling: usize,
    #[serde(default)]
    pub variant_matches: Vec<SpeechVariantAcceptance>,
    #[serde(default)]
    pub variants_by_rule: BTreeMap<String, usize>,
    pub corpus_version: String,
    pub purpose: String,
    pub totals: SpeechSliceMetrics,
    pub by_locale: BTreeMap<String, SpeechSliceMetrics>,
    pub by_category: BTreeMap<String, SpeechSliceMetrics>,
    pub failures: Vec<SpeechFailure>,
    pub unresolved_by_reason: BTreeMap<String, usize>,
    #[serde(default)]
    pub diagnostics: Vec<SpeechCaseDiagnostic>,
    #[serde(default)]
    pub diagnostics_by_reason: BTreeMap<String, usize>,
}
pub fn speech_kind(value: &SpeechValue) -> &'static str {
    match value {
        SpeechValue::Text => "text",
        SpeechValue::InflectedNumber { .. } => "number",
        SpeechValue::MeasuredExpression { .. } => "measure",
        SpeechValue::LabelledNumber { .. } => "labelled_number",
        SpeechValue::Expression { value } => super::expressions::kind(value),
        SpeechValue::Temporal { value } => super::temporal::kind(value),
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
fn options_for_case(case: &SpeechCase, policy: &SpeechEvaluationPolicy) -> SpeechOptions {
    let mut options = SpeechOptions::new(case.locale);
    options.date_order = case.date_order;
    options.decimal_separator = case.decimal_separator;
    if let Some(style) = policy.reading_style {
        options.reading_style = style;
    }
    if let Some(notation) = policy.number_notation {
        options.number_notation = notation;
    }
    if let Some(naming) = policy.currency_naming {
        options.currency_naming = naming;
    }
    if let Some(clock_policy) = policy.clock_policy {
        options.clock_policy = clock_policy;
    }
    if let Some(invalid_date_policy) = policy.invalid_date_policy {
        options.invalid_date_policy = invalid_date_policy;
    }
    if policy.category_hints {
        options.interpret_as = match case.category.as_str() {
            "number" => SpeechInterpretation::Number,
            "date" => SpeechInterpretation::Date,
            "time" => SpeechInterpretation::Time,
            "currency" => SpeechInterpretation::Currency,
            "measure" => SpeechInterpretation::Measure,
            _ => SpeechInterpretation::Auto,
        };
    }
    if let Some(local) = policy.locales.get(case.locale.code()) {
        if let Some(order) = local.date_order {
            options.date_order = order;
        }
        if local.infer_decimal_separator {
            options.decimal_separator = None;
        }
        if let Some(separator) = local.decimal_separator {
            options.decimal_separator = Some(separator);
        }
        options.currency_symbols = local.currency_symbols.clone();
        options.unit_aliases = local.unit_aliases.clone();
    }
    options
}
/// Evaluate all fixtures, including unsupported and failed cases in denominators.
pub fn evaluate_speech(corpus: &SpeechCorpus) -> Result<SpeechEvalReport, TextIntelError> {
    evaluate_speech_with_policy(corpus, &SpeechEvaluationPolicy::default())
}
/// Evaluate under explicit shared rendering/source conventions, retaining all rows.
pub fn evaluate_speech_with_policy(
    corpus: &SpeechCorpus,
    policy: &SpeechEvaluationPolicy,
) -> Result<SpeechEvalReport, TextIntelError> {
    corpus.validate()?;
    policy.validate()?;
    let conflicts = corpus.reference_conflicts_with_policy(policy);
    let ceiling = corpus.cases.iter().filter(|c| !c.spoken.is_empty()).count()
        - conflicts
            .iter()
            .map(|c| c.unavoidable_exact_failures)
            .sum::<usize>();
    let mut report = SpeechEvalReport {
        policy: policy.clone(),
        reference_conflicts: conflicts,
        exact_spoken_ceiling: ceiling,
        variant_matches: Vec::new(),
        variants_by_rule: BTreeMap::new(),
        corpus_version: corpus.version.clone(),
        purpose: corpus.purpose.clone(),
        totals: Default::default(),
        by_locale: BTreeMap::new(),
        by_category: BTreeMap::new(),
        failures: Vec::new(),
        unresolved_by_reason: BTreeMap::new(),
        diagnostics: Vec::new(),
        diagnostics_by_reason: BTreeMap::new(),
    };
    for case in &corpus.cases {
        let options = options_for_case(case, policy);
        let parsed = super::parser::parse_for_speech(&case.text, &options);
        let mut predicted = Vec::new();
        let mut predicted_values = case.expected_values.as_ref().map(|_| Vec::new());
        let mut spoken = None;
        let mut variant = None;
        let mut fully_supported = false;
        let mut error = None;
        let mut parsed_ok = false;
        match parsed {
            Ok(doc) => {
                parsed_ok = true;
                if let Some(values) = &mut predicted_values {
                    values.extend(
                        doc.spans
                            .iter()
                            .filter(|span| !matches!(span.value, SpeechValue::Text))
                            .cloned(),
                    );
                }
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
                match super::verbalize::verbalize_document_with_options(&doc, &options) {
                    Ok(result) => {
                        for (span, reading) in doc.spans.iter().zip(&result.readings) {
                            if let Some(reason) = &reading.reason {
                                *report
                                    .diagnostics_by_reason
                                    .entry(reason.clone())
                                    .or_insert(0) += 1;
                                report.diagnostics.push(SpeechCaseDiagnostic {
                                    case_id: case.id.clone(),
                                    locale: case.locale,
                                    kind: speech_kind(&span.value).into(),
                                    start: reading.start,
                                    end: reading.end,
                                    source: reading.source.clone(),
                                    spoken: reading.spoken.clone(),
                                    reading_supported: reading.supported,
                                    reason: reason.clone(),
                                });
                            }
                            if !reading.supported {
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
                        }
                        if result.fully_supported && !case.spoken.contains(&result.text) {
                            variant = case.spoken.iter().find_map(|reference| {
                                super::variants::compare_spoken(
                                    &doc,
                                    &options,
                                    &result.text,
                                    reference,
                                )
                                .map(|matched| (reference.clone(), matched))
                            });
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
        let semantic_exact = parsed_ok
            && case
                .expected_values
                .as_ref()
                .is_some_and(|expected| predicted_values.as_ref() == Some(expected));
        let semantic_correct = predicted_values
            .iter()
            .flatten()
            .filter(|predicted| {
                case.expected_values
                    .as_ref()
                    .is_some_and(|expected| expected.contains(predicted))
            })
            .count();
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
            metrics.spoken_variant += usize::from(variant.is_some());
            metrics.spoken_accepted += usize::from(spoken_exact || variant.is_some());
            metrics.fully_supported += usize::from(fully_supported);
            if let Some(expected) = &case.expected_spans {
                metrics.expected_spans += expected.len();
                metrics.predicted_spans += predicted.len();
                metrics.correct_spans += correct;
            }
            if let Some(expected) = &case.expected_values {
                let semantic = metrics.semantic.get_or_insert_with(Default::default);
                semantic.cases += 1;
                semantic.exact += usize::from(semantic_exact);
                semantic.expected_spans += expected.len();
                semantic.predicted_spans += predicted_values.as_ref().map_or(0, Vec::len);
                semantic.correct_spans += semantic_correct;
            }
        }
        if let Some((reference, matched)) = variant {
            for rule in &matched.rules {
                *report.variants_by_rule.entry(rule.clone()).or_default() += 1;
            }
            report.variant_matches.push(SpeechVariantAcceptance {
                id: case.id.clone(),
                reference,
                rules: matched.rules,
            });
        }
        if (case.expected_spans.is_some() && !parse_exact)
            || (case.expected_values.is_some() && !semantic_exact)
            || error.is_some()
            || (!case.spoken.is_empty() && !spoken_exact)
        {
            report.failures.push(SpeechFailure {
                id: case.id.clone(),
                locale: case.locale,
                category: case.category.clone(),
                expected_spoken: case.spoken.clone(),
                expected: case.expected_spans.clone(),
                predicted,
                spoken,
                error,
                expected_values: case.expected_values.clone(),
                predicted_values,
            });
        }
    }
    Ok(report)
}

impl SpeechEvalReport {
    /// Select a gate explicitly; the original exact failure list is never hidden.
    pub fn passes(&self, accept_reviewed_variants: bool) -> bool {
        if self
            .totals
            .semantic
            .as_ref()
            .is_some_and(|semantic| semantic.exact != semantic.cases)
        {
            return false;
        }
        self.failures.iter().all(|failure| {
            if failure.error.is_some()
                || failure
                    .expected_values
                    .as_ref()
                    .is_some_and(|expected| failure.predicted_values.as_ref() != Some(expected))
                || failure
                    .expected
                    .as_ref()
                    .is_some_and(|expected| expected != &failure.predicted)
            {
                return false;
            }
            failure.expected_spoken.is_empty()
                || failure
                    .spoken
                    .as_ref()
                    .is_some_and(|spoken| failure.expected_spoken.contains(spoken))
                || (accept_reviewed_variants
                    && self
                        .variant_matches
                        .iter()
                        .any(|matched| matched.id == failure.id))
        })
    }
}
