//! Declarative CLI definitions. Every binary declares its surface as
//! static [`CliSpec`] data; parsing ([`crate::cli::parse`]) and help
//! rendering ([`crate::cli::help`]) both derive from the same tables, so
//! handlers never hardcode flag names or usage strings.

/// Value-taking vs standalone flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgKind {
    /// `--flag`: presence only.
    Flag,
    /// `--flag <metavar>` or `--flag=<metavar>`.
    Value { metavar: &'static str },
}

/// One `--long` option, addressable by `id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArgSpec {
    /// Stable id used by handlers (`parsed.value("model-path")`).
    pub id: &'static str,
    /// Long flag without dashes (`"model-path"` → `--model-path`).
    pub long: &'static str,
    pub kind: ArgKind,
    pub help: &'static str,
}

/// One positional argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PositionalSpec {
    pub id: &'static str,
    /// Displayed as `<metavar>` in usage lines.
    pub metavar: &'static str,
    pub required: bool,
    /// Collects all remaining positionals; only valid on the last entry.
    pub rest: bool,
    pub help: &'static str,
}

/// One subcommand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSpec {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub summary: &'static str,
    /// Registry ids valid for this command (globals are always valid).
    pub args: &'static [&'static str],
    /// Required value ids (subset of `args`); enforced by the parser.
    pub required_args: &'static [&'static str],
    pub positionals: &'static [PositionalSpec],
}

/// Whole CLI surface. An empty `commands` table declares a single-command
/// CLI: every registry arg is valid and every positional is data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CliSpec {
    /// Binary name used in usage lines.
    pub name: &'static str,
    pub about: &'static str,
    /// Shared argument registry.
    pub args: &'static [ArgSpec],
    /// Registry ids valid for every command.
    pub global_args: &'static [&'static str],
    pub commands: &'static [CommandSpec],
    /// Positionals for single-command CLIs (ignored otherwise).
    pub single_positionals: &'static [PositionalSpec],
    /// Required value ids for single-command CLIs (ignored otherwise).
    pub single_required_args: &'static [&'static str],
    /// Extra paragraph rendered under the overview (contracts, notes).
    pub footer: Option<&'static str>,
}

/// Reserved `--help` long flag. Handled by
/// [`crate::cli::parse::handle_meta`], never by handlers.
pub const HELP_LONG: &str = "help";
/// Reserved `-h` short flag.
pub const HELP_SHORT: char = 'h';
/// Reserved `--version` long flag.
pub const VERSION_LONG: &str = "version";
/// Reserved `-V` short flag.
pub const VERSION_SHORT: char = 'V';

impl CliSpec {
    /// Resolve a command word to its spec, following aliases.
    pub fn find_command(&self, word: &str) -> Option<&CommandSpec> {
        self.commands
            .iter()
            .find(|command| command.name == word || command.aliases.contains(&word))
    }

    /// Resolve a registry id to its spec.
    pub fn find_arg(&self, id: &str) -> Option<&ArgSpec> {
        self.args.iter().find(|arg| arg.id == id)
    }

    /// Resolve a `--long` flag to its spec.
    pub fn find_long(&self, long: &str) -> Option<&ArgSpec> {
        self.args.iter().find(|arg| arg.long == long)
    }

    /// Structural integrity: unique ids/longs/names, dangling references,
    /// positional shape, non-empty help. Binaries run this in tests, not
    /// at startup (specs are programmer-authored constants).
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("CLI name must not be empty".to_string());
        }
        let mut ids = std::collections::BTreeSet::new();
        let mut longs = std::collections::BTreeSet::new();
        for arg in self.args {
            if arg.id.trim().is_empty() || arg.long.trim().is_empty() {
                return Err("argument id and long must not be empty".to_string());
            }
            if arg.long.starts_with('-') {
                return Err(format!(
                    "argument long {:?} must not include dashes",
                    arg.long
                ));
            }
            if !ids.insert(arg.id) {
                return Err(format!("duplicate argument id {:?}", arg.id));
            }
            if !longs.insert(arg.long) {
                return Err(format!("duplicate argument long {:?}", arg.long));
            }
            if arg.long == HELP_LONG || arg.long == VERSION_LONG {
                return Err(format!("argument long {:?} is reserved", arg.long));
            }
            if arg.help.trim().is_empty() {
                return Err(format!("argument {:?} needs help text", arg.id));
            }
            if let ArgKind::Value { metavar } = arg.kind
                && metavar.trim().is_empty()
            {
                return Err(format!("argument {:?} needs a metavar", arg.id));
            }
        }
        for id in self.global_args {
            if self.find_arg(id).is_none() {
                return Err(format!("global argument {id:?} is not in the registry"));
            }
        }
        if self.commands.is_empty() {
            if !self.global_args.is_empty() {
                return Err(
                    "single-command CLIs take globals from the registry directly".to_string(),
                );
            }
            check_positionals("command", self.single_positionals)?;
            for id in self.single_required_args {
                require_value(self, "command", id)?;
            }
            return Ok(());
        }
        if !self.single_positionals.is_empty() || !self.single_required_args.is_empty() {
            return Err("single-command fields need an empty commands table".to_string());
        }
        let mut names = std::collections::BTreeSet::new();
        for command in self.commands {
            if command.name.trim().is_empty() {
                return Err("command name must not be empty".to_string());
            }
            if !names.insert(command.name) {
                return Err(format!("duplicate command {:?}", command.name));
            }
            for alias in command.aliases {
                if alias.trim().is_empty() {
                    return Err(format!("command {:?} has a blank alias", command.name));
                }
                if !names.insert(*alias) {
                    return Err(format!("duplicate command name or alias {alias:?}"));
                }
            }
            if command.summary.trim().is_empty() {
                return Err(format!("command {:?} needs a summary", command.name));
            }
            for id in command.args {
                if self.find_arg(id).is_none() {
                    return Err(format!(
                        "command {:?} references unknown argument {id:?}",
                        command.name
                    ));
                }
            }
            for id in command.required_args {
                if !command.args.contains(id) && !self.global_args.contains(id) {
                    return Err(format!(
                        "command {:?} requires {id:?}, which it does not accept",
                        command.name
                    ));
                }
                require_value(self, command.name, id)?;
            }
            check_positionals(command.name, command.positionals)?;
        }
        Ok(())
    }
}

fn require_value(spec: &CliSpec, command: &str, id: &str) -> Result<(), String> {
    match spec.find_arg(id) {
        Some(arg) if matches!(arg.kind, ArgKind::Value { .. }) => Ok(()),
        Some(_) => Err(format!("command {command:?} cannot require flag {id:?}")),
        None => Err(format!(
            "command {command:?} requires unknown argument {id:?}"
        )),
    }
}

fn check_positionals(command: &str, positionals: &[PositionalSpec]) -> Result<(), String> {
    let mut optional_seen = false;
    for (index, positional) in positionals.iter().enumerate() {
        if positional.metavar.trim().is_empty() {
            return Err(format!(
                "command {command:?} has a blank positional metavar"
            ));
        }
        if positional.help.trim().is_empty() {
            return Err(format!(
                "command {command:?} positional <{}> needs help text",
                positional.metavar
            ));
        }
        if positional.rest && index + 1 != positionals.len() {
            return Err(format!(
                "command {command:?}: only the last positional may collect the rest"
            ));
        }
        if !positional.required {
            optional_seen = true;
        } else if optional_seen {
            return Err(format!(
                "command {command:?}: required positional <{}> follows an optional one",
                positional.metavar
            ));
        }
    }
    Ok(())
}
