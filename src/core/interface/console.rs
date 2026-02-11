use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "gareth",
    author,
    version,
    about = "",
    disable_help_subcommand = true
)]
pub struct CommandLineDirectives {
    /// Prompt passed to the language model
    pub prompt: Vec<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Language model provider
    #[arg(short, long, default_value = "codestral")]
    pub provider: String,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Generate shell completion
    Completion { shell: clap_complete::Shell },

    /// Process a single prompt
    Prompt {
        /// Prompt provided via the command line
        items: Vec<String>,
    },
}
