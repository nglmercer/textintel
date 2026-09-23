//! Train an interpretable logistic similarity scorer from the evaluation set.
//!
//! ```text
//! textintel-train similarity data/evaluation --output models/similarity-v5.json
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
use textintel::cli::{handle_meta, parse_args, render_overview, train_spec};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let spec = train_spec();
    let version = env!("CARGO_PKG_VERSION");
    if let Some(text) = handle_meta(spec, version, &argv) {
        println!("{text}");
        return Ok(());
    }
    let parsed = match parse_args(spec, &argv) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("{error}");
            eprintln!("Run `{} --help` for usage.", spec.name);
            std::process::exit(error.exit_code());
        }
    };
    match parsed.command() {
        Some("similarity") => run_similarity(&parsed),
        Some("spam") => run_spam(&parsed),
        Some(_) => unreachable!("the parser only resolves spec commands"),
        None => {
            eprintln!("{}", render_overview(spec, version));
            std::process::exit(2);
        }
    }
}
