use std::ffi::OsStr;
use std::process::Command;

/// The hash of the empty tree, used to diff a repository that has no commits.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// What a git command returned.
struct GitOutput {
    /// True when git finished successfully.
    success: bool,
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
            success: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        },
        Err(err) => GitOutput {
            success: false,
            stdout: String::new(),
            stderr: err.to_string(),
        },
    }
}

/// Returns the reference the changes are measured against.
///
/// A repository without commits is measured against the empty tree.
fn base() -> String {
    if git(["rev-parse", "--verify", "--quiet", "HEAD"]).success {
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
                display: "git diff --cached --quiet".to_string(),
                args: vec!["diff".into(), "--cached".into(), "--quiet".into()],
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
pub fn staged_changes(files: &[String]) -> String {
    let steps = steps(files);

    let mut commands: Vec<String> = Vec::with_capacity(steps.len());
    let mut outputs: Vec<String> = Vec::with_capacity(steps.len());

    for (index, step) in steps.iter().enumerate() {
        commands.push(step.display.clone());
        let out = git(&step.args);

        // The first command only asks whether there is anything to describe.
        if index == 0 && out.success {
            return section(&commands, "No staged changes");
        }

        let mut output = out.stdout;
        if !out.stderr.trim().is_empty() {
            output.push_str(&out.stderr);
        }
        outputs.push(output);
    }

    section(&commands, &outputs.join("\n"))
}

/// Wraps a command list and its output in the markers the model reads.
fn section(commands: &[String], output: &str) -> String {
    format!(
        "[section]\n[command]\n{}\n[output]\n{}\n[end section]",
        commands.join("\n"),
        output.trim_end()
    )
}
