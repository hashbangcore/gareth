use clap::{ArgMatches, Parser, Subcommand};

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
        /// Files or directories to limit the diff to
        #[arg(value_name = "FILE")]
        files: Vec<String>,
    },
}

/// Rejects a command line where the file arguments are split by an option.
///
/// The options may come before or after the files, but the files themselves
/// have to be given together.
pub fn check_commit_layout(matches: &ArgMatches) -> Result<(), String> {
    let Some(commit) = matches
        .subcommand_matches("repo")
        .and_then(|repo| repo.subcommand_matches("commit"))
    else {
        return Ok(());
    };

    let files: Vec<usize> = commit
        .indices_of("files")
        .map(|it| it.collect())
        .unwrap_or_default();
    let (Some(first), Some(last)) = (files.first(), files.last()) else {
        return Ok(());
    };

    let split = ["hint", "context", "convention"].iter().any(|name| {
        commit
            .indices_of(name)
            .is_some_and(|mut indices| indices.any(|index| index > *first && index < *last))
    });

    if split {
        return Err("the file arguments must be given together".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    /// Parses the arguments and returns the commit options.
    fn commit_options(args: &[&str]) -> (Option<String>, Option<String>, Vec<String>) {
        let cli = Cli::try_parse_from(args).unwrap();
        let Some(Commands::Repo {
            command:
                RepoCommands::Commit {
                    hint,
                    context,
                    files,
                    ..
                },
        }) = cli.command
        else {
            panic!("expected a commit command");
        };
        (hint, context, files)
    }

    /// Parses the arguments and reports whether the file group is accepted.
    fn file_group_is_accepted(args: &[&str]) -> Result<(), String> {
        let matches = Cli::command().get_matches_from(args);
        check_commit_layout(&matches)
    }

    #[test]
    fn reads_the_hint_and_context_flags() {
        let (hint, context, files) = commit_options(&[
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
        assert!(files.is_empty());
    }

    #[test]
    fn leaves_both_options_empty_when_omitted() {
        let (hint, context, files) = commit_options(&["gareth", "repo", "commit"]);
        assert!(hint.is_none());
        assert!(context.is_none());
        assert!(files.is_empty());
    }

    #[test]
    fn takes_a_positional_argument_as_a_file_and_not_as_a_hint() {
        let (hint, _, files) = commit_options(&["gareth", "repo", "commit", "es un refactor"]);
        assert!(hint.is_none());
        assert_eq!(files, vec!["es un refactor"]);
    }

    #[test]
    fn reads_the_file_arguments() {
        let (_, _, files) = commit_options(&["gareth", "repo", "commit", "src/a.rs", "src/b.rs"]);
        assert_eq!(files, vec!["src/a.rs", "src/b.rs"]);
    }

    #[test]
    fn reads_the_files_next_to_the_options() {
        let (hint, context, files) = commit_options(&[
            "gareth",
            "repo",
            "commit",
            "--hint",
            "un hint",
            "--context",
            "un contexto",
            "src/a.rs",
            "src/b.rs",
        ]);
        assert_eq!(hint.as_deref(), Some("un hint"));
        assert_eq!(context.as_deref(), Some("un contexto"));
        assert_eq!(files, vec!["src/a.rs", "src/b.rs"]);
    }

    #[test]
    fn accepts_the_files_before_the_options() {
        assert!(
            file_group_is_accepted(&["gareth", "repo", "commit", "src/a.rs", "--hint", "h"])
                .is_ok()
        );
    }

    #[test]
    fn accepts_the_files_after_the_options() {
        assert!(
            file_group_is_accepted(&["gareth", "repo", "commit", "--hint", "h", "src/a.rs"])
                .is_ok()
        );
    }

    #[test]
    fn rejects_the_files_split_by_an_option() {
        let err = file_group_is_accepted(&[
            "gareth", "repo", "commit", "src/a.rs", "--hint", "h", "src/b.rs",
        ])
        .unwrap_err();
        assert!(err.contains("must be given together"), "err was {err}");
    }

    #[test]
    fn accepts_a_command_without_files() {
        assert!(file_group_is_accepted(&["gareth", "repo", "commit", "--hint", "h"]).is_ok());
    }
}
