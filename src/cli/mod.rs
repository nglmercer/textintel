//! Declarative CLI SDK: spec types ([`spec`]), parsing ([`parse`]),
//! generated help ([`help`]), and the shipped binaries' surfaces
//! ([`catalog`]).
//!
//! Every binary declares its surface as static [`CliSpec`] data. The
//! parser resolves commands, flags, and positionals from that table and
//! the help renderer prints usage from the same source, so handlers
//! read values by id and no flag spelling or usage string is repeated
//! anywhere:
//!
//! ```rust,no_run
//! use textintel::cli::{handle_meta, parse, textintel_spec};
//!
//! let argv: Vec<String> = std::env::args().skip(1).collect();
//! let spec = textintel_spec();
//! if let Some(text) = handle_meta(spec, env!("CARGO_PKG_VERSION"), &argv) {
//!     println!("{text}");
//!     return;
//! }
//! let parsed = parse_args(spec, &argv).expect("usage error");
//! assert_eq!(parsed.command(), Some("eval"));
//! ```

pub mod catalog;
pub mod help;
pub mod parse;
pub mod spec;

pub use catalog::{eval_llm_spec, textintel_spec, train_decision_spec, train_spec};
pub use help::{render_command, render_overview};
pub use parse::{CliError, ParsedArgs, handle_meta, parse_args};
pub use spec::{ArgKind, ArgSpec, CliSpec, CommandSpec, PositionalSpec};
