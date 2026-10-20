//! The tasks the tool can run.
/// Sends a single prompt to the model.
pub mod pipeline;
/// Works with the current repository.
pub mod repo;
/// Works with the tools of an MCP server.
pub mod mcp;
