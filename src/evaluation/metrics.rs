//! Metric primitives for [`EvaluationReport`](super::EvaluationReport).
//!
//! Pure scoring helpers shared by every evaluation entry point; the runners
//! in the parent module own batching and latency accounting.

use std::collections::BTreeSet;

use crate::core::error::TextIntelError;
use crate::engine::TextIntelligence;

use super::{
    BinaryMetrics, EvaluateOptions, EvaluationCase, LanguageMetrics, RankingMetrics, RebusMetrics,
};

pub(crate) fn same_text(left: &str, right: &str) -> bool {
    crate::normalization::unicode::casefold_text(left).trim()
        == crate::normalization::unicode::casefold_text(right).trim()
}

pub(crate) fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

pub(crate) fn binary_metrics(samples: &[(f64, bool)]) -> BinaryMetrics {
    let positives = samples.iter().filter(|(_, label)| *label).count();
    let negatives = samples.len().saturating_sub(positives);
    let mut ranked = samples.to_vec();
    ranked.sort_by(|left, right| right.0.total_cmp(&left.0));
    let roc_auc = if positives == 0 || negatives == 0 {
        0.0
    } else {
        let concordant = samples
            .iter()
            .filter(|(_, label)| *label)
            .map(|(positive, _)| {
                samples
                    .iter()
                    .filter(|(_, label)| !*label)
                    .map(|(negative, _)| {
                        if positive > negative {
                            1.0
                        } else if positive == negative {
                            0.5
                        } else {
                            0.0
                        }
                    })
                    .sum::<f64>()
            })
            .sum::<f64>();
        concordant / (positives * negatives) as f64
    };
    let mut true_positives = 0usize;
    let mut false_positives = 0usize;
    let mut pr_auc = 0.0;
    let mut previous_recall = 0.0;
    for (_, label) in &ranked {
        if *label {
            true_positives += 1;
        } else {
            false_positives += 1;
        }
        let recall = ratio(true_positives, positives);
        let precision = ratio(true_positives, true_positives + false_positives);
        pr_auc += precision * (recall - previous_recall).max(0.0);
        previous_recall = recall;
    }
    let threshold = 0.5;
    let predicted_positive = samples
        .iter()
        .filter(|(score, _)| *score >= threshold)
        .count();
    let true_positive = samples
        .iter()
        .filter(|(score, label)| *score >= threshold && *label)
        .count();
    let false_positive = predicted_positive.saturating_sub(true_positive);
    let false_negative = positives.saturating_sub(true_positive);
    let true_negative = negatives.saturating_sub(false_positive);
    let f1 = if 2 * true_positive + false_positive + false_negative == 0 {
        0.0
    } else {
        2.0 * true_positive as f64 / (2 * true_positive + false_positive + false_negative) as f64
    };
    let accuracy = if samples.is_empty() {
        0.0
    } else {
        (true_positive + true_negative) as f64 / samples.len() as f64
    };
    let precision = ratio(true_positive, true_positive + false_positive);
    let recall = ratio(true_positive, positives);
    let brier = samples
        .iter()
        .map(|(score, label)| {
            let expected = if *label { 1.0 } else { 0.0 };
            (score - expected).powi(2)
        })
        .sum::<f64>()
        / samples.len().max(1) as f64;
    let calibration_bins = (0..10)
        .map(|bin| {
            let lower = bin as f64 / 10.0;
            let upper = (bin + 1) as f64 / 10.0;
            let values = samples
                .iter()
                .filter(|(score, _)| {
                    *score >= lower && (*score < upper || (bin == 9 && *score <= upper))
                })
                .collect::<Vec<_>>();
            if values.is_empty() {
                0.0
            } else {
                let confidence =
                    values.iter().map(|(score, _)| *score).sum::<f64>() / values.len() as f64;
                let accuracy =
                    values.iter().filter(|(_, label)| *label).count() as f64 / values.len() as f64;
                (confidence - accuracy).abs() * values.len() as f64 / samples.len() as f64
            }
        })
        .sum();
    BinaryMetrics {
        count: samples.len(),
        positives,
        negatives,
        accuracy,
        precision,
        recall,
        roc_auc,
        pr_auc,
        f1,
        brier,
        expected_calibration_error: calibration_bins,
    }
}

