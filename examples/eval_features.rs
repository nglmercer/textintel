//! Development harness: dump per-case decision features for the evaluation
//! dataset as JSONL on stdout. Used by `tools/train_decision/` to fit the
//! baked-in linear decision layer; not part of the public API surface.
//!
//! Usage: cargo run --locked --no-default-features --example eval_features -- data/evaluation [split]

use textintel::evaluation::EvaluationDataset;
use textintel::{EngineConfig, TextIntelligence};

fn number(value: f64) -> serde_json::Value {
    serde_json::Value::from(value)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "data/evaluation".into());
    let split_filter = std::env::args().nth(2);
    let pairs_file = std::env::args().nth(3);
    let dataset = EvaluationDataset::load_path(&path)?;
    let mut config = EngineConfig::default();
    if std::env::var("EVAL_PHONETIC").is_ok() {
        config.phonetic = true;
    }
    if std::env::var("EVAL_SEMHASH").is_ok() {
        config.semantic = true;
    }
    let mut engine = TextIntelligence::new(config);
    if std::env::var("EVAL_SEMHASH").is_ok() {
        engine = engine.with_embedding_provider(textintel::semantic::embeddings::FeatureHashEmbeddingProvider::default());
    }
    let mut synthetic: Vec<(String, String, bool, String, String)> = Vec::new();
    if let Some(file) = &pairs_file {
        for line in std::fs::read_to_string(file)?.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let row: serde_json::Value = serde_json::from_str(line)?;
            synthetic.push((
                row["a"].as_str().unwrap_or_default().to_string(),
                row["b"].as_str().unwrap_or_default().to_string(),
                row["label"].as_bool().unwrap_or(false),
                row["category"].as_str().unwrap_or("noise").to_string(),
                row["split"].as_str().unwrap_or("train").to_string(),
            ));
        }
    }
    let mut iterate: Vec<(String, String, bool, String, String)> = Vec::new();
    for case in &dataset.cases {
        iterate.push((
            case.a.clone(),
            case.b.clone(),
            case.is_similar(),
            case.primary_category().to_string(),
            case.split.clone(),
        ));
    }
    iterate.extend(synthetic);
    for (case_a, case_b, case_label, case_category, case_split) in &iterate {
        if let Some(split) = &split_filter
            && split != "-"
            && case_split != split
        {
            continue;
        }
        let result = engine.compare(case_a, case_b)?;
        let transliteration_similarity = result.transliteration_similarity.unwrap_or(0.0);
        let transliteration_confidence = result.transliteration_confidence.unwrap_or(0.0);
        let mut features = serde_json::Map::new();
        let pairs: [(&str, f64); 35] = [
            ("weighted_score", result.score),
            ("semantic", result.semantic.unwrap_or(0.0)),
            ("semantic_present", result.semantic.is_some() as u8 as f64),
            ("lexical", result.lexical.unwrap_or(0.0)),
            ("character", result.character.unwrap_or(0.0)),
            ("visual", result.visual.unwrap_or(0.0)),
            ("phonetic", result.phonetic.unwrap_or(0.0)),
            ("phonetic_present", result.phonetic.is_some() as u8 as f64),
            ("symbolic", result.symbolic.unwrap_or(0.0)),
            ("symbolic_present", result.symbolic.is_some() as u8 as f64),
            ("decoded_similarity", result.decoded_similarity.unwrap_or(0.0)),
            ("transliteration_similarity", transliteration_similarity),
            ("transliteration_confidence", transliteration_confidence),
            (
                "transliteration_weighted",
                transliteration_similarity * transliteration_confidence,
            ),
            ("obfuscation_similarity", result.obfuscation_similarity.unwrap_or(0.0)),
            ("lexicon_validity", result.lexicon_validity),
            ("single_word_pair", result.single_word_pair),
            ("swapped_word_similarity", result.swapped_word_similarity),
            ("swapped_phonetic", result.swapped_phonetic),
            ("confusable_swap", result.confusable_swap),
            ("valid_swap_similarity", result.valid_swap_similarity),
            ("normalized_identity", result.normalized_identity),
            ("substring_containment", result.substring_containment),
            ("cross_script_pair", result.cross_script_pair),
            ("cross_script_agreement", result.cross_script_agreement),
            ("exact_decode", result.exact_decode),
            ("single_word_exact", result.single_word_exact),
            ("entity_agreement", result.entity_agreement),
            ("entity_conflict", result.entity_conflict),
            ("transliteration_compatibility", result.transliteration_compatibility),
            ("contextual_semantic", result.contextual_semantic),
            ("cross_language_semantic", result.cross_language_semantic),
            (
                "semantic_without_lexical_overlap",
                result.semantic_without_lexical_overlap,
            ),
            (
                "transliteration_semantic_agreement",
                result.transliteration_semantic_agreement,
            ),
            (
                "transliteration_semantic_conflict",
                result.transliteration_semantic_conflict,
            ),
        ];
        for (name, value) in pairs {
            features.insert(name.to_string(), number(value));
        }
        let mut row = serde_json::Map::new();
        row.insert("id".to_string(), serde_json::Value::from(format!("{case_a}|{case_b}")));
        row.insert(
            "category".to_string(),
            serde_json::Value::from(case_category.as_str()),
        );
        row.insert("split".to_string(), serde_json::Value::from(case_split.as_str()));
        row.insert("label".to_string(), serde_json::Value::from(*case_label));
        row.insert("a".to_string(), serde_json::Value::from(case_a.as_str()));
        row.insert("b".to_string(), serde_json::Value::from(case_b.as_str()));
        row.insert("features".to_string(), serde_json::Value::Object(features));
        println!("{}", serde_json::to_string(&row)?);
    }
    Ok(())
}
