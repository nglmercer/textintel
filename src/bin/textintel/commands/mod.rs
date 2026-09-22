//! `textintel` subcommand handlers, one module per area. Handlers take
//! [`ParsedArgs`] and read flags/positionals by spec id; usage,
//! validation, and `--help` come from the [`textintel::cli`] spec.
//!
//! [`ParsedArgs`]: textintel::cli::ParsedArgs

pub mod common;
pub mod compare;
pub mod decision;
pub mod eval;
pub mod generate;
pub mod resources;
pub mod store;
pub mod text;
