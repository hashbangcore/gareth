use crate::util::shell::{Shell, run_shell};

/// Collects git status and staged diff to give context to the model.
pub fn staged_changes() -> String {
    run_commands(&[
        "git diff --cached --quiet && echo 'No staged changes' || (git diff --staged --stat --no-color && git diff --staged --no-color)",
    ])
}

/// Runs a list of shell commands and returns a structured report.
pub fn run_commands(commands: &[&str]) -> String {
    let mut sections = Vec::with_capacity(commands.len());

    for cmd_str in commands {
        let out = run_shell(cmd_str, Shell::Posix);
        sections.push(match &out.error {
            Some(err) => format!(
                "[section]\n[command]\n{}\n[error]\n{}\n[end section]",
                cmd_str, err
            ),
            None => {
                let combined_output = if !out.stderr.is_empty() {
                    format!("{}{}", out.stdout, out.stderr)
                } else {
                    out.stdout.clone()
                };
                format!(
                    "[section]\n[command]\n{}\n[output]\n{}\n[end section]",
                    cmd_str,
                    combined_output.trim_end()
                )
            }
        });
    }

    sections.join("\n\n")
}
