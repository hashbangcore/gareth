use std::process::{Command, Stdio};

/// Captured result of a shell command.
pub struct CommandOutput {
    /// Raw standard output.
    pub stdout: String,
    /// Raw standard error.
    pub stderr: String,
    /// Error text when the interpreter could not be started.
    pub error: Option<String>,
}

/// Runs a command string through the POSIX shell and captures its output.
pub fn run_shell(command: &str) -> CommandOutput {
    let mut cmd = Command::new("sh");
    cmd.arg("-c");

    let result = cmd
        .arg(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    match result {
        Ok(out) => from_output(&out.stdout, &out.stderr),
        Err(err) => CommandOutput {
            stdout: String::new(),
            stderr: String::new(),
            error: Some(err.to_string()),
        },
    }
}

/// Builds a `CommandOutput` from raw process output.
fn from_output(stdout: &[u8], stderr: &[u8]) -> CommandOutput {
    CommandOutput {
        stdout: String::from_utf8_lossy(stdout).to_string(),
        stderr: String::from_utf8_lossy(stderr).to_string(),
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_stdout_of_a_successful_command() {
        let out = run_shell("echo hola");
        assert!(out.error.is_none());
        assert_eq!(out.stdout, "hola\n");
    }

    #[test]
    fn captures_stderr_of_a_failing_command() {
        let out = run_shell("echo aviso >&2");
        assert!(out.error.is_none());
        assert_eq!(out.stdout, "");
        assert_eq!(out.stderr, "aviso\n");
    }
}
