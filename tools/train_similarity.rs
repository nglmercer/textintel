//! Train an interpretable logistic similarity scorer from the evaluation set.
//!
//! Usage:
//!
//! ```text
//! textintel-train similarity data/evaluation.json --output models/similarity-v2.json
//! ```
//!
//! The tool trains on the `train` split only, calibrates the bias on the
//! `validation` split, and reports held-out metrics on `test` without
//! training on it. A production-like engine (semantic and phonetic channels
//! enabled, feature-hash embeddings, rule-based G2P) featurizes every pair so
//! the artifact matches production scoring conditions — including nonzero
//! semantic and phonetic weights whenever the evaluation proves them useful.

use std::collections::BTreeMap;

use textintel::evaluation::EvaluationDataset;
use textintel::semantic::FeatureHashEmbeddingProvider;
use textintel::SimilarityScorer;
use textintel::{
    balanced_sample_weights, language_agreement, logistic_step, logistic_step_weighted,
    training_features, EngineConfig, LogisticSimilarityScorer, SimilarityModelArtifact,
    TextIntelligence, TRAINING_FEATURES,
};

const ITERATIONS: usize = 20000;
const CALIBRATION_ITERATIONS: usize = 500;
const LEARNING_RATE: f64 = 0.2;
const L2: f64 = 1e-3;

fn usage() -> &'static str {
    "Usage:\n  textintel-train similarity <evaluation.json> --output <artifact.json> [--iterations N] [--learning-rate F] [--l2 F]\n  textintel-train spam --output <artifact.json> [--count N] [--seed N]"
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].clone())
}

struct SplitData {
    features: Vec<Vec<f64>>,
    labels: Vec<bool>,
}

fn featurize(
    engine: &TextIntelligence,
    dataset: &EvaluationDataset,
    split: &str,
) -> Result<SplitData, Box<dyn std::error::Error>> {
    let cases = dataset.filter_split(split);
    let mut features = Vec::with_capacity(cases.len());
    let mut labels = Vec::with_capacity(cases.len());
    let mut semantic_present = 0usize;
    let mut phonetic_present = 0usize;
    for case in cases {
        let left = engine.analyze(&case.a)?;
        let right = engine.analyze(&case.b)?;
        let comparison = engine.compare_fingerprints(&left, &right);
        semantic_present += usize::from(comparison.semantic.is_some());
        phonetic_present += usize::from(comparison.phonetic.is_some());
        let agreement = language_agreement(&left, &right);
        let map = training_features(&comparison, agreement);
        features.push(
            TRAINING_FEATURES
                .iter()
                .map(|name| map.get(*name).copied().unwrap_or(0.0))
                .collect(),
        );
        labels.push(case.is_similar());
    }
    println!(
        "{split}: semantic present in {semantic_present}/{} pairs, phonetic in {phonetic_present}/{}",
        features.len(),
        features.len()
    );
    Ok(SplitData { features, labels })
}

fn metrics_at(
    scorer: &LogisticSimilarityScorer,
    engine: &TextIntelligence,
    dataset: &EvaluationDataset,
    split: &str,
) -> Result<BTreeMap<String, f64>, Box<dyn std::error::Error>> {
    // Held-out measurement through the public scorer interface: re-analyze
    // each pair and score with the trained weights.
    let cases = dataset.filter_split(split);
    let mut scores = Vec::with_capacity(cases.len());
    for case in cases {
        let left = engine.analyze(&case.a)?;
        let right = engine.analyze(&case.b)?;
        scores.push((scorer.score(&left, &right).score, case.is_similar()));
    }
    Ok(report(&scores))
}

fn roc_auc(samples: &[(f64, bool)]) -> f64 {
    let positives = samples.iter().filter(|(_, label)| *label).count() as f64;
    let negatives = samples.len() as f64 - positives;
    if positives == 0.0 || negatives == 0.0 {
        return 0.0;
    }
    let mut concordant = 0.0;
    for (positive, _) in samples.iter().filter(|(_, label)| *label) {
        for (negative, _) in samples.iter().filter(|(_, label)| !*label) {
            if positive > negative {
                concordant += 1.0;
            } else if positive == negative {
                concordant += 0.5;
            }
        }
    }
    concordant / (positives * negatives)
}