pub(crate) fn rebus_metrics(
    engine: &TextIntelligence,
    cases: &[&EvaluationCase],
) -> Result<RebusMetrics, TextIntelError> {
    let mut labelled = 0usize;
    let mut top1 = 0usize;
    let mut top3 = 0usize;
    let mut top5 = 0usize;
    let mut reciprocal_sum = 0.0;
    for case in cases {
        if !case.labels.get("rebus").copied().unwrap_or(false) {
            continue;
        }
        labelled += 1;
        // Decode with the case's language labels when present: this mirrors
        // production callers, which know the expected language, and lets
        // language-scoped symbol readings participate.
        let languages = if case.languages.is_empty() {
            None
        } else {
            Some(case.languages.as_slice())
        };
        let candidates = engine.decode_with_languages(&case.a, languages, None)?;
        let mut rank: Option<usize> = None;
        for (index, candidate) in candidates.iter().enumerate() {
            let matches_b = same_text(&candidate.text, &case.b);
            let matches_expected = case
                .expected
                .decoded_contains
                .iter()
                .any(|expected| same_text(&candidate.text, expected));
            if matches_b || matches_expected {
                rank = Some(index + 1);
                break;
            }
        }
        match rank {
            Some(1) => {
                top1 += 1;
                top3 += 1;
                top5 += 1;
                reciprocal_sum += 1.0;
            }
            Some(2..=3) => {
                top3 += 1;
                top5 += 1;
                reciprocal_sum += 1.0 / rank.unwrap_or(1) as f64;
            }
            Some(4..=5) => {
                top5 += 1;
                reciprocal_sum += 1.0 / rank.unwrap_or(1) as f64;
            }
            Some(other) => {
                reciprocal_sum += 1.0 / other as f64;
            }
            None => {}
        }
    }
    Ok(RebusMetrics {
        labelled_cases: labelled,
        top1_accuracy: ratio(top1, labelled),
        top_k_accuracy: ratio(top5, labelled),
        top3_accuracy: ratio(top3, labelled),
        top5_accuracy: ratio(top5, labelled),
        mrr: if labelled == 0 {
            0.0
        } else {
            reciprocal_sum / labelled as f64
        },
    })
}

pub(crate) fn language_metrics(
    engine: &TextIntelligence,
    cases: &[&EvaluationCase],
) -> Result<LanguageMetrics, TextIntelError> {
    let mut labelled = 0usize;
    let mut top1 = 0usize;
    let mut top3 = 0usize;
    let mut unknown_predicted = 0usize;
    let mut unknown_correct = 0usize;
    let mut unknown_support = 0usize;
    for case in cases {
        if case.languages.is_empty() {
            continue;
        }
        labelled += 1;
        let detected = engine.analyze(&case.a)?;
        let predicted: Vec<String> = detected
            .language_candidates
            .iter()
            .map(|candidate| candidate.language.clone())
            .collect();
        let expected_unknown = case
            .languages
            .iter()
            .any(|language| language.eq_ignore_ascii_case("unknown"));
        if expected_unknown {
            unknown_support += 1;
        }
        let predicted_unknown = predicted
            .first()
            .is_some_and(|language| language.eq_ignore_ascii_case("unknown"));
        if predicted_unknown {
            unknown_predicted += 1;
            if expected_unknown {
                unknown_correct += 1;
            }
        }
        let matches = |candidate: &str| {
            case.languages
                .iter()
                .any(|expected| expected.eq_ignore_ascii_case(candidate))
        };
        if predicted.first().is_some_and(|language| matches(language)) {
            top1 += 1;
        }
        if predicted.iter().take(3).any(|language| matches(language)) {
            top3 += 1;
        }
    }
    Ok(LanguageMetrics {
        labelled_cases: labelled,
        top1_accuracy: ratio(top1, labelled),
        top3_accuracy: ratio(top3, labelled),
        unknown_precision: ratio(unknown_correct, unknown_predicted),
        unknown_recall: ratio(unknown_correct, unknown_support),
        unknown_support,
    })
}

