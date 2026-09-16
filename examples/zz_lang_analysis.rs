//! TEMPORARY release probe: language-detection errors per split.
use textintel::evaluation::EvaluationDataset;
use textintel::{EngineConfig, TextIntelligence};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let split = std::env::args().nth(1).unwrap_or("test".to_string());
    let dataset = EvaluationDataset::from_dir("data/evaluation")?;
    let engine = TextIntelligence::builder()
        .config(EngineConfig::default())
        .production_local()
        .build()?;
    let cases: Vec<_> = dataset
        .cases
        .iter()
        .filter(|c| c.normalized_split() == split && !c.languages.is_empty())
        .collect();
    println!("labelled={}", cases.len());
    let mut top1_miss = 0;
    let mut top3_miss = 0;
    for case in &cases {
        let fingerprint = engine.analyze(&case.a)?;
        let ranked: Vec<&str> = fingerprint
            .language_candidates
            .iter()
            .map(|c| c.language.as_str())
            .collect();
        let expected: Vec<String> = case.languages.iter().map(|l| l.to_lowercase()).collect();
        let top1_hit = ranked.first().is_some_and(|top| expected.iter().any(|e| e == top));
        let top3_hit = ranked
            .iter()
            .take(3)
            .any(|top| expected.iter().any(|e| e == top));
        if !top1_hit {
            top1_miss += 1;
        }
        if !top3_hit {
            top3_miss += 1;
        }
        if !top1_hit || !top3_hit {
            println!(
                "miss top1={} top3={} id={} cat={:?} a={:?} expected={:?} got={:?}",
                !top1_hit,
                !top3_hit,
                case.id,
                case.category,
                case.a,
                case.languages,
                ranked.iter().take(5).collect::<Vec<_>>()
            );
        }
    }
    println!("top1_miss={top1_miss} top3_miss={top3_miss}");
    Ok(())
}
