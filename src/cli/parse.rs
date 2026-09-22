//! Spec-driven argument parsing. Flags may precede the command word
//! (`textintel --json eval ...` works); `--flag value` and
//! `--flag=value` both parse; `--` ends flag parsing. Unknown flags and
//! missing required inputs fail with spec-derived messages instead of
//! being silently misread as positional text.

use std::collections::BTreeSet;
use std::str::FromStr;

use super::help::{render_command, render_overview};
use super::spec::{ArgKind, CliSpec, HELP_LONG, HELP_SHORT, VERSION_LONG, VERSION_SHORT};

/// Parsed command line: resolved command, positional data, and flag
/// values addressed by registry id.
#[derive(Debug, Clone)]
pub struct ParsedArgs<'a> {
    spec: &'a CliSpec,
    command: Option<&'a str>,
    positionals: Vec<String>,
    /// Value occurrences in order (repeated flags keep every entry).
    pairs: Vec<(String, String)>,
    flags: BTreeSet<String>,
}

impl<'a> ParsedArgs<'a> {
    /// Canonical command name, or `None` for single-command CLIs and for
    /// a missing command word.
    pub fn command(&self) -> Option<&str> {
        self.command
    }

    /// Whether a flag id was passed.
    pub fn flag(&self, id: &str) -> bool {
        self.flags.contains(id)
    }

    /// First value for a value id, or `None` when absent.
    pub fn value(&self, id: &str) -> Option<&str> {
        self.pairs
            .iter()
            .find(|(key, _)| key == id)
            .map(|(_, value)| value.as_str())
    }

    /// Every value for a repeatable id, in occurrence order.
    pub fn values_of(&self, id: &str) -> Vec<&str> {
        self.pairs
            .iter()
            .filter(|(key, _)| key == id)
            .map(|(_, value)| value.as_str())
            .collect()
    }

    /// Positional data by index.
    pub fn positional(&self, index: usize) -> Option<&str> {
        self.positionals.get(index).map(String::as_str)
    }

    /// Positional data from `from` onward (for `rest` positionals).
    pub fn rest(&self, from: usize) -> &[String] {
        self.positionals.get(from..).unwrap_or(&[])
    }

    /// Required positional or a usage error naming the metavar.
    pub fn required_positional(&self, index: usize) -> Result<&str, CliError> {
        self.positional(index).ok_or_else(|| {
            let metavar = self
                .positionals_spec()
                .get(index)
                .map(|positional| positional.metavar)
                .unwrap_or("value");
            CliError::MissingPositional {
                metavar: metavar.to_string(),
                command: self.command_label(),
            }
        })
    }

    /// Required value or a usage error naming the flag.
    pub fn required_value(&self, id: &str) -> Result<&str, CliError> {
        self.value(id).ok_or_else(|| CliError::MissingValue {
            flag: self.long_of(id).to_string(),
        })
    }

    /// Typed value (`None` when absent); parse failures become usage
    /// errors instead of panics or silent defaults.
    pub fn parsed_value<T>(&self, id: &str) -> Result<Option<T>, CliError>
    where
        T: FromStr,
        T::Err: std::fmt::Display,
    {
        self.value(id)
            .map(|value| {
                value.parse::<T>().map_err(|error| CliError::InvalidValue {
                    flag: self.long_of(id).to_string(),
                    value: value.to_string(),
                    reason: error.to_string(),
                })
            })
            .transpose()
    }

    fn long_of(&self, id: &str) -> String {
        self.spec
            .find_arg(id)
            .map(|arg| arg.long.to_string())
            .unwrap_or_else(|| id.to_string())
    }

    fn command_label(&self) -> String {
        self.command.unwrap_or(self.spec.name).to_string()
    }

    fn positionals_spec(&self) -> &[super::spec::PositionalSpec] {
        match self.command.and_then(|name| self.spec.find_command(name)) {
            Some(command) => command.positionals,
            None => self.spec.single_positionals,
        }
    }
}

/// Usage errors. All render with spec-derived valid options and exit 2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    /// No command word on a multi-command CLI.
    NoCommand,
    UnknownCommand {
        name: String,
        available: Vec<String>,
    },
    UnknownFlag {
        flag: String,
        valid: Vec<String>,
    },
    MissingValue {
        flag: String,
    },
    UnexpectedValue {
        flag: String,
    },
    MissingPositional {
        metavar: String,
        command: String,
    },
    InvalidValue {
        flag: String,
        value: String,
        reason: String,
    },
}

