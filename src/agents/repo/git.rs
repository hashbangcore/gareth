//! The changes of the repository, as git reports them.

use std::ffi::OsStr;
use std::process::Command;

/// The hash of the empty tree, used to diff a repository that has no commits.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// What a git command returned.
struct GitOutput {
    /// The exit code of git, if it finished.
    code: Option<i32>,
    /// What git wrote to the standard output.
    stdout: String,
    /// What git wrote to the standard error.
    stderr: String,
}

/// A git command to run, with the line shown to the model.
struct Step {
    /// The command as it is shown to the model.
    display: String,
    /// The arguments passed to git.
    args: Vec<String>,
}

/// Runs git with the given arguments.
fn git<I, S>(args: I) -> GitOutput
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    match Command::new("git").args(args).output() {
        Ok(out) => GitOutput {
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        },
        Err(err) => GitOutput {
            code: None,
            stdout: String::new(),
            stderr: err.to_string(),
        },
    }
}

/// Builds the message that explains why git failed.
fn failure_message(out: &GitOutput) -> String {
    let message = out.stderr.trim();
    if message.is_empty() {
        "git failed".to_string()
    } else {
        message.to_string()
    }
}

/// Fails when the working directory is not inside a repository.
fn check_repository() -> Result<(), String> {
    let out = git(["rev-parse", "--is-inside-work-tree"]);
    if out.code != Some(0) {
        return Err(failure_message(&out));
    }
    Ok(())
}

/// Returns the reference the changes are measured against.
///
/// A repository without commits is measured against the empty tree.
fn base() -> String {
    if git(["rev-parse", "--verify", "--quiet", "HEAD"]).code == Some(0) {
        "HEAD".to_string()
    } else {
        EMPTY_TREE.to_string()
    }
}

/// Builds the arguments of a diff limited to the given files.
fn diff_args(head: &[&str], files: &[String]) -> Vec<String> {
    let mut args: Vec<String> = head.iter().map(|arg| arg.to_string()).collect();
    args.push("--".to_string());
    args.extend(files.iter().cloned());
    args
}

/// Builds the commands that describe the changes.
///
/// Without files the whole index is described. With files only those paths
/// are described, which is what `git commit <files>` would commit.
fn steps(files: &[String]) -> Vec<Step> {
    if files.is_empty() {
        return vec![
            Step {
                display: "git diff --staged --quiet".to_string(),
                args: vec!["diff".into(), "--staged".into(), "--quiet".into()],
            },
            Step {
                display: "git diff --staged --stat --no-color".to_string(),
                args: vec![
                    "diff".into(),
                    "--staged".into(),
                    "--stat".into(),
                    "--no-color".into(),
                ],
            },
            Step {
                display: "git diff --staged --no-color".to_string(),
                args: vec!["diff".into(), "--staged".into(), "--no-color".into()],
            },
        ];
    }

    let base = base();
    let listed = files
        .iter()
        .map(|file| format!("'{file}'"))
        .collect::<Vec<_>>()
        .join(" ");

    vec![
        Step {
            display: format!("git diff {base} --quiet -- {listed}"),
            args: diff_args(&["diff", &base, "--quiet"], files),
        },
        Step {
            display: format!("git diff {base} --stat --no-color -- {listed}"),
            args: diff_args(&["diff", &base, "--stat", "--no-color"], files),
        },
        Step {
            display: format!("git diff {base} --no-color -- {listed}"),
            args: diff_args(&["diff", &base, "--no-color"], files),
        },
    ]
}

/// Collects the changes to describe to the model.
///
/// `Ok(None)` means there is nothing to describe, `Err` carries the message
/// git reported.
///
/// # Errors
///
/// Returns the message git reported when a command fails.
pub fn staged_changes(files: &[String]) -> Result<Option<String>, String> {
    check_repository()?;

    let steps = steps(files);

    let mut commands: Vec<String> = Vec::with_capacity(steps.len());
    let mut outputs: Vec<String> = Vec::with_capacity(steps.len());

    for (index, step) in steps.iter().enumerate() {
        commands.push(step.display.clone());
        let out = git(&step.args);

        if index == 0 {
            // The first command only asks whether there is anything to describe.
            // Git answers 1 when there are differences and fails above that.
            match out.code {
                Some(0) => return Ok(None),
                Some(1) => {}
                _ => return Err(failure_message(&out)),
            }
        } else if out.code != Some(0) {
            return Err(failure_message(&out));
        }

        let mut output = out.stdout;
        if !out.stderr.trim().is_empty() {
            output.push_str(&out.stderr);
        }
        outputs.push(output);
    }

    Ok(Some(section(&commands, &outputs.join("\n"))))
}

/// Checks that the paths are known to git, the way `git commit <paths>` needs.
///
/// # Errors
///
/// Returns the message git reported when a path is unknown or git fails.
pub fn check_files(files: &[String]) -> Result<(), String> {
    if files.is_empty() {
        return Ok(());
    }

    let mut args: Vec<String> = vec!["ls-files".into(), "--error-unmatch".into(), "--".into()];
    args.extend(files.iter().cloned());

    let out = git(&args);
    if out.code != Some(0) {
        return Err(failure_message(&out));
    }
    Ok(())
}

/// Wraps a command list and its output in the markers the model reads.
fn section(commands: &[String], output: &str) -> String {
    format!(
        "[section]\n[command]\n{}\n[output]\n{}\n[end section]",
        commands.join("\n"),
        output.trim_end()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_the_index_when_no_files_are_given() {
        let steps = steps(&[]);
        assert_eq!(steps.len(), 3);
        assert!(steps[1].args.contains(&"--staged".to_string()));
        assert!(!steps[1].args.contains(&"--".to_string()));
    }

    #[test]
    fn describes_the_files_when_they_are_given() {
        let steps = steps(&["src/a.rs".to_string()]);
        assert!(steps[1].args.contains(&"--".to_string()));
        assert!(steps[1].args.contains(&"src/a.rs".to_string()));
        assert!(steps[1].display.contains("'src/a.rs'"));
    }

    #[test]
    fn accepts_an_empty_file_list() {
        assert!(check_files(&[]).is_ok());
    }

    #[ignore = "needs git in the PATH"]
    #[test]
    fn refuses_a_path_that_git_does_not_know() {
        let err = check_files(&["gareth-no-existe-jamas.txt".to_string()]).unwrap_err();
        assert!(
            err.contains("gareth-no-existe-jamas.txt") || err.contains("not a git repository"),
            "err was {err}"
        );
    }
}
