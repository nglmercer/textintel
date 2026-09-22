//! `textintel` binary: parse against the declarative [`textintel::cli`]
//! spec and dispatch to [`commands`] handlers. Usage, validation, and
//! `--help` all derive from the spec — nothing is hardcoded here.

mod commands;

use textintel::cli::{handle_meta, parse_args, render_overview, textintel_spec};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let spec = textintel_spec();
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
    let Some(command) = parsed.command() else {
        eprintln!("{}", render_overview(spec, version));
        std::process::exit(2);
    };
    let code = match command {
        "analyze" => commands::text::analyze(&parsed)?,
        "explain" => commands::text::explain(&parsed)?,
        "decode" => commands::text::decode(&parsed)?,
        "compare" => commands::compare::compare(&parsed)?,
        "duplicate" => commands::compare::duplicate(&parsed)?,
        "spam" => commands::text::spam(&parsed)?,
        "batch" => commands::text::batch(&parsed)?,
        "resources" => commands::resources::resources(&parsed)?,
        "diagnostics" | "provider-info" => commands::resources::diagnostics(&parsed)?,
        "schema-version" => commands::resources::schema_version(&parsed)?,
        "eval" => commands::eval::run_eval(&parsed)?,
        "index" => commands::store::index(&parsed)?,
        "search" => commands::store::search(&parsed)?,
        "generate" => commands::generate::generate(&parsed)?,
        "decide" => commands::decision::run_decide(&parsed)?,
        "classify" => commands::decision::run_classify(&parsed)?,
        "decision-model-info" => commands::decision::run_decision_model_info(&parsed)?,
        "eval-decision" => commands::decision::run_eval_decision(&parsed)?,
        _ => unreachable!("the parser only resolves spec commands"),
    };
    if code != 0 {
        std::process::exit(code);
    }
    Ok(())
}
