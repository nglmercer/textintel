//! TEMPORARY ranking-analysis probe (delete before release).
use std::collections::BTreeSet;
use textintel::evaluation::EvaluationDataset;
use textintel::{EngineConfig, TextIntelligence};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dataset = EvaluationDataset::from_dir("data/evaluation")?;
    let engine = TextIntelligence::builder()
        .config(EngineConfig::default())
        .production_local()
        .build()
        .map_err(|e| e.to_string())?;
    let cases = dataset.filter_split("test");
    let mut documents: Vec<String> = Vec::new();
    let mut seen = BTreeSet::new();
    for case in &cases {
        if documents.len() >= 500 {
            break;
        }
        if seen.insert(case.b.clone()) {
            documents.push(case.b.clone());
        }
    }
    let mut doc_fps = Vec::with_capacity(documents.len());
    for chunk in documents.chunks(200) {
        doc_fps.extend(engine.analyze_batch(chunk)?);
    }
    let offset: usize = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let limit: usize = std::env::args()
        .nth(2)
        .and_then(|value| value.parse().ok())
        .unwrap_or(100);
    let queries: Vec<&&textintel::evaluation::EvaluationCase> =
        cases.iter().skip(offset).take(limit).collect();
    let mut misses = 0;
    let mut reciprocal_sum = 0.0;
    let mut relevant = 0usize;
    let mut recall_at_10 = 0usize;
    for q in &queries {
        if !q.labels.get("similar").copied().unwrap_or(false) {
            continue;
        }
        let qfp = engine.analyze(&q.a)?;
        let mut scored: Vec<(usize, f64)> = doc_fps
            .iter()
            .enumerate()
            .map(|(i, d)| (i, engine.compare_fingerprints(&qfp, d).score))
            .collect();
        scored.sort_by(|l, r| {
            r.1.total_cmp(&l.1)
                .then_with(|| documents[l.0].cmp(&documents[r.0]))
        });
        let rank = scored.iter().position(|(i, _)| {
            textintel::normalization::unicode::casefold_text(&documents[*i]).trim()
                == textintel::normalization::unicode::casefold_text(&q.b).trim()
        });
        relevant += 1;
        match rank {
            Some(position) => {
                reciprocal_sum += 1.0 / (position + 1) as f64;
                if position < 10 {
                    recall_at_10 += 1;
                }
            }
            None => {}
        }
        match rank {
            Some(0) => {}
            _ => {
                misses += 1;
                let own = scored
                    .iter()
                    .find(|(i, _)| documents[*i] == q.b)
                    .map(|(_, s)| *s)
                    .unwrap_or(-1.0);
                println!(
                    "rank={:?} own={:.3} id={} cat={} q={:?} t={:?}",
                    rank.map(|r| r + 1),
                    own,
                    q.id,
                    q.primary_category(),
                    q.a,
                    q.b
                );
                for (i, s) in scored.iter().take(3) {
                    println!("    top: {s:.3} {:?}", documents[*i]);
                }
            }
        }
    }
    println!(
        "misses={misses} relevant={relevant} mrr={:.3} recall_at_10={:.3}",
        if relevant == 0 {
            0.0
        } else {
            reciprocal_sum / relevant as f64
        },
        if relevant == 0 {
            0.0
        } else {
            recall_at_10 as f64 / relevant as f64
        }
    );
    Ok(())
}
