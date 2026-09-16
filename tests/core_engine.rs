//! Core engine coverage: entry points (`analyze`, `compare`, `decode`,
//! batch variants), normalization, lexical, and `core::{config, error,
//! types}` — one module per behavior area. Each module is self-contained;
//! shared setup was intentionally not factored out (every test builds its
//! own engine inline).

#[path = "core_engine/config.rs"]
mod config;
#[path = "core_engine/entry_points.rs"]
mod entry_points;
#[path = "core_engine/error_types.rs"]
mod error_types;
#[path = "core_engine/lexical.rs"]
mod lexical;
#[path = "core_engine/normalization.rs"]
mod normalization;
