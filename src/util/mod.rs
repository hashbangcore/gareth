//! Shared helpers used across agents, grouped by domain.
//!
//! This layer sits below `core` and `agents`: it must not depend on them.
//!
//! - [`shell`]: running shell commands and capturing their output.
//! - [`terminal`]: rendering and printing model output for the terminal.
//! - [`files`]: reading files from disk and deciding whether they hold text.
//! - [`attach`]: turning attachments into prompt context blocks.
//! - [`text`]: tokenizing, path detection, block indentation and prompt sections.
//! - [`env`]: environment variables.
//! - [`io`]: standard input.

pub mod attach;
pub mod env;
pub mod files;
pub mod io;
pub mod shell;
pub mod terminal;
pub mod text;

pub use io::get_stdin;
pub use text::section;
