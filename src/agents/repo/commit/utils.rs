//! Helpers to build and tidy the pieces of a commit message.

/// A prompt that can be sent to the model.
#[derive(Clone, Copy, Debug)]
pub enum Prompt {
    /// The types and scopes a commit message can use.
    Convention,
    /// The instructions that describe the project context.
    Context,
    /// The instructions that describe the user hint.
    Hint,
    /// The instructions to write the commit message.
    Instruction,
    /// The shape of the commit message.
    Skeleton,
}

impl Prompt {
    /// Returns the text of the prompt.
    pub fn text(self) -> &'static str {
        match self {
            Self::Convention => include_str!("prompts/convention.txt"),
            Self::Context => include_str!("prompts/context.txt"),
            Self::Hint => include_str!("prompts/hint.txt"),
            Self::Instruction => include_str!("prompts/instruction.txt"),
            Self::Skeleton => include_str!("prompts/skeleton.txt"),
        }
    }
}

/// Comments every line, used to show the convention under the result.
pub fn comment(text: &str) -> String {
    text.lines()
        .map(|line| format!("# {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Ensures there is a blank line between title and body.
pub fn normalize_commit_message(message: &str) -> String {
    let trimmed = message.trim_end();
    let lines: Vec<&str> = trimmed.split('\n').collect();
    if lines.len() <= 1 {
        return trimmed.to_string();
    }
    if !lines[1].is_empty() {
        let mut out = String::new();
        out.push_str(lines[0]);
        out.push('\n');
        out.push('\n');
        out.push_str(&lines[1..].join("\n"));
        return out;
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::normalize_commit_message;

    #[test]
    fn leaves_a_single_line_untouched() {
        assert_eq!(
            normalize_commit_message("fix: something  "),
            "fix: something"
        );
    }

    #[test]
    fn inserts_a_blank_line_between_title_and_body() {
        assert_eq!(
            normalize_commit_message("fix: something\n- a change"),
            "fix: something\n\n- a change"
        );
    }

    #[test]
    fn keeps_an_existing_blank_line() {
        assert_eq!(
            normalize_commit_message("fix: something\n\n- a change"),
            "fix: something\n\n- a change"
        );
    }
}
