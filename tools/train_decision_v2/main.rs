//! Train a v2 interaction head over a frozen embedding backbone.
//!
//! Arguments are declared in [`textintel::cli::train_decision_spec`] (run
//! with `--help` for usage). `--train file@N` repeats a file N times
//! (upsampling for small tasks). Features are extracted once and cached
//! (see `--cache`); head training itself is deterministic and CPU-cheap.
//! The backbone stays frozen — only the ~200K-param head trains.

use std::collections::BTreeMap;

use textintel::TextIntelligence;
use textintel::core::parallel::map_chunks_ordered;
use textintel::core::providers::EmbeddingProvider;
use textintel::decision::{
    DecisionExample, FUSION_FEATURES, HeadTrainExample, HeadTrainer, InteractionArtifact,
    SplitMix64, fusion_feature_vector, head_loss_accuracy, init_head_xavier, interaction_features,
};

const CACHE_VERSION: &str = "interaction-features-v2";

fn load_examples(path: &str) -> Result<Vec<DecisionExample>, String> {
    let source =
        std::fs::read_to_string(path).map_err(|error| format!("cannot read {path}: {error}"))?;
    let mut examples = Vec::new();
    for (index, line) in source.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let example: DecisionExample = serde_json::from_str(line)
            .map_err(|error| format!("{path} line {}: {error}", index + 1))?;
        example
            .validate()
            .map_err(|message| format!("{path} line {}: {message}", index + 1))?;
        if !matches!(
            example.question,
            textintel::decision::DecisionQuestion::Choice { .. }
        ) {
            return Err(format!(
                "{path} line {}: only choice examples train the head",
                index + 1
            ));
        }
        examples.push(example);
    }
    Ok(examples)
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct FeatureCache {
    version: String,
    embedding_model: String,
    embedding_revision: String,
    embedding_dim: usize,
    fusion_features: usize,
    examples: Vec<CachedExample>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct CachedExample {
    id: String,
    candidates: Vec<Vec<f32>>,
    gold: usize,
}

fn cache_key(parts: &[String]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Content identity, including repeat counts. File edits invalidate caches.
fn source_digest(files: &[String]) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    for file in files {
        let path = file.split_once('@').map_or(file.as_str(), |(path, _)| path);
        let bytes = std::fs::read(path).map_err(|e| format!("cannot read {path}: {e}"))?;
        hash.update((file.len() as u64).to_le_bytes());
        hash.update(file.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(&bytes);
    }
    Ok(hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[derive(Clone, Copy)]
struct FeatureOptions<'a> {
    cache_dir: Option<&'a str>,
    jobs: usize,
    use_fusion: bool,
}

/// Extract (or load cached) head-training features for `examples`.
/// `jobs` shards the analysis/embedding chunk loops over worker
/// threads; chunks rejoin in order, so features are identical to the
/// sequential run. Values below 2 run sequentially.
fn featurize(
    engine: &TextIntelligence,
    embeddings: &dyn EmbeddingProvider,
    embedding_dim: usize,
    label: &str,
    files: &[String],
    options: FeatureOptions<'_>,
) -> Result<Vec<HeadTrainExample>, String> {
    let FeatureOptions {
        cache_dir,
        jobs,
        use_fusion,
    } = options;
    let fusion_dim = if use_fusion { FUSION_FEATURES.len() } else { 0 };
    let mut examples = Vec::new();
    for file in files {
        let (path, repeat) = match file.split_once('@') {
            Some((path, count)) => (
                path,
                count
                    .parse::<usize>()
                    .map_err(|_| format!("bad repeat in {file}"))?,
            ),
            None => (file.as_str(), 1),
        };
        let loaded = load_examples(path)?;
        println!("{label}: {path} x{repeat} ({} examples)", loaded.len());
        for _ in 0..repeat {
            examples.extend(loaded.iter().cloned());
        }
    }
    if examples.is_empty() {
        return Err(format!("{label}: no examples"));
    }
    // Cache identity covers the file list, the backbone, the dims, and
    // the engine's rebus toggle (skipped decoding changes fusion features).
    let no_rebus = !engine.config().rebus;
    let metadata = embeddings
        .model_metadata()
        .ok_or_else(|| "backbone has no metadata".to_string())?;
    let key = cache_key(&[
        CACHE_VERSION.to_string(),
        source_digest(files)?,
        textintel::resource_revision(&engine.resource_manifest()),
        textintel::FINGERPRINT_SCHEMA_VERSION.to_string(),
        metadata.model_id.clone(),
        metadata.revision.clone().unwrap_or_default(),
        embedding_dim.to_string(),
        format!("no_rebus={no_rebus},fusion={use_fusion}"),
    ]);
    if let Some(dir) = cache_dir {
        let path = std::path::Path::new(dir).join(format!("{label}-{key}.json"));
        if path.is_file() {
            let source = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
            let cache: FeatureCache =
                serde_json::from_str(&source).map_err(|error| format!("bad cache: {error}"))?;
            if cache.version == CACHE_VERSION
                && cache.embedding_dim == embedding_dim
                && cache.fusion_features == fusion_dim
                && cache.embedding_model == metadata.model_id
                && cache.embedding_revision == metadata.revision.clone().unwrap_or_default()
                && cache.examples.len() == examples.len()
            {
                println!(
                    "{label}: cache hit {} ({} examples)",
                    path.display(),
                    cache.examples.len()
                );
                return cache
                    .examples
                    .into_iter()
                    .map(|entry| HeadTrainExample::new(entry.candidates, entry.gold))
                    .collect::<Result<Vec<_>, _>>();
            }
            println!("{label}: cache miss (identity changed)");
        }
    }
    println!("{label}: extracting {} examples...", examples.len());
    // Unique texts first: descriptions repeat across examples of a task.
    let mut texts: Vec<String> = Vec::new();
    let mut index_of: BTreeMap<String, usize> = BTreeMap::new();
    let intern = |text: String, texts: &mut Vec<String>, index_of: &mut BTreeMap<String, usize>| {
        if let Some(position) = index_of.get(&text) {
            return *position;
        }
        let position = texts.len();
        index_of.insert(text.clone(), texts.len());
        texts.push(text);
        position
    };
    type CandidateRef = (String, usize);
    type ExampleLayout = (usize, Vec<CandidateRef>, usize);
    let mut layout: Vec<ExampleLayout> = Vec::with_capacity(examples.len());
    for example in &examples {
        let state = intern(example.state.clone(), &mut texts, &mut index_of);
        let textintel::decision::DecisionQuestion::Choice { criteria, .. } = &example.question
        else {
            unreachable!("validated above");
        };
        let mut ids: Vec<&String> = criteria.keys().collect();
        ids.sort_unstable();
        let mut candidates = Vec::with_capacity(ids.len());
        for id in &ids {
            candidates.push((
                (*id).clone(),
                intern(criteria[*id].clone(), &mut texts, &mut index_of),
            ));
        }
        let gold = example
            .gold_index()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "unknown gold label".to_string())?;
        layout.push((state, candidates, gold));
    }
    println!("{label}: {} unique texts", texts.len());
    // `analyze_batch` enforces `max_batch_size`: chunk large corpora.
    let fingerprints = if use_fusion {
        map_chunks_ordered(&texts, 256, jobs, |chunk| {
            engine
                .analyze_batch(chunk)
                .map_err(|error| format!("analysis failed: {error}"))
        })?
    } else {
        Vec::new()
    };
    // Embed in chunks to keep provider calls bounded.
    let vectors = map_chunks_ordered(&texts, 64, jobs, |chunk| {
        embeddings
            .embed(chunk)
            .map_err(|error| format!("embedding failed: {error}"))
    })?;
    if vectors.len() != texts.len() {
        return Err("backbone returned a short embedding batch".to_string());
    }
    let mut cached = Vec::with_capacity(layout.len());
    for (position, (state, candidates, gold)) in layout.iter().enumerate() {
        let fusion = use_fusion.then(|| fusion_feature_vector(&fingerprints[*state]));
        let mut rows = Vec::with_capacity(candidates.len());
        for (_id, text) in candidates {
            rows.push(interaction_features(
                &vectors[*state],
                &vectors[*text],
                fusion.as_deref(),
            )?);
        }
        let example = HeadTrainExample::new(rows, *gold)?;
        cached.push(CachedExample {
            id: format!("{label}-{position}"),
            candidates: example.candidates.clone(),
            gold: example.gold,
        });
    }
    println!("{label}: extracted {} examples", cached.len());
    if let Some(dir) = cache_dir {
        std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        let path = std::path::Path::new(dir).join(format!("{label}-{key}.json"));
        let cache = FeatureCache {
            version: CACHE_VERSION.to_string(),
            embedding_model: metadata.model_id,
            embedding_revision: metadata.revision.unwrap_or_default(),
            embedding_dim,
            fusion_features: fusion_dim,
            examples: cached.clone(),
        };
        std::fs::write(
            &path,
            serde_json::to_string(&cache).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        println!("{label}: cached {}", path.display());
    }
    cached
        .into_iter()
        .map(|entry| HeadTrainExample::new(entry.candidates, entry.gold))
        .collect::<Result<Vec<_>, _>>()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let spec = textintel::cli::train_decision_spec();
    if let Some(text) = textintel::cli::handle_meta(spec, env!("CARGO_PKG_VERSION"), &argv) {
        println!("{text}");
        return Ok(());
    }
    let parsed = match textintel::cli::parse_args(spec, &argv) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("{error}");
            eprintln!("Run `{} --help` for usage.", spec.name);
            std::process::exit(error.exit_code());
        }
    };
    let embeddings_dir = parsed.required_value("embeddings")?;
    let train_files: Vec<String> = parsed
        .values_of("train")
        .into_iter()
        .map(str::to_string)
        .collect();
    let valid_files: Vec<String> = parsed
        .values_of("valid")
        .into_iter()
        .map(str::to_string)
        .collect();
    let out = parsed.required_value("out")?;
    let get = |id: &str| parsed.value(id).map(str::to_string);
    let hidden: usize = get("hidden").map_or(Ok(128), |value| value.parse())?;
    let learning_rate: f32 = get("lr").map_or(Ok(0.001), |value| value.parse())?;
    let batch_size: usize = get("batch").map_or(Ok(32), |value| value.parse())?;
    let epochs: usize = get("epochs").map_or(Ok(20), |value| value.parse())?;
    let patience: usize = get("patience").map_or(Ok(4), |value| value.parse())?;
    let seed: u64 = get("seed").map_or(Ok(7), |value| value.parse())?;
    let max_train: usize = get("max-train").map_or(Ok(0), |value| value.parse())?;
    let cache_dir = get("cache");
    let jobs: usize = get("jobs").map_or(Ok(1), |value| value.parse())?;
    let approach = parsed.value("approach").unwrap_or("interaction");
    if !matches!(approach, "interaction" | "prototype") {
        return Err("--approach must be interaction or prototype".into());
    }
    let no_rebus = parsed.flag("no-rebus") || approach == "prototype";
    let use_fusion = !parsed.flag("no-fusion") && approach != "prototype";

    let backbone = textintel::decision::open_decision_embeddings(embeddings_dir)?;
    let metadata = backbone
        .model_metadata()
        .ok_or("backbone has no metadata")?;
    println!(
        "backbone: {}@{} dim={} normalized={}",
        metadata.model_id,
        metadata.revision.as_deref().unwrap_or("?"),
        metadata.dimensions,
        metadata.normalized
    );
    let mut engine_config = textintel::EngineConfig::default();
    if no_rebus {
        engine_config.rebus = false;
    }
    let engine = TextIntelligence::new(engine_config).with_embedding_provider(backbone.clone());
    let train = featurize(
        &engine,
        backbone.as_ref(),
        metadata.dimensions,
        "train",
        &train_files,
        FeatureOptions {
            cache_dir: cache_dir.as_deref(),
            jobs,
            use_fusion,
        },
    )
    .map_err(|error| error.to_string())?;
    let valid = featurize(
        &engine,
        backbone.as_ref(),
        metadata.dimensions,
        "valid",
        &valid_files,
        FeatureOptions {
            cache_dir: cache_dir.as_deref(),
            jobs,
            use_fusion,
        },
    )
    .map_err(|error| error.to_string())?;
    if approach == "prototype" {
        let blend: f32 = parsed.value("prototype-blend").unwrap_or("0.5").parse()?;
        let mut criteria = None;
        for file in train_files.iter().chain(&valid_files) {
            let path = file.split_once('@').map_or(file.as_str(), |(path, _)| path);
            for example in load_examples(path)? {
                let textintel::decision::DecisionQuestion::Choice {
                    criteria: current, ..
                } = example.question
                else {
                    unreachable!()
                };
                if let Some(expected) = &criteria {
                    if expected != &current {
                        return Err(
                            "prototype training requires one fixed criterion map across splits"
                                .into(),
                        );
                    }
                } else {
                    criteria = Some(current);
                }
            }
        }
        let mut artifact = textintel::decision::PrototypeArtifact::fit(
            metadata,
            criteria.ok_or("prototype task has no criteria")?,
            &train,
            &valid,
            blend,
            format!("train-{}", source_digest(&train_files)?),
        )?;
        artifact.training_config.extend(BTreeMap::from([
            ("train_sources".into(), train_files.join(",")),
            ("valid_sources".into(), valid_files.join(",")),
            ("train_digest".into(), source_digest(&train_files)?),
            ("valid_digest".into(), source_digest(&valid_files)?),
            ("fusion".into(), "false".into()),
            ("rebus".into(), "false".into()),
        ]));
        artifact.revision = Some(format!(
            "prototype-{}",
            cache_key(&[serde_json::to_string(&artifact)?])
        ));
        if let Some(parent) = std::path::Path::new(out).parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(out, serde_json::to_string(&artifact)?)?;
        println!(
            "wrote prototype {out} (blend={blend}, temperature={})",
            artifact.temperature
        );
        return Ok(());
    }
    let mut train = train;
    if max_train > 0 && train.len() > max_train {
        train.truncate(max_train);
    }
    let input_dim = train[0].candidates[0].len();
    println!(
        "input_dim={input_dim} hidden={hidden} train={} valid={}",
        train.len(),
        valid.len()
    );
    if hidden == 0 || epochs == 0 || patience == 0 || batch_size == 0 {
        return Err("hidden, epochs, patience and batch must be positive".into());
    }
    let mut head = init_head_xavier(input_dim, hidden, seed).map_err(|error| error.to_string())?;
    println!("head params: {}", head.parameter_count());
    let mut trainer =
        HeadTrainer::new(&head, learning_rate, batch_size).map_err(|error| error.to_string())?;

    let mut best = head.clone();
    let mut best_valid = f64::INFINITY;
    let mut waited = 0usize;
    let mut epochs_run = 0usize;
    let mut order: Vec<usize> = (0..train.len()).collect();
    for epoch in 1..=epochs {
        epochs_run = epoch;
        SplitMix64::new(seed + epoch as u64).shuffle(&mut order);
        let mut loss_sum = 0.0;
        let mut batches = 0usize;
        for chunk in order.chunks(batch_size) {
            loss_sum += trainer
                .step_indexed(&mut head, &train, chunk)
                .map_err(|error| error.to_string())?;
            batches += 1;
        }
        let (valid_loss, valid_acc) =
            head_loss_accuracy(&head, &valid).map_err(|error| error.to_string())?;
        println!(
            "epoch={epoch} train_loss={:.4} valid_loss={valid_loss:.4} valid_acc={valid_acc:.4}",
            loss_sum / batches.max(1) as f64
        );
        if valid_loss < best_valid {
            best_valid = valid_loss;
            best = head.clone();
            waited = 0;
        } else {
            waited += 1;
            if waited >= patience {
                println!("early stop at epoch {epoch} (best valid_loss={best_valid:.4})");
                break;
            }
        }
    }
    let (train_loss, train_acc) =
        head_loss_accuracy(&best, &train).map_err(|error| error.to_string())?;
    let (valid_loss, valid_acc) =
        head_loss_accuracy(&best, &valid).map_err(|error| error.to_string())?;
    println!(
        "final: train_loss={train_loss:.4} train_acc={train_acc:.4} valid_loss={valid_loss:.4} valid_acc={valid_acc:.4}"
    );
    let mut artifact = InteractionArtifact::from_head(
        &best,
        metadata.dimensions,
        if use_fusion { FUSION_FEATURES.len() } else { 0 },
        format!("train-{}", source_digest(&train_files)?),
    )
    .map_err(|error| error.to_string())?;
    artifact.metrics = BTreeMap::from([
        ("train_loss".to_string(), train_loss),
        ("train_acc".to_string(), train_acc),
        ("valid_loss".to_string(), valid_loss),
        ("valid_acc".to_string(), valid_acc),
    ]);
    let samples = valid
        .iter()
        .map(|example| {
            let logits = example
                .candidates
                .iter()
                .map(|features| best.forward(features).map(f64::from))
                .collect::<Result<Vec<_>, _>>()?;
            textintel::decision::CalibrationSample::new(logits, example.gold)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let fitted = textintel::decision::fit_temperature(&samples)?;
    artifact.temperature = fitted.temperature;
    artifact
        .metrics
        .insert("valid_nll_calibrated".to_string(), fitted.nll_after);
    artifact.embedding_model = Some(metadata.clone());
    artifact.training_config = BTreeMap::from([
        ("seed".into(), seed.to_string()),
        ("learning_rate".into(), learning_rate.to_string()),
        ("batch".into(), batch_size.to_string()),
        ("epochs_run".into(), epochs_run.to_string()),
        ("train_sources".into(), train_files.join(",")),
        ("valid_sources".into(), valid_files.join(",")),
        ("train_digest".into(), source_digest(&train_files)?),
        ("valid_digest".into(), source_digest(&valid_files)?),
        ("rebus".into(), (!no_rebus).to_string()),
        ("fusion".into(), use_fusion.to_string()),
        ("cache_version".into(), CACHE_VERSION.to_string()),
    ]);
    artifact.revision = Some(format!("interaction-{}", cache_key(&[artifact.to_json()?])));
    println!(
        "temperature={:.4} valid_nll_before={:.4} valid_nll_calibrated={:.4}",
        fitted.temperature, fitted.nll_before, fitted.nll_after
    );
    if let Some(parent) = std::path::Path::new(&out).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(out, artifact.to_json().map_err(|error| error.to_string())?)?;
    println!("wrote {out}");
    Ok(())
}