fn report(samples: &[(f64, bool)]) -> BTreeMap<String, f64> {
    let positives = samples.iter().filter(|(_, label)| *label).count() as f64;
    let total = samples.len() as f64;
    let mut true_positives = 0.0;
    let mut predicted_positive = 0u32;
    for (score, label) in samples {
        if *score >= 0.5 {
            predicted_positive += 1;
            if *label {
                true_positives += 1.0;
            }
        }
    }
    let false_positives = predicted_positive as f64 - true_positives;
    let accuracy = (true_positives + (total - positives - false_positives)) / total.max(1.0);
    let precision = if predicted_positive == 0 {
        0.0
    } else {
        true_positives / predicted_positive as f64
    };
    let recall = if positives == 0.0 {
        0.0
    } else {
        true_positives / positives
    };
    let f1 = if precision + recall == 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    };
    let brier = samples
        .iter()
        .map(|(score, label)| {
            let target = if *label { 1.0 } else { 0.0 };
            (score - target).powi(2)
        })
        .sum::<f64>()
        / total.max(1.0);
    // Best operating threshold by Youden's J, for information only: the
    // scorer always emits probabilities and callers keep their own cutoff.
    let mut thresholds: Vec<f64> = samples.iter().map(|(score, _)| *score).collect();
    thresholds.sort_by(|left, right| left.total_cmp(right));
    let mut best_threshold = 0.5;
    let mut best_j = f64::NEG_INFINITY;
    for threshold in thresholds {
        let (mut tp, mut fp) = (0.0, 0.0);
        for (score, label) in samples {
            if *score >= threshold {
                if *label {
                    tp += 1.0;
                } else {
                    fp += 1.0;
                }
            }
        }
        let tpr = if positives == 0.0 {
            0.0
        } else {
            tp / positives
        };
        let fpr = if total - positives == 0.0 {
            0.0
        } else {
            fp / (total - positives)
        };
        if tpr - fpr > best_j {
            best_j = tpr - fpr;
            best_threshold = threshold;
        }
    }
    let mut report = BTreeMap::new();
    report.insert("accuracy".to_string(), accuracy);
    report.insert("precision".to_string(), precision);
    report.insert("recall".to_string(), recall);
    report.insert("f1".to_string(), f1);
    report.insert("brier".to_string(), brier);
    report.insert("roc_auc".to_string(), roc_auc(samples));
    report.insert("pr_auc".to_string(), pr_auc(samples));
    report.insert("ece".to_string(), expected_calibration_error(samples));
    report.insert("best_threshold".to_string(), best_threshold);
    report
}

fn pr_auc(samples: &[(f64, bool)]) -> f64 {
    let positives = samples.iter().filter(|(_, label)| *label).count();
    if positives == 0 || samples.is_empty() {
        return 0.0;
    }
    let mut ranked = samples.to_vec();
    ranked.sort_by(|left, right| right.0.total_cmp(&left.0));
    let mut area = 0.0;
    let mut hits = 0;
    let mut previous_recall = 0.0;
    for (index, (_, label)) in ranked.iter().enumerate() {
        let seen = index + 1;
        if *label {
            hits += 1;
        }
        let precision = hits as f64 / seen as f64;
        let recall = hits as f64 / positives as f64;
        area += precision * (recall - previous_recall);
        previous_recall = recall;
    }
    area.clamp(0.0, 1.0)
}

fn expected_calibration_error(samples: &[(f64, bool)]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let bins = 10;
    let mut totals = vec![0u32; bins];
    let mut hits = vec![0u32; bins];
    let mut confidence = vec![0.0; bins];
    for (score, label) in samples {
        let bin = ((score.clamp(0.0, 1.0) * bins as f64) as usize).min(bins - 1);
        totals[bin] += 1;
        confidence[bin] += score.clamp(0.0, 1.0);
        if *label {
            hits[bin] += 1;
        }
    }
    let mut error = 0.0;
    for bin in 0..bins {
        if totals[bin] == 0 {
            continue;
        }
        let accuracy = hits[bin] as f64 / totals[bin] as f64;
        let mean_confidence = confidence[bin] / totals[bin] as f64;
        error += totals[bin] as f64 / samples.len() as f64 * (accuracy - mean_confidence).abs();
    }
    error.clamp(0.0, 1.0)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("similarity") => run_similarity(&args),
        Some("spam") => run_spam(&args),
        _ => {
            eprintln!("{}", usage());
            std::process::exit(2);
        }
    }
}

