//! TEMPORARY release probe: rebus top1 misses per split.
use textintel::evaluation::EvaluationDataset;
use textintel::{EngineConfig, TextIntelligence};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let split = std::env::args().nth(1).unwrap_or("test".to_string());
    let dataset = EvaluationDataset::from_dir("data/evaluation")?;
    let engine = TextIntelligence::builder()
        .config(EngineConfig::default())
        .production_local()
        .build()?;
    for case in dataset
        .cases
        .iter()
        .filter(|c| c.normalized_split() == split)
    {
        if !case.labels.get("rebus").copied().unwrap_or(false) {
            continue;
        }
        let languages = if case.languages.is_empty() {
            None
        } else {
            Some(case.languages.as_slice())
        };
        let candidates = engine.decode_with_languages(&case.a, languages, None)?;
        let matches = |text: &str| {
            text.eq_ignore_ascii_case(&case.b)
                || case
                    .expected
                    .decoded_contains
                    .iter()
                    .any(|want| text.eq_ignore_ascii_case(want))
        };
        let rank = candidates
            .iter()
            .position(|c| matches(&c.text))
            .map(|index| index + 1);
        if rank != Some(1) {
            println!(
                "miss rank={:?} id={} a={:?} b={:?} top3={:?}",
                rank,
                case.id,
                case.a,
                case.b,
                candidates
                    .iter()
                    .take(3)
                    .map(|c| (c.text.as_str(), (c.score * 1000.0) as i32))
                    .collect::<Vec<_>>()
            );
        }
    }
    Ok(())
}
