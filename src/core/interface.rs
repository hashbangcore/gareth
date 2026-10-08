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
    /// Open a minimal chat session
    Chat,

    /// Generate a commit message
    Commit {
        /// Optional prompt used as commit context
        #[arg(trailing_var_arg = true)]
        hint: Vec<String>,
        /// Path to a custom commit convention file
        #[arg(short = 'c', long, env = "GARETH_CONVENTION")]
        convention: Option<String>,
    },

    /// Process a single prompt
    Prompt {
        /// Prompt provided via the command line
        input: Vec<String>,
    },

    /// Generate shell completion
    Completion { shell: clap_complete::Shell },
}