fn run_similarity(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = args
        .get(1)
        .map(String::as_str)
        .unwrap_or("data/evaluation.json");
    let output = flag_value(args, "--output")
        .ok_or("similarity training requires --output <artifact.json>")?;
    let iterations = flag_value(args, "--iterations")
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(|_| "--iterations must be a positive integer")?
        .unwrap_or(ITERATIONS)
        .max(1);
    let learning_rate = flag_value(args, "--learning-rate")
        .map(|value| value.parse::<f64>())
        .transpose()
        .map_err(|_| "--learning-rate must be a number")?
        .unwrap_or(LEARNING_RATE);
    let l2 = flag_value(args, "--l2")
        .map(|value| value.parse::<f64>())
        .transpose()
        .map_err(|_| "--l2 must be a number")?
        .unwrap_or(L2);
    let source = std::fs::read_to_string(path)?;
    let dataset = EvaluationDataset::from_json(&source)?;
    // Production-like featurization: semantic and phonetic evidence must be
    // present or their weights train to exactly zero (a default engine would
    // starve both channels and ship a misleading artifact).
    let engine = TextIntelligence::new(EngineConfig {
        semantic: true,
        phonetic: true,
        ..EngineConfig::default()
    })
    .with_embedding_provider(
        FeatureHashEmbeddingProvider::new(256).map_err(|error| error.to_string())?,
    );

    println!("featurizing train split...");
    let train = featurize(&engine, &dataset, "train")?;
    println!("featurizing validation split...");
    let validation = featurize(&engine, &dataset, "validation")?;
    println!(
        "train cases: {} (positives: {})",
        train.labels.len(),
        train.labels.iter().filter(|label| **label).count()
    );

    // Class-balanced loss: the training split is ~85% positive while held-out
    // slices are closer to balanced, so uniform weighting would drag the
    // operating point toward always-positive (recall ~0.98, poor precision).
    let train_sample_weights = balanced_sample_weights(&train.labels);
    let validation_sample_weights = balanced_sample_weights(&validation.labels);
    let mut weights = vec![0.0; TRAINING_FEATURES.len()];
    let mut bias = 0.0;
    let mut loss = f64::INFINITY;
    for _ in 0..iterations {
        loss = logistic_step_weighted(
            &train.features,
            &train.labels,
            &mut weights,
            &mut bias,
            learning_rate,
            l2,
            &train_sample_weights,
        );
    }
    println!("balanced train loss after {iterations} iterations: {loss:.4}");

    // Calibration on validation: freeze weights, fit the bias only. The
    // scratch copy absorbs the weight update and is discarded.
    for _ in 0..CALIBRATION_ITERATIONS {
        let mut scratch = weights.clone();
        logistic_step_weighted(
            &validation.features,
            &validation.labels,
            &mut scratch,
            &mut bias,
            learning_rate,
            0.0,
            &validation_sample_weights,
        );
    }
    println!("bias after validation calibration: {bias:.4}");

    let names: BTreeMap<String, f64> = TRAINING_FEATURES
        .iter()
        .zip(weights.iter())
        .map(|(name, weight)| ((*name).to_string(), *weight))
        .collect();
    println!("weights:");
    for name in TRAINING_FEATURES {
        println!("  {name} = {:.4}", names.get(*name).copied().unwrap_or(0.0));
    }
    let scorer = LogisticSimilarityScorer::new(names.clone(), bias);
    let mut metrics = BTreeMap::new();
    for split in ["train", "validation", "test"] {
        let split_metrics = metrics_at(&scorer, &engine, &dataset, split)?;
        println!(
            "{split}: accuracy={:.3} f1={:.3} brier={:.3} roc_auc={:.3} pr_auc={:.3} ece={:.3} best_threshold={:.3}",
            split_metrics["accuracy"],
            split_metrics["f1"],
            split_metrics["brier"],
            split_metrics["roc_auc"],
            split_metrics["pr_auc"],
            split_metrics["ece"],
            split_metrics["best_threshold"],
        );
        for (key, value) in split_metrics {
            metrics.insert(format!("{split}_{key}"), value);
        }
    }
    let artifact = SimilarityModelArtifact::new(dataset.version.clone(), names, bias)
        .with_revision(format!("train-{iterations}-iter-ds{}", dataset.version))
        .with_metrics(metrics);
    if let Some(parent) = std::path::Path::new(&output).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(
        &output,
        artifact.to_json().map_err(|error| error.to_string())?,
    )?;
    println!("wrote {output}");
    Ok(())
}

