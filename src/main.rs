//! Gareth, a command line tool for language models.

#![deny(clippy::correctness)]
#![warn(missing_docs)]

mod agents;
mod core;
mod util;

use agents as agent;
use clap::CommandFactory;
use clap::FromArgMatches;
use clap_complete::generate;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = util::get_stdin()?;
    let matches = core::Cli::command().get_matches();
    core::check_commit_layout(&matches)?;
    let args = core::Cli::from_arg_matches(&matches)?;

    if args.trace && (args.command.is_some() || !args.prompt.is_empty()) {
        let mut cmd = core::Cli::command();
        cmd.error(
            clap::error::ErrorKind::ArgumentConflict,
            "--trace cannot be used with subcommands or prompt input",
        )
        .exit();
    }

    if args.trace {
        core::trace::run_trace_server().await?;
        return Ok(());
    }

    let service = core::Service::new(args.verbose)?;

    execute(&service, &args, stdin).await
}

async fn execute(
    service: &core::Service,
    args: &core::Cli,
    stdin: String,
) -> Result<(), Box<dyn std::error::Error>> {
    match &args.command {
        Some(core::Commands::Repo { command }) => {
            agent::repo::dispatch(service, args, command).await?
        }
        Some(core::Commands::Prompt { input }) => {
            let input_text = input.join(" ");
            agent::pipeline::dispatch(service, args, &input_text, stdin).await?
        }
        Some(core::Commands::Completion { shell }) => {
            if args.dump {
                return Err("the completion command does not send anything to the model".into());
            }
            let mut cmd = core::Cli::command();
            generate(*shell, &mut cmd, "gareth", &mut std::io::stdout());
        }
        None => {
            if args.prompt.is_empty() {
                core::Cli::command().print_help()?;
            } else {
                let prompt_text = args.prompt.join(" ");
                agent::pipeline::dispatch(service, args, &prompt_text, stdin).await?;
            }
        }
    }

    Ok(())
}