impl CliError {
    /// Process exit code for usage errors.
    pub fn exit_code(&self) -> i32 {
        2
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoCommand => write!(f, "missing command"),
            Self::UnknownCommand { name, available } => {
                write!(
                    f,
                    "unknown command {name:?} (available: {})",
                    available.join(", ")
                )
            }
            Self::UnknownFlag { flag, valid } => {
                write!(f, "unknown flag {flag:?} (valid: {})", valid.join(", "))
            }
            Self::MissingValue { flag } => write!(f, "flag --{flag} requires a value"),
            Self::UnexpectedValue { flag } => write!(f, "flag --{flag} takes no value"),
            Self::MissingPositional { metavar, command } => {
                write!(f, "command {command} requires <{metavar}>")
            }
            Self::InvalidValue {
                flag,
                value,
                reason,
            } => {
                write!(f, "invalid value {value:?} for --{flag}: {reason}")
            }
        }
    }
}

impl std::error::Error for CliError {}

fn meta_forms() -> [String; 4] {
    [
        format!("--{HELP_LONG}"),
        format!("-{HELP_SHORT}"),
        format!("--{VERSION_LONG}"),
        format!("-{VERSION_SHORT}"),
    ]
}

/// `--help` / `--version` handling. Call before [`parse_args`]: returns the
/// text to print (and exit 0 with) when a request is present, `None`
/// otherwise. Stops scanning at `--` so literal text past it never
/// triggers meta output.
pub fn handle_meta(spec: &CliSpec, version: &str, argv: &[String]) -> Option<String> {
    let mut help = false;
    let mut show_version = false;
    let mut command_word: Option<&str> = None;
    let forms = meta_forms();
    for token in argv {
        if token == "--" {
            break;
        }
        if token == &forms[0] || token == &forms[1] {
            help = true;
            continue;
        }
        if token == &forms[2] || token == &forms[3] {
            show_version = true;
            continue;
        }
        if command_word.is_none() && !token.starts_with('-') {
            command_word = Some(token);
        }
    }
    if show_version {
        return Some(format!("{} {version}", spec.name));
    }
    if help {
        if let Some(word) = command_word
            && let Some(command) = spec.find_command(word)
        {
            return Some(render_command(spec, command));
        }
        return Some(render_overview(spec, version));
    }
    None
}

/// Parse `argv` (without the binary name) against `spec`.
pub fn parse_args<'a>(spec: &'a CliSpec, argv: &[String]) -> Result<ParsedArgs<'a>, CliError> {
    // First pass: positional words for command resolution. Known value
    // flags swallow their value token so `--output a.json run ...` still
    // resolves `run` as the command word.
    let mut raw_positionals: Vec<String> = Vec::new();
    let mut bad: Vec<String> = Vec::new();
    let mut index = 0;
    while index < argv.len() {
        let token = &argv[index];
        if token == "--" {
            raw_positionals.extend(argv[index + 1..].iter().cloned());
            break;
        }
        if let Some(long) = token.strip_prefix("--")
            && !long.is_empty()
        {
            let name = long.split('=').next().unwrap_or(long);
            match spec.find_long(name) {
                Some(arg) if matches!(arg.kind, ArgKind::Value { .. }) => {
                    if !long.contains('=') {
                        if index + 1 >= argv.len() {
                            return Err(CliError::MissingValue {
                                flag: arg.long.to_string(),
                            });
                        }
                        index += 1;
                    }
                }
                Some(_) => {}
                None => bad.push(token.clone()),
            }
            index += 1;
            continue;
        }
        if token.starts_with('-') && token.len() > 1 {
            // Shorts (only -h/-V exist, handled by handle_meta) and meta
            // tokens reaching parse (caller skipped handle_meta) are
            // reported, never silently treated as data.
            bad.push(token.clone());
            index += 1;
            continue;
        }
        raw_positionals.push(token.clone());
        index += 1;
    }
    // Command resolution for multi-command CLIs.
    let command = if spec.commands.is_empty() {
        None
    } else {
        match raw_positionals.first() {
            None => None,
            Some(word) => match spec.find_command(word) {
                Some(found) => Some(found.name),
                None => {
                    return Err(CliError::UnknownCommand {
                        name: word.clone(),
                        available: spec
                            .commands
                            .iter()
                            .map(|command| command.name.to_string())
                            .collect(),
                    });
                }
            },
        }
    };
    if let Some(flag) = bad.into_iter().next() {
        return Err(CliError::UnknownFlag {
            flag,
            valid: valid_longs(spec, command),
        });
    }
    let mut positionals = raw_positionals;
    if command.is_some() {
        positionals.remove(0);
    }
    let allowed: BTreeSet<&str> = if spec.commands.is_empty() {
        spec.args.iter().map(|arg| arg.id).collect()
    } else {
        let mut allowed: BTreeSet<&str> = spec.global_args.iter().copied().collect();
        if let Some(name) = command
            && let Some(found) = spec.find_command(name)
        {
            allowed.extend(found.args.iter().copied());
        }
        allowed
    };
    let mut parsed = ParsedArgs {
        spec,
        command,
        positionals,
        pairs: Vec::new(),
        flags: BTreeSet::new(),
    };
    resolve_flags(spec, argv, command, &allowed, &mut parsed)?;
    Ok(parsed)
}

