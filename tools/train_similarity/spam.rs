//! Logistic spam-model training on seeded synthetic data.

use std::collections::BTreeMap;

use textintel::evaluation::EvaluationDataset;
use textintel::{logistic_step, EngineConfig, TextIntelligence};

use super::flag_value;
use super::metrics::logistic_report;

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

#[derive(Debug, serde::Deserialize)]
struct CorpusFile {
    #[serde(default)]
    version: String,
    items: Vec<CorpusItem>,
}

#[derive(Debug, serde::Deserialize)]
struct CorpusItem {
    text: String,
    label: String,
}

pub(crate) fn run_spam(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
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
    let corpus_path = flag_value(args, "--corpus");

    let engine = TextIntelligence::new(EngineConfig::default());
    let corpus_version: Option<String>;
    let corpus: Vec<(String, bool)> = match corpus_path.as_deref() {
        Some(path) => {
            let source = std::fs::read_to_string(path)
                .map_err(|error| format!("read spam corpus {path}: {error}"))?;
            let file: CorpusFile = serde_json::from_str(&source)
                .map_err(|error| format!("parse spam corpus {path}: {error}"))?;
            if file.items.is_empty() {
                return Err("spam corpus has no items".into());
            }
            let mut loaded = Vec::with_capacity(file.items.len());
            for (index, item) in file.items.iter().enumerate() {
                if item.text.trim().is_empty() {
                    return Err(format!("spam corpus item {index} has empty text").into());
                }
                let label = match item.label.to_ascii_lowercase().as_str() {
                    "spam" => true,
                    "ham" | "benign" => false,
                    other => {
                        return Err(
                            format!("spam corpus item {index} has unknown label {other:?}").into(),
                        );
                    }
                };
                loaded.push((item.text.clone(), label));
            }
            if !loaded.iter().any(|(_, label)| *label)
                || !loaded.iter().any(|(_, label)| !*label)
            {
                return Err("spam corpus needs both spam and ham items".into());
            }
            // Seeded shuffle (same Fisher-Yates as the synthetic path) so
            // the 80/20 train/validation split below is deterministic.
            let mut rng = Rng(seed);
            for index in (1..loaded.len()).rev() {
                let other = rng.below(index + 1);
                loaded.swap(index, other);
            }
            corpus_version = if file.version.trim().is_empty() {
                None
            } else {
                Some(file.version.clone())
            };
            println!(
                "loaded {} spam + {} ham messages from {path} (version {}, seed {seed})...",
                loaded.iter().filter(|(_, label)| *label).count(),
                loaded.iter().filter(|(_, label)| !*label).count(),
                corpus_version.as_deref().unwrap_or("unversioned"),
            );
            loaded
        }
        None => {
            println!("generating {count} ham + {count} spam messages (seed {seed})...");
            corpus_version = None;
            generate_corpus(count, seed)
        }
    };
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

    // Held-out generalization: held-out spam labels are measured, never
    // trained on. Corpus mode scores the dedicated held-out spam corpus
    // (default `data/spam/v2-eval.json`, disjoint from the training
    // corpus); synthetic mode keeps the legacy reading over the evaluation
    // pairs' spam labels.
    let mut pair_rows = Vec::new();
    if corpus_path.is_some() {
        let heldout_path = flag_value(args, "--heldout")
            .unwrap_or_else(|| "data/spam/v2-eval.json".to_string());
        let source = std::fs::read_to_string(&heldout_path)
            .map_err(|error| format!("read held-out spam corpus {heldout_path}: {error}"))?;
        let file: CorpusFile = serde_json::from_str(&source)
            .map_err(|error| format!("parse held-out spam corpus {heldout_path}: {error}"))?;
        for item in &file.items {
            let label = match item.label.to_ascii_lowercase().as_str() {
                "spam" => true,
                "ham" | "benign" => false,
                other => {
                    return Err(
                        format!("held-out spam corpus has unknown label {other:?}").into(),
                    );
                }
            };
            let fingerprint = engine.analyze(&item.text)?;
            let patterns = engine.match_patterns(&item.text)?;
            pair_rows.push((
                spam_feature_vector(&spam_features(&fingerprint, &patterns)),
                label,
            ));
        }
    } else {
        let dataset = EvaluationDataset::load_path("data/evaluation")
            .map_err(|error| error.to_string())?;
        // Both messages of each pair count: the evaluation harness compares
        // pairs, so score `a` and `b` texts for a fair reading.
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
    let artifact = match (corpus_path.as_deref(), corpus_version) {
        (Some(path), Some(version)) => SpamModelArtifact::new(version.clone(), names, bias)
            .with_revision(format!(
                "corpus-{version}-{}-seed{seed}",
                std::path::Path::new(path)
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("corpus"),
            )),
        (Some(_), None) => SpamModelArtifact::new("unversioned-spam-corpus", names, bias)
            .with_revision(format!("corpus-seed{seed}")),
        (None, _) => SpamModelArtifact::new("synthetic-spam-v1", names, bias)
            .with_revision(format!("synth-{count}-{seed}")),
    }
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
