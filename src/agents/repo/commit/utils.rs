pub fn prompt(prompt: &str) -> String {
    match prompt {
        "convention" => include_str!("prompts/convention.txt").to_string(),
        "instruction" => include_str!("prompts/instruction.txt").to_string(),
        "skeleton" => include_str!("prompts/skeleton.txt").to_string(),
        _ => panic!("Unknown prompt: {prompt}"),
    }
}

/// Comments every line, used to show the convention under the result.
pub fn comment(text: &str) -> String {
    text.lines()
        .map(|line| format!("# {}", line))
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