pub(crate) fn ranking_metrics(
    engine: &TextIntelligence,
    cases: &[&EvaluationCase],
    options: &EvaluateOptions,
) -> Result<RankingMetrics, TextIntelError> {
    // Retrieval probe: every distinct `b` text is a document and a sample of
    // `a` texts are queries. A query is relevant to its own paired document
    // when the case is labelled similar.
    let mut documents: Vec<String> = Vec::new();
    let mut seen = BTreeSet::new();
    for case in cases {
        if documents.len() >= options.ranking_documents {
            break;
        }
        if seen.insert(case.b.clone()) {
            documents.push(case.b.clone());
        }
    }
    // Every query's own target must be indexed or recall is meaningless.
    let queries: Vec<&&EvaluationCase> = cases.iter().take(options.ranking_queries).collect();
    if queries.is_empty() || documents.is_empty() {
        return Ok(RankingMetrics::default());
    }
    // `analyze_batch` enforces `max_batch_size`: index large corpora in chunks.
    let batch_size = engine.config().max_batch_size.max(1);
    let mut doc_fingerprints = Vec::with_capacity(documents.len());
    for chunk in documents.chunks(batch_size) {
        doc_fingerprints.extend(engine.analyze_batch(chunk)?);
    }
    let mut recall_1 = 0usize;
    let mut recall_5 = 0usize;
    let mut recall_10 = 0usize;
    let mut reciprocal_sum = 0.0;
    let mut ndcg_sum = 0.0;
    let mut relevant_queries = 0usize;
    for query_case in queries {
        let query = engine.analyze(&query_case.a)?;
        let mut scored = doc_fingerprints
            .iter()
            .enumerate()
            .map(|(index, document)| (index, engine.compare_fingerprints(&query, document).score))
            .collect::<Vec<_>>();
        // Deterministic tie-breaking keeps search reproducible.
        scored.sort_by(|left, right| {
            right
                .1
                .total_cmp(&left.1)
                .then_with(|| documents[left.0].cmp(&documents[right.0]))
        });
        let rank = scored
            .iter()
            .position(|(index, _)| same_text(&documents[*index], &query_case.b));
        if !query_case.is_similar() {
            continue;
        }
        relevant_queries += 1;
        match rank {
            Some(0) => {
                recall_1 += 1;
                recall_5 += 1;
                recall_10 += 1;
                reciprocal_sum += 1.0;
                ndcg_sum += 1.0;
            }
            Some(position) => {
                if position < 5 {
                    recall_5 += 1;
                }
                if position < 10 {
                    recall_10 += 1;
                }
                reciprocal_sum += 1.0 / (position + 1) as f64;
                if position < 10 {
                    ndcg_sum += 1.0 / ((position + 2) as f64).log2();
                }
            }
            None => {}
        }
    }
    Ok(RankingMetrics {
        queries: relevant_queries,
        recall_at_1: ratio(recall_1, relevant_queries),
        recall_at_5: ratio(recall_5, relevant_queries),
        recall_at_10: ratio(recall_10, relevant_queries),
        mrr: if relevant_queries == 0 {
            0.0
        } else {
            reciprocal_sum / relevant_queries as f64
        },
        ndcg_at_10: if relevant_queries == 0 {
            0.0
        } else {
            ndcg_sum / relevant_queries as f64
        },
    })
}
