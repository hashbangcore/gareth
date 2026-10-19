//! Repository tasks and the commands that run them.

use crate::core;

mod commit;
mod git;

pub use commit::dispatch as commit_dispatch;

/// Runs the repository command selected on the command line.
pub async fn dispatch(
    service: &core::Service,
    args: &core::Cli,
    command: &core::RepoCommands,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        core::RepoCommands::Commit {
            hint,
            context,
            convention,
        } => {
            commit_dispatch(
                service,
                args,
                hint.as_deref(),
                context.as_deref(),
                convention.as_deref(),
            )
            .await
        }
    }
}
