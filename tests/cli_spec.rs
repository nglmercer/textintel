//! CLI spec conformance: every shipped spec validates, every command
//! renders help from its own entries, and every command parses from a
//! spec-derived argv. If a command exists in the spec but not on the
//! command line (or vice versa), these tests fail.

use textintel::cli::{
    CliSpec, eval_llm_spec, parse_args, render_command, render_overview, textintel_spec,
    train_decision_spec, train_spec,
};

fn all_specs() -> Vec<&'static CliSpec> {
    vec![
        textintel_spec(),
        train_spec(),
        train_decision_spec(),
        eval_llm_spec(),
    ]
}

fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_string()).collect()
}

#[test]
fn every_spec_validates() {
    for spec in all_specs() {
        spec.validate()
            .unwrap_or_else(|error| panic!("{}: {error}", spec.name));
    }
}

#[test]
fn overview_mentions_every_command() {
    for spec in all_specs() {
        let help = render_overview(spec, "0.0.0-test");
        assert!(help.contains(spec.name), "{} overview", spec.name);
        assert!(help.contains("Usage:"), "{} overview", spec.name);
        for command in spec.commands {
            assert!(
                help.contains(command.name),
                "{} overview mentions {}",
                spec.name,
                command.name
            );
        }
    }
}

#[test]
fn command_help_derives_from_its_entries() {
    for spec in all_specs() {
        for command in spec.commands {
            let help = render_command(spec, command);
            assert!(help.contains(command.name));
            assert!(help.contains(command.summary));
            for positional in command.positionals {
                assert!(
                    help.contains(positional.metavar),
                    "{} {} mentions <{}>",
                    spec.name,
                    command.name,
                    positional.metavar
                );
            }
            for id in command.args {
                let arg = spec.find_arg(id).expect("registry entry");
                assert!(
                    help.contains(&format!("--{}", arg.long)),
                    "{} {} mentions --{}",
                    spec.name,
                    command.name,
                    arg.long
                );
            }
        }
    }
}

/// Build a minimal valid argv for a command from its own spec: required
/// positionals get `x`, required values get `v`.
fn minimal_argv(spec: &CliSpec, command: &str) -> Vec<String> {
    let mut words = vec![command.to_string()];
    let found = spec.find_command(command).expect("command");
    for positional in found.positionals {
        if positional.required {
            words.push("x".to_string());
        }
    }
    for id in found.required_args {
        let arg = spec.find_arg(id).expect("registry entry");
        words.push(format!("--{}", arg.long));
        words.push("v".to_string());
    }
    words
}

#[test]
fn every_command_parses_from_its_spec() {
    for spec in all_specs() {
        if spec.commands.is_empty() {
            // Single-command CLIs: required values only.
            let mut words: Vec<String> = Vec::new();
            for id in spec.single_required_args {
                let arg = spec.find_arg(id).expect("registry entry");
                words.push(format!("--{}", arg.long));
                words.push("v".to_string());
            }
            let parsed = parse_args(spec, &words).expect("single parse");
            assert_eq!(parsed.command(), None);
            continue;
        }
        for command in spec.commands {
            let words = minimal_argv(spec, command.name);
            let parsed = parse_args(spec, &words)
                .unwrap_or_else(|error| panic!("{} {}: {error}", spec.name, command.name));
            assert_eq!(parsed.command(), Some(command.name));
            for alias in command.aliases {
                let mut aliased = vec![(*alias).to_string()];
                aliased.extend(words[1..].iter().cloned());
                let parsed = parse_args(spec, &aliased).expect("alias parse");
                assert_eq!(parsed.command(), Some(command.name));
            }
        }
    }
}

#[test]
fn unknown_inputs_error_with_spec_options() {
    let spec = textintel_spec();
    let error = parse_args(spec, &argv(&["no-such-command"])).expect_err("unknown command");
    assert!(error.to_string().contains("analyze"));
    let error = parse_args(spec, &argv(&["analyze", "--bogus", "x"])).expect_err("unknown flag");
    assert!(error.to_string().contains("--json"));
    assert_eq!(error.exit_code(), 2);
    // Missing required positional and value.
    assert!(parse_args(spec, &argv(&["analyze"])).is_err());
    assert!(parse_args(spec, &argv(&["classify", "x"])).is_err());
    assert!(parse_args(spec, &argv(&["classify", "x", "--task", "t"])).is_ok());
}
