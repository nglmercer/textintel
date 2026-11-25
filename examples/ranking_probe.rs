//! Development probe: replicate the retrieval-ranking evaluation per query
//! and report, for each failing query, which irrelevant documents outrank
//! the first relevant one — under both the plain weighted scorer and the
//! decision layer. Not part of the public API surface.

use std::collections::{BTreeMap, BTreeSet};

use textintel::evaluation::EvaluationDataset;
use textintel::{EngineConfig, TextIntelligence};

fn same_text(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dataset = EvaluationDataset::load_path("data/evaluation")?;
    let cases: Vec<_> = dataset
        .cases
        .iter()
        .filter(|case| case.split == "test")
        .collect();
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
    let mut groups: Vec<(&str, Vec<&str>)> = Vec::new();
    let mut group_index: BTreeMap<&str, usize> = BTreeMap::new();
    for case in &cases {
        let position = *group_index.entry(case.a.as_str()).or_insert_with(|| {
            groups.push((case.a.as_str(), Vec::new()));
            groups.len() - 1
        });
        if case.is_similar() {
            groups[position].1.push(case.b.as_str());
        }
    }
    let indexed = |text: &str| documents.iter().any(|doc| same_text(doc, text));
    let queries: Vec<(&str, Vec<&str>)> = groups
        .into_iter()
        .filter(|(_, relevant)| !relevant.is_empty())
        .take(100)
        .filter(|(_, relevant)| relevant.iter().any(|target| indexed(target)))
        .collect();

    let engine = TextIntelligence::new(EngineConfig::default());
    let mut doc_fingerprints = Vec::with_capacity(documents.len());
    for chunk in documents.chunks(engine.config().max_batch_size.max(1)) {
        doc_fingerprints.extend(engine.analyze_batch(chunk)?);
    }
    let mut fails = 0usize;
    for (query_text, relevant) in &queries {
        let query = engine.analyze(query_text)?;
        let mut scored = doc_fingerprints
            .iter()
            .enumerate()
            .map(|(index, document)| {
                let result = engine.compare_fingerprints(&query, document);
                (index, result)
            })
            .collect::<Vec<_>>();
        scored.sort_by(|left, right| {
            right
                .1
                .score
                .total_cmp(&left.1.score)
                .then_with(|| documents[left.0].cmp(&documents[right.0]))
        });
        let rank = scored.iter().position(|(index, _)| {
            relevant
                .iter()
                .any(|target| same_text(&documents[*index], target))
        });
        let position = rank.map(|r| r + 1).unwrap_or(usize::MAX);
        if position > 5 {
            fails += 1;
            println!("--- query {query_text:?} first relevant rank {position}");
            println!("    relevant: {relevant:?}");
            for (index, result) in scored.iter().take(6) {
                let is_rel = relevant
                    .iter()
                    .any(|target| same_text(&documents[*index], target));
                println!(
                    "    {} score={:.4} doc={:?}",
                    if is_rel { "REL" } else { "imp" },
                    result.score,
                    documents[*index]
                );
            }
        }
    }
    println!("queries failing recall@5: {fails}/{}", queries.len());
    Ok(())
}