// ---------------------------------------------------------------------------
// Spam training on seeded synthetic data.
//
// The evaluation set carries only 22 spam labels, far too few to fit 15
// weights. The tool therefore trains on a generated ham/spam corpus (fixed
// seed, disjoint from the evaluation texts) and measures generalization on
// the evaluation spam labels as held-out data, never training on them.
// ---------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound.max(1) as u64) as usize
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }

    fn pick<'a>(&mut self, options: &'a [&'a str]) -> &'a str {
        options[self.below(options.len())]
    }
}

const HAM_TEMPLATES: &[&str] = &[
    "running {num} minutes late, please start without me",
    "can you send the slides before friday morning",
    "gracias por tu ayuda con la mudanza del sábado",
    "the meeting moved to room {num}, see you there",
    "reminder: dentist appointment at {time} tomorrow",
    "nos vemos en el café de la esquina a las {time}",
    "could you review the attached notes when you have a moment",
    "happy to confirm lunch on {day}, my treat",
    "el informe ya está listo, lo revisamos el {day}",
    "don't forget to water the plants while we're away",
    "the train arrives at platform {num} around {time}",
    "¿puedes pasarme la receta de la tortilla por favor?",
    "thanks for covering my shift, I owe you coffee",
    "pick up milk, bread and cheese on your way home",
    "la reunión se pospone para la próxima semana",
    "your library books are due back on {day}",
    "meeting notes from yesterday are in the shared folder",
    "¿a qué hora sale tu vuelo el {day}?",
    "the kids have football practice at {time} today",
    "please confirm your attendance for the workshop",
    "he dejado las llaves en el cajón de la entrada",
    "the plumber will come between {time} and noon",
    "¿traes postre para la cena del {day}?",
    "invoice {num} has been paid, thank you",
];

const SPAM_TEMPLATES: &[&str] = &[
    "CONGRATULATIONS!!! You WON a FREE {gift}!!! Claim NOW at {url}",
    "ganaste un {gift}, reclama tu premio en {url}!!!",
    "URGENT: your account will be SUSPENDED, verify now at {url}",
    "{emoji} {emoji} MAKE MONEY FAST {emoji} no experience, write to {email}",
    "limited offer: DOUBLE your crypto, send to wallet {num}XqZ",
    "has ganado {num} euros, confirma tus datos en {url}",
    "FREE {gift} for the first {num} callers!!! dial now",
    "tu paquete está retenido, paga la tasa aquí: {url}",
    "work from home, earn ${num} daily!!! contact {email}",
    "AVISO FINAL: deuda pendiente de {num} euros, regulariza en {url}",
    "hot singles in your area want to meet YOU {url} {emoji}",
    "gana dinero rápido sin esfuerzo, escríbenos a {email}",
    "your invoice is overdue, pay immediately at {url} to avoid fees",
    "FELICIDADES!!! fuiste seleccionado para un {gift} gratis {url}",
    "miracle cure doctors HATE!!! order at {url} {emoji} {emoji}",
    "préstamo aprobado por {num} euros sin aval, responde a {email}",
    "you inherited ${num} from a distant relative, claim at {url}",
    "SUPER DESCUENTO {num}% solo hoy en {url} {emoji}",
    "your password expires TODAY, reset it here: {url}",
    "contraseña caducada, restablécela aquí: {url} {emoji}",
    "earn {num} euros per week stuffing envelopes, write {email}",
    "CLICK NOW {url} or lose your {gift} forever!!!",
    " bloomberg alert: pump incoming, join {url} {emoji}",
    "OFERTA ÚNICA: {gift} gratis para los {num} primeros {url}",
];

const URLS: &[&str] = &[
    "http://bit.ly/9x2kq",
    "https://free-prize-claim.example.com/win",
    "http://tinyurl.example.net/a1b2",
    "https://winner-notify.example.org/collect",
];

const EMAILS: &[&str] = &[
    "claim@fast-cash.example.com",
    "prizes@mega-win.example.net",
    "ofertas@premio-facil.example.org",
];

