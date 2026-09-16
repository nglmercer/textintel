//! Train an interpretable logistic similarity scorer from the evaluation set.
//!
//! Usage:
//!
//! ```text
//! textintel-train similarity data/evaluation --output models/similarity-v2.json
//! ```
//!
//! The tool trains on the `train` split only, calibrates the bias on the
//! `validation` split, and reports held-out metrics on `test` without
//! training on it. A production-like engine (semantic and phonetic channels
//! enabled, the production semantic backend, rule-based G2P) featurizes every
//! pair so the artifact matches production scoring conditions — including
//! nonzero semantic and phonetic weights whenever the evaluation proves them
//! useful. Pass `--transformer-model <dir>` (or set
//! `TEXTINTEL_TRANSFORMER_MODEL`) to featurize with the configured local
//! transformer; otherwise the feature-hash fallback serves and the choice is
//! recorded in the artifact.

mod metrics;
mod similarity;
mod spam;

use similarity::run_similarity;
use spam::run_spam;

fn usage() -> &'static str {
    "Usage:\n  textintel-train similarity <dataset> --output <artifact.json> [--iterations N] [--learning-rate F] [--l2 F] [--transformer-model <dir>]\n  textintel-train spam --output <artifact.json> [--count N] [--seed N] [--corpus <spam-train.json>] [--heldout <spam-eval.json>]"
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].clone())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version" | "-V") => {
            println!("textintel-train {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("--help" | "-h") => {
            println!("{}", usage());
            Ok(())
        }
        Some("similarity") => run_similarity(&args),
        Some("spam") => run_spam(&args),
        _ => {
            eprintln!("{}", usage());
            std::process::exit(2);
        }
    }
}
