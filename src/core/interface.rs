use crate::util::terminal::OutputFormat;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "gareth",
    author,
    version,
    about = "CLI for interacting with language models",
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Prompt passed to the language model
    pub prompt: Vec<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Start a trace server to print raw LLM traffic
    #[arg(short = 't', long)]
    pub trace: bool,

    /// Force output format for prompt (plain | markdown)
    #[arg(short = 'o', long, global = true, value_parser = ["plain", "markdown"])]
    pub output: Option<String>,
}

impl Cli {
    /// Returns the requested output format, if any.
    pub fn output_format(&self) -> Option<OutputFormat> {
        self.output.as_deref().and_then(OutputFormat::from_name)
    }
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Work with the current repository
    Repo {
        #[command(subcommand)]
        command: RepoCommands,
    },

    /// Process a single prompt
    Prompt {
        /// Prompt provided via the command line
        input: Vec<String>,
    },

    /// Generate shell completion
    Completion { shell: clap_complete::Shell },
}

#[derive(Subcommand, Debug)]
pub enum RepoCommands {
    /// Generate a commit message
    Commit {
        /// Hint text or path to a text file
        #[arg(long)]
        hint: Option<String>,
        /// Project context text or path to a text file
        #[arg(long)]
        context: Option<String>,
        /// Path to a custom commit convention file
        #[arg(short = 'c', long, env = "GARETH_CONVENTION")]
        convention: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses the arguments and returns the commit options.
    fn commit_options(args: &[&str]) -> (Option<String>, Option<String>) {
        let cli = Cli::try_parse_from(args).unwrap();
        let Some(Commands::Repo {
            command: RepoCommands::Commit { hint, context, .. },
        }) = cli.command
        else {
            panic!("expected a commit command");
        };
        (hint, context)
    }

    #[test]
    fn reads_the_hint_and_context_flags() {
        let (hint, context) = commit_options(&[
            "gareth",
            "repo",
            "commit",
            "--hint",
            "es un refactor",
            "--context",
            "proyecto rust",
        ]);
        assert_eq!(hint.as_deref(), Some("es un refactor"));
        assert_eq!(context.as_deref(), Some("proyecto rust"));
    }

    #[test]
    fn leaves_both_options_empty_when_omitted() {
        let (hint, context) = commit_options(&["gareth", "repo", "commit"]);
        assert!(hint.is_none());
        assert!(context.is_none());
    }

    #[test]
    fn no_longer_accepts_a_positional_hint() {
        assert!(Cli::try_parse_from(["gareth", "repo", "commit", "es un refactor"]).is_err());
    }
}