const EMOJI: &[&str] = &["💰", "🔥", "🎁", "💸", "⚡", "💵"];
const GIFTS: &[&str] = &["iPhone", "prize", "gift card", "cruise", "laptop", "premio"];
const DAYS: &[&str] = &["monday", "tuesday", "wednesday", "el lunes", "el viernes"];
const TIMES: &[&str] = &["9:30", "14:00", "18:45", "10:15"];

fn fill(template: &str, rng: &mut Rng) -> String {
    template
        .replace("{url}", rng.pick(URLS))
        .replace("{email}", rng.pick(EMAILS))
        .replace("{emoji}", rng.pick(EMOJI))
        .replace("{gift}", rng.pick(GIFTS))
        .replace("{num}", &rng.below(900).saturating_add(10).to_string())
        .replace("{day}", rng.pick(DAYS))
        .replace("{time}", rng.pick(TIMES))
}

fn leet(text: &str) -> String {
    text.chars()
        .map(|character| match character {
            'a' | 'A' => '4',
            'e' | 'E' => '3',
            'i' | 'I' => '1',
            'o' | 'O' => '0',
            's' | 'S' => '5',
            other => other,
        })
        .collect()
}

fn spam_variant(template: &str, rng: &mut Rng) -> String {
    let mut text = fill(template, rng);
    if rng.chance(30) {
        text = leet(&text);
    }
    if rng.chance(45) {
        text.push_str("!!!");
    }
    if rng.chance(20) {
        text = text.to_uppercase();
    }
    text
}

fn generate_corpus(count: usize, seed: u64) -> Vec<(String, bool)> {
    let mut rng = Rng(seed);
    let mut corpus = Vec::with_capacity(count * 2);
    for _ in 0..count {
        corpus.push((fill(rng.pick(HAM_TEMPLATES), &mut rng), false));
        corpus.push((spam_variant(rng.pick(SPAM_TEMPLATES), &mut rng), true));
    }
    // Seeded Fisher-Yates shuffle.
    for index in (1..corpus.len()).rev() {
        let other = rng.below(index + 1);
        corpus.swap(index, other);
    }
    corpus
}