fn valid_longs(spec: &CliSpec, command: Option<&str>) -> Vec<String> {
    let mut longs: Vec<String> = spec
        .global_args
        .iter()
        .filter_map(|id| spec.find_arg(id))
        .map(|arg| format!("--{}", arg.long))
        .collect();
    if let Some(name) = command
        && let Some(found) = spec.find_command(name)
    {
        for id in found.args {
            if let Some(arg) = spec.find_arg(id) {
                let long = format!("--{}", arg.long);
                if !longs.contains(&long) {
                    longs.push(long);
                }
            }
        }
    }
    if spec.commands.is_empty() {
        longs = spec
            .args
            .iter()
            .map(|arg| format!("--{}", arg.long))
            .collect();
    }
    longs.sort();
    longs
}

fn resolve_flags(
    spec: &CliSpec,
    argv: &[String],
    command: Option<&str>,
    allowed: &BTreeSet<&str>,
    parsed: &mut ParsedArgs<'_>,
) -> Result<(), CliError> {
    let mut index = 0;
    let mut past_terminator = false;
    while index < argv.len() {
        let token = &argv[index];
        if token == "--" {
            past_terminator = true;
            index += 1;
            continue;
        }
        if past_terminator || !token.starts_with("--") || token.len() == 2 {
            index += 1;
            continue;
        }
        let long = &token[2..];
        let (name, inline) = match long.split_once('=') {
            Some((name, value)) => (name, Some(value.to_string())),
            None => (long, None),
        };
        let Some(arg) = spec.find_long(name) else {
            return Err(CliError::UnknownFlag {
                flag: token.clone(),
                valid: valid_longs(spec, command),
            });
        };
        if !allowed.contains(arg.id) {
            return Err(CliError::UnknownFlag {
                flag: token.clone(),
                valid: valid_longs(spec, command),
            });
        }
        match arg.kind {
            ArgKind::Flag => {
                if inline.is_some() {
                    return Err(CliError::UnexpectedValue {
                        flag: arg.long.to_string(),
                    });
                }
                parsed.flags.insert(arg.id.to_string());
            }
            ArgKind::Value { .. } => {
                let value = match inline {
                    Some(value) => value,
                    None => {
                        index += 1;
                        argv.get(index)
                            .cloned()
                            .ok_or_else(|| CliError::MissingValue {
                                flag: arg.long.to_string(),
                            })?
                    }
                };
                parsed.pairs.push((arg.id.to_string(), value));
            }
        }
        index += 1;
    }
    // Required positionals and values.
    let (positionals_spec, required): (&[super::spec::PositionalSpec], &[&str]) =
        match command.and_then(|name| spec.find_command(name)) {
            Some(found) => (found.positionals, found.required_args),
            None => (spec.single_positionals, spec.single_required_args),
        };
    let mut required_count = 0;
    for positional in positionals_spec {
        if positional.required && !positional.rest {
            required_count += 1;
        }
    }
    // A required `rest` positional needs at least one value past the
    // fixed ones.
    let fixed = positionals_spec
        .iter()
        .filter(|positional| !positional.rest)
        .count();
    for positional in positionals_spec {
        if positional.rest
            && positional.required
            && parsed.positionals.len() <= fixed.saturating_sub(1)
        {
            return Err(CliError::MissingPositional {
                metavar: positional.metavar.to_string(),
                command: command.unwrap_or(spec.name).to_string(),
            });
        }
    }
    if parsed.positionals.len() < required_count {
        let metavar = positionals_spec
            .get(parsed.positionals.len())
            .map(|positional| positional.metavar)
            .unwrap_or("value");
        return Err(CliError::MissingPositional {
            metavar: metavar.to_string(),
            command: command.unwrap_or(spec.name).to_string(),
        });
    }
    for id in required {
        if parsed.value(id).is_none() {
            let flag = spec.find_arg(id).map(|arg| arg.long).unwrap_or(id);
            return Err(CliError::MissingValue {
                flag: flag.to_string(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::spec::{ArgKind, ArgSpec, CliSpec, CommandSpec, PositionalSpec};
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
        ArgSpec {
            id: "tag",
            long: "tag",
            kind: ArgKind::Value { metavar: "TAG" },
            help: "Repeatable tag.",
        },
    ];

    const COMMANDS: &[CommandSpec] = &[CommandSpec {
        name: "run",
        aliases: &["go"],
        summary: "Run things.",
        args: &["output", "tag"],
        required_args: &["output"],
        positionals: &[PositionalSpec {
            id: "input",
            metavar: "INPUT",
            required: true,
            rest: false,
            help: "Input file.",
        }],
    }];

    const SPEC: &CliSpec = &CliSpec {
        name: "tool",
        about: "Test tool.",
        args: ARGS,
        global_args: &["json"],
        commands: COMMANDS,
        single_positionals: &[],
        single_required_args: &[],
        footer: None,
    };

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_string()).collect()
    }

    #[test]
    fn flags_may_precede_the_command() {
        let parsed = parse_args(
            SPEC,
            &argv(&["--json", "run", "--output", "a.json", "in.txt"]),
        )
        .expect("parse");
        assert_eq!(parsed.command(), Some("run"));
        assert!(parsed.flag("json"));
        assert_eq!(parsed.value("output"), Some("a.json"));
        assert_eq!(parsed.positional(0), Some("in.txt"));
    }

    #[test]
    fn aliases_resolve_to_canonical_names() {
        let parsed =
            parse_args(SPEC, &argv(&["go", "--output", "a.json", "in.txt"])).expect("parse");
        assert_eq!(parsed.command(), Some("run"));
    }

    #[test]
    fn equals_values_and_terminator_work() {
        let parsed =
            parse_args(SPEC, &argv(&["run", "--output=a.json", "--", "--json"])).expect("parse");
        assert_eq!(parsed.value("output"), Some("a.json"));
        assert!(!parsed.flag("json"));
        assert_eq!(parsed.positional(0), Some("--json"));
    }

    #[test]
    fn repeated_values_keep_order_and_first_wins() {
        let parsed = parse_args(
            SPEC,
            &argv(&["run", "--output", "a", "--tag", "x", "--tag", "y", "in"]),
        )
        .expect("parse");
        assert_eq!(parsed.values_of("tag"), vec!["x", "y"]);
        assert_eq!(parsed.value("tag"), Some("x"));
    }

    #[test]
    fn unknown_commands_flags_and_missing_inputs_error() {
        assert!(matches!(
            parse_args(SPEC, &argv(&["nope"])),
            Err(CliError::UnknownCommand { .. })
        ));
        assert!(matches!(
            parse_args(SPEC, &argv(&["run", "--bogus", "--output", "a", "in"])),
            Err(CliError::UnknownFlag { .. })
        ));
        assert!(matches!(
            parse_args(SPEC, &argv(&["run", "--json=x", "--output", "a", "in"])),
            Err(CliError::UnexpectedValue { .. })
        ));
        assert!(matches!(
            parse_args(SPEC, &argv(&["run", "--output"])),
            Err(CliError::MissingValue { .. })
        ));
        assert!(matches!(
            parse_args(SPEC, &argv(&["run", "--output", "a"])),
            Err(CliError::MissingPositional { .. })
        ));
        assert!(matches!(
            parse_args(SPEC, &argv(&["run", "in"])),
            Err(CliError::MissingValue { .. })
        ));
        assert_eq!(CliError::NoCommand.exit_code(), 2);
    }

    #[test]
    fn meta_requests_render_without_parsing() {
        let overview = handle_meta(SPEC, "1.0", &argv(&["--help"])).expect("help");
        assert!(overview.contains("tool") && overview.contains("run"));
        let command = handle_meta(SPEC, "1.0", &argv(&["run", "--help"])).expect("help");
        assert!(command.contains("--output"));
        let version = handle_meta(SPEC, "1.0", &argv(&["-V"])).expect("version");
        assert_eq!(version, "tool 1.0");
        assert!(handle_meta(SPEC, "1.0", &argv(&["run", "--", "--help"])).is_none());
        assert!(handle_meta(SPEC, "1.0", &argv(&["run", "in"])).is_none());
    }

    #[test]
    fn typed_values_report_usage_errors() {
        let parsed =
            parse_args(SPEC, &argv(&["run", "--output", "a", "--tag", "12", "in"])).expect("parse");
        assert_eq!(
            parsed.parsed_value::<usize>("tag").expect("typed"),
            Some(12)
        );
        let bad =
            parse_args(SPEC, &argv(&["run", "--output", "a", "--tag", "x", "in"])).expect("parse");
        assert!(matches!(
            bad.parsed_value::<usize>("tag"),
            Err(CliError::InvalidValue { .. })
        ));
        assert!(bad.required_value("output").is_ok());
        assert!(bad.required_positional(0).is_ok());
    }
}
