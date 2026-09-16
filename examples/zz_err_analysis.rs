//! TEMPORARY error-analysis probe (delete before release).
use textintel::evaluation::EvaluationDataset;
use textintel::{
    language_agreement, training_features, EngineConfig, TextIntelligence, TRAINING_FEATURES,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let split = std::env::args().nth(1).unwrap_or("validation".to_string());
    let dataset = EvaluationDataset::from_dir("data/evaluation")?;
    let mut engine = TextIntelligence::builder()
        .config(EngineConfig::default())
        .production_local()
        .build()
        .map_err(|e| e.to_string())?;
    if let Ok(path) = std::env::var("SCORER") {
        let source = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let artifact =
            textintel::SimilarityModelArtifact::from_json(&source).map_err(|e| e.to_string())?;
        engine = engine.with_similarity_scorer(artifact.to_scorer());
    }
    let cases = dataset.filter_split(&split);
    let mut rows: Vec<(f64, bool, String, String, String, String)> = Vec::new();
    let mut feat_dump: Vec<(String, Vec<f64>)> = Vec::new();
    for case in &cases {
        let left = engine.analyze(&case.a)?;
        let right = engine.analyze(&case.b)?;
        let cmp = engine.compare_fingerprints(&left, &right);
        let agree = language_agreement(&left, &right);
        let map = training_features(&cmp, agree);
        let feats: Vec<f64> = TRAINING_FEATURES
            .iter()
            .map(|n| map.get(*n).copied().unwrap_or(0.0))
            .collect();
        rows.push((
            cmp.score,
            case.is_similar(),
            case.id.clone(),
            case.primary_category().to_string(),
            case.a.clone(),
            case.b.clone(),
        ));
        feat_dump.push((case.id.clone(), feats));
    }
    let feats_of = |id: &str| {
        feat_dump
            .iter()
            .find(|(i, _)| i == id)
            .map(|(_, f)| f.clone())
            .unwrap()
    };
    let show_feats = |id: &str| {
        let f = feats_of(id);
        TRAINING_FEATURES
            .iter()
            .zip(f.iter())
            .filter(|(_, v)| **v > 0.01)
            .map(|(n, v)| format!("{n}={v:.2}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    // Overall + per-category accuracy at 0.5.
    let mut cat_tot = std::collections::BTreeMap::new();
    let mut cat_ok = std::collections::BTreeMap::new();
    let mut tp = 0;
    let mut pos = 0;
    for (s, lab, _, cat, _, _) in &rows {
        *cat_tot.entry(cat.clone()).or_insert(0) += 1;
        let pred = *s >= 0.5;
        if pred == *lab {
            *cat_ok.entry(cat.clone()).or_insert(0) += 1;
        }
        if *lab {
            pos += 1;
            if pred {
                tp += 1;
            }
        }
    }
    let pred_p = rows.iter().filter(|(s, _, _, _, _, _)| *s >= 0.5).count();
    let prec = tp as f64 / pred_p.max(1) as f64;
    let rec = tp as f64 / pos.max(1) as f64;
    println!(
        "split={split} n={} f1={:.3} prec={:.3} rec={:.3}",
        rows.len(),
        2.0 * prec * rec / (prec + rec).max(1e-9),
        prec,
        rec
    );
    for (c, t) in &cat_tot {
        println!(
            "  cat {c}: acc={:.3} (n={t})",
            *cat_ok.get(c).unwrap_or(&0) as f64 / *t as f64
        );
    }
    println!("--- FALSE NEGATIVES (similar, score<0.5, highest first) ---");
    let mut fns: Vec<_> = rows
        .iter()
        .filter(|(s, l, _, _, _, _)| *l && *s < 0.5)
        .collect();
    fns.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (s, _, id, cat, a, b) in fns.iter().take(30) {
        println!(
            "{s:.3} {id} [{cat}] {a:?} <-> {b:?}\n    {}",
            show_feats(id)
        );
    }
    println!("--- FALSE POSITIVES (dissimilar, score>=0.5, lowest first) ---");
    let mut fps: Vec<_> = rows
        .iter()
        .filter(|(s, l, _, _, _, _)| !*l && *s >= 0.5)
        .collect();
    fps.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (s, _, id, cat, a, b) in fps.iter().take(30) {
        println!(
            "{s:.3} {id} [{cat}] {a:?} <-> {b:?}\n    {}",
            show_feats(id)
        );
    }
    Ok(())
}
