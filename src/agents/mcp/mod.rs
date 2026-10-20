//! Work with the tools of an MCP server.

pub mod access;
mod client;
mod config;
mod protocol;

pub use access::{Options, dispatch};
