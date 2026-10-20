//! Core types and services used by the CLI.
mod config;
/// CLI argument definitions.
pub mod interface;
mod router;
/// Trace server for raw LLM traffic.
pub mod trace;

pub use interface::{Cli, Commands, RepoCommands, check_commit_layout};
pub use router::{Message, Service, Tool, Turn, describe};
