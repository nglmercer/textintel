//! Generated help output. Every line derives from [`CliSpec`] data:
//! usage lines assemble from positional and argument specs, tables align
//! from the same entries, and the footer echoes the spec's contract text.

use super::spec::{
    ArgKind, ArgSpec, CliSpec, CommandSpec, HELP_LONG, HELP_SHORT, VERSION_LONG, VERSION_SHORT,
};

fn flag_term(arg: &ArgSpec) -> String {
    match arg.kind {
        ArgKind::Flag => format!("--{}", arg.long),
        ArgKind::Value { metavar } => format!("--{} <{}>", arg.long, metavar),
    }
}

fn usage_options(spec: &CliSpec, command: Option<&CommandSpec>) -> String {
    let mut ids: Vec<&'static str> = Vec::new();
    let mut push = |id: &'static str| {
        if !ids.contains(&id) {
            ids.push(id);
        }
    };
    if let Some(found) = command {
        for id in found.args.iter().chain(found.required_args.iter()) {
            push(id);
        }
    }
    let globals: &[&'static str] = if spec.commands.is_empty() {
        &[]
    } else {
        spec.global_args
    };
    for id in globals {
        push(id);
    }
    if spec.commands.is_empty() {
        for arg in spec.args {
            push(arg.id);
        }
    }
    let required: &[&str] = match command {
        Some(found) => found.required_args,
        None => spec.single_required_args,
    };
    ids.into_iter()
        .filter_map(|id| spec.find_arg(id))
        .map(|arg| {
            let term = flag_term(arg);
            if required.contains(&arg.id) {
                term
            } else {
                format!("[{term}]")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn usage_positionals(positionals: &[super::spec::PositionalSpec], command: Option<&str>) -> String {
    let _ = command;
    positionals
        .iter()
        .map(|positional| {
            let mut term = format!("<{}>", positional.metavar);
            if positional.rest {
                term.push_str("...");
            }
            if positional.required {
                term
            } else {
                format!("[{term}]")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn table(rows: &[(String, String)]) -> String {
    let width = rows.iter().map(|(left, _)| left.len()).max().unwrap_or(0);
    rows.iter()
        .map(|(left, right)| format!("  {left:<width$}  {right}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn meta_rows() -> Vec<(String, String)> {
    vec![
        (
            format!("-{HELP_SHORT}, --{HELP_LONG}"),
            "Show this help and exit".to_string(),
        ),
        (
            format!("-{VERSION_SHORT}, --{VERSION_LONG}"),
            "Show the version and exit".to_string(),
        ),
    ]
}

/// Full help for one command, generated from its spec.
pub fn render_command(spec: &CliSpec, command: &CommandSpec) -> String {
    let mut out = format!("{} {} — {}\n", spec.name, command.name, command.summary);
    if !command.aliases.is_empty() {
        out.push_str(&format!("Aliases: {}\n", command.aliases.join(", ")));
    }
    let usage = [
        usage_positionals(command.positionals, Some(command.name)),
        usage_options(spec, Some(command)),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" ");
    out.push_str(&format!("\nUsage:\n  {} {}", spec.name, command.name));
    if !usage.is_empty() {
        out.push_str(&format!(" {usage}"));
    }
    out.push('\n');
    if !command.positionals.is_empty() {
        let rows: Vec<(String, String)> = command
            .positionals
            .iter()
            .map(|positional| {
                let mut term = format!("<{}>", positional.metavar);
                if positional.rest {
                    term.push_str("...");
                }
                let mut help = positional.help.to_string();
                if !positional.required {
                    help.push_str(" (optional)");
                }
                (term, help)
            })
            .collect();
        out.push_str(&format!("\nArguments:\n{}\n", table(&rows)));
    }
    let mut rows: Vec<(String, String)> = command
        .args
        .iter()
        .filter_map(|id| spec.find_arg(id))
        .map(|arg| (flag_term(arg), arg.help.to_string()))
        .collect();
    for id in spec.global_args {
        if command.args.contains(id) {
            continue;
        }
        if let Some(arg) = spec.find_arg(id) {
            rows.push((flag_term(arg), arg.help.to_string()));
        }
    }
    rows.extend(meta_rows());
    out.push_str(&format!("\nOptions:\n{}\n", table(&rows)));
    out
}

/// Overview help: command list (multi) or single usage, plus globals.
pub fn render_overview(spec: &CliSpec, version: &str) -> String {
    let mut out = format!("{} {version} — {}\n", spec.name, spec.about);
    if spec.commands.is_empty() {
        let usage = [
            usage_positionals(spec.single_positionals, None),
            usage_options(spec, None),
        ]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
        out.push_str(&format!("\nUsage:\n  {}", spec.name));
        if !usage.is_empty() {
            out.push_str(&format!(" {usage}"));
        }
        out.push('\n');
        let mut rows: Vec<(String, String)> = spec
            .args
            .iter()
            .map(|arg| (flag_term(arg), arg.help.to_string()))
            .collect();
        rows.extend(meta_rows());
        out.push_str(&format!("\nOptions:\n{}\n", table(&rows)));
    } else {
        out.push_str(&format!("\nUsage:\n  {} <command> [options]\n", spec.name));
        let rows: Vec<(String, String)> = spec
            .commands
            .iter()
            .map(|command| {
                let mut name = command.name.to_string();
                if !command.aliases.is_empty() {
                    name.push_str(&format!(" ({})", command.aliases.join(", ")));
                }
                (name, command.summary.to_string())
            })
            .collect();
        out.push_str(&format!("\nCommands:\n{}\n", table(&rows)));
        let mut rows: Vec<(String, String)> = spec
            .global_args
            .iter()
            .filter_map(|id| spec.find_arg(id))
            .map(|arg| (flag_term(arg), arg.help.to_string()))
            .collect();
        rows.extend(meta_rows());
        out.push_str(&format!("\nGlobal options:\n{}\n", table(&rows)));
        out.push_str(&format!(
            "\nRun `{} <command> --help` for command details.\n",
            spec.name
        ));
    }
    if let Some(footer) = spec.footer {
        out.push_str(&format!("\n{footer}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::spec::{ArgKind, ArgSpec};
    use super::super::spec::{CliSpec, CommandSpec, PositionalSpec};
    use super::*;

    const ARGS: &[ArgSpec] = &[
        ArgSpec {
            id: "json",
            long: "json",
            kind: ArgKind::Flag,
            help: "JSON output.",
        },
        ArgSpec {
            id: "output",
            long: "output",
            kind: ArgKind::Value { metavar: "FILE" },
            help: "Output file.",
        },
    ];

    const COMMANDS: &[CommandSpec] = &[CommandSpec {
        name: "run",
        aliases: &["go"],
        summary: "Run things.",
        args: &["output"],
        required_args: &["output"],
        positionals: &[
            PositionalSpec {
                id: "input",
                metavar: "INPUT",
                required: true,
                rest: false,
                help: "Input file.",
            },
            PositionalSpec {
                id: "extra",
                metavar: "EXTRA",
                required: false,
                rest: true,
                help: "Extra inputs.",
            },
        ],
    }];

    const SPEC: &CliSpec = &CliSpec {
        name: "tool",
        about: "Test tool.",
        args: ARGS,
        global_args: &["json"],
        commands: COMMANDS,
        single_positionals: &[],
        single_required_args: &[],
        footer: Some("Footer contract."),
    };

    #[test]
    fn overview_lists_commands_and_globals() {
        let help = render_overview(SPEC, "1.0");
        assert!(help.contains("tool 1.0 — Test tool."));
        assert!(help.contains("run (go)"));
        assert!(help.contains("Run things."));
        assert!(help.contains("--json"));
        assert!(help.contains("--help"));
        assert!(help.contains("Footer contract."));
        assert!(help.contains("tool <command> --help"));
    }

    #[test]
    fn command_help_assembles_usage_and_tables() {
        let help = render_command(SPEC, &COMMANDS[0]);
        assert!(help.contains("Aliases: go"));
        // Required positional bare, optional rest bracketed, required
        // value bare, optional flag bracketed.
        assert!(help.contains("tool run <INPUT> [<EXTRA>...] --output <FILE> [--json]"));
        assert!(help.contains("<EXTRA>..."));
        assert!(help.contains("(optional)"));
        assert!(help.contains("--output <FILE>"));
        assert!(help.contains("Output file."));
    }
}
