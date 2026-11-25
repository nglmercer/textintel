//! Extracts spam-detection feature rows for the calibration trainer.
//! Each line of output: {"id","label","split","features":{...}}.
use textintel::TextIntelligence;
use textintel::detection::spam::spam_features;
use textintel::evaluation::dataset::SpamCorpus;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut out = String::new();
    let engine = TextIntelligence::default();
    for (path, split) in [(&args[1], "train"), (&args[2], "eval")] {
        let corpus = SpamCorpus::from_file(path).expect("corpus");
        for item in &corpus.items {
            let fp = engine.analyze(&item.text).expect("analyze");
            let patterns = engine.match_patterns(&item.text).expect("patterns");
            let f = spam_features(&fp, &patterns);
            let row = serde_json::json!({
                "id": item.id,
                "label": if item.is_spam() { 1 } else { 0 },
                "split": split,
                "features": {
                    "url_count": f.url_count as f64,
                    "email_count": f.email_count as f64,
                    "emoji_count": f.emoji_count as f64,
                    "number_ratio": f.number_ratio,
                    "emoji_ratio": f.emoji_ratio,
                    "uppercase_ratio": f.uppercase_ratio,
                    "punctuation_ratio": f.punctuation_ratio,
                    "entropy": f.entropy,
                    "repetition_score": f.repetition_score,
                    "obfuscation_score": f.obfuscation_score,
                    "pattern_score": f.pattern_score,
                    "decoded_similarity": f.decoded_similarity,
                    "mixed_scripts": if f.mixed_scripts { 1.0 } else { 0.0 },
                    "confusable_count": f.confusable_count as f64,
                }
            });
            out.push_str(&serde_json::to_string(&row).unwrap());
            out.push('\n');
        }
    }
    std::fs::write(&args[3], out).unwrap();
    println!("wrote {}", args[3]);
}