fn run_spam(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    use textintel::{spam_feature_vector, spam_features, SpamModelArtifact, SPAM_FEATURES};

    let output =
        flag_value(args, "--output").ok_or("spam training requires --output <artifact.json>")?;
    let count = flag_value(args, "--count")
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(|_| "--count must be a positive integer")?
        .unwrap_or(700)
        .max(10);
    let seed = flag_value(args, "--seed")
        .map(|value| value.parse::<u64>())
        .transpose()
        .map_err(|_| "--seed must be a non-negative integer")?
        .unwrap_or(0xC0FFEE);

    let engine = TextIntelligence::new(EngineConfig::default());
    println!("generating {count} ham + {count} spam messages (seed {seed})...");
    let corpus = generate_corpus(count, seed);
    let mut rows = Vec::with_capacity(corpus.len());
    for (text, label) in &corpus {
        let fingerprint = engine.analyze(text)?;
        let patterns = engine.match_patterns(text)?;
        rows.push((
            spam_feature_vector(&spam_features(&fingerprint, &patterns)),
            *label,
        ));
    }
    let cut = rows.len() * 4 / 5;
    let (train, validation) = rows.split_at(cut);
    let split = |rows: &[(Vec<f64>, bool)]| {
        (
            rows.iter().map(|row| row.0.clone()).collect::<Vec<_>>(),
            rows.iter().map(|row| row.1).collect::<Vec<_>>(),
        )
    };
    let (train_features, train_labels) = split(train);
    let (validation_features, validation_labels) = split(validation);

    let mut weights = vec![0.0; SPAM_FEATURES.len()];
    let mut bias = 0.0;
    let mut loss = f64::INFINITY;
    for _ in 0..5000 {
        loss = logistic_step(
            &train_features,
            &train_labels,
            &mut weights,
            &mut bias,
            0.2,
            1e-3,
        );
    }
    println!("train loss: {loss:.4}");
    // Calibration on validation: freeze weights, fit the bias only (same
    // pattern as the similarity trainer). The scratch copy absorbs the
    // weight update and is discarded. Only artifacts produced by this step
    // may carry `calibrated: true`.
    for _ in 0..500 {
        let mut scratch = weights.clone();
        logistic_step(
            &validation_features,
            &validation_labels,
            &mut scratch,
            &mut bias,
            0.2,
            0.0,
        );
    }

    let names: BTreeMap<String, f64> = SPAM_FEATURES
        .iter()
        .zip(weights.iter())
        .map(|(name, weight)| ((*name).to_string(), *weight))
        .collect();
    let mut metrics = BTreeMap::new();
    for (split_name, features, labels) in [
        ("train", &train_features, &train_labels),
        ("validation", &validation_features, &validation_labels),
    ] {
        let split_metrics = logistic_report(&weights, bias, features, labels);
        println!(
            "{split_name}: accuracy={:.3} f1={:.3} brier={:.3} roc_auc={:.3} pr_auc={:.3} ece={:.3}",
            split_metrics["accuracy"],
            split_metrics["f1"],
            split_metrics["brier"],
            split_metrics["roc_auc"],
            split_metrics["pr_auc"],
            split_metrics["ece"],
        );
        for (key, value) in split_metrics {
            metrics.insert(format!("{split_name}_{key}"), value);
        }
    }

    // Held-out generalization: evaluation spam labels are measured, never
    // trained on.
    let source = std::fs::read_to_string("data/evaluation.json")?;
    let dataset = EvaluationDataset::from_json(&source)?;
    // Both messages of each pair count: the evaluation harness compares
    // pairs, so score `a` and `b` texts for a fair reading.
    let mut pair_rows = Vec::new();
    for case in &dataset.cases {
        let Some(label) = case.labels.get("spam").copied() else {
            continue;
        };
        for text in [&case.a, &case.b] {
            let fingerprint = engine.analyze(text)?;
            let patterns = engine.match_patterns(text)?;
            pair_rows.push((
                spam_feature_vector(&spam_features(&fingerprint, &patterns)),
                label,
            ));
        }
    }
    let eval_metrics = logistic_report(
        &weights,
        bias,
        &pair_rows
            .iter()
            .map(|row| row.0.clone())
            .collect::<Vec<_>>(),
        &pair_rows.iter().map(|row| row.1).collect::<Vec<_>>(),
    );
    println!(
        "heldout-eval: accuracy={:.3} f1={:.3} brier={:.3} roc_auc={:.3} pr_auc={:.3} ece={:.3} (n={})",
        eval_metrics["accuracy"],
        eval_metrics["f1"],
        eval_metrics["brier"],
        eval_metrics["roc_auc"],
        eval_metrics["pr_auc"],
        eval_metrics["ece"],
        pair_rows.len(),
    );
    for (key, value) in eval_metrics {
        metrics.insert(format!("heldout_eval_{key}"), value);
    }

    // Operating point from held-out validation data (Youden's J), never from
    // train. Inference labels `probability >= decision_threshold` as spam.
    let decision_threshold = metrics
        .get("validation_best_threshold")
        .copied()
        .filter(|value| value.is_finite())
        .map(|value| value.clamp(0.0, 1.0))
        .unwrap_or(0.5);
    println!("decision threshold from validation: {decision_threshold:.3}");
    let artifact = SpamModelArtifact::new("synthetic-spam-v1", names, bias)
        .with_revision(format!("synth-{count}-{seed}"))
        .with_calibrated(true)
        .with_decision_threshold(decision_threshold)
        .with_metrics(metrics);
    if let Some(parent) = std::path::Path::new(&output).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(
        &output,
        artifact.to_json().map_err(|error| error.to_string())?,
    )?;
    println!("wrote {output}");
    Ok(())
}

/// Accuracy/F1/Brier/ROC for a fixed weight vector (shared by the similarity
/// and spam trainers).
fn logistic_report(
    weights: &[f64],
    bias: f64,
    features: &[Vec<f64>],
    labels: &[bool],
) -> BTreeMap<String, f64> {
    use textintel::sigmoid;

    let samples: Vec<(f64, bool)> = features
        .iter()
        .zip(labels.iter())
        .map(|(row, label)| {
            let logit = row
                .iter()
                .zip(weights.iter())
                .map(|(value, weight)| value * weight)
                .sum::<f64>()
                + bias;
            (sigmoid(logit), *label)
        })
        .collect();
    report(&samples)
}
