//! Printing the output of the model in the terminal.

use std::io::IsTerminal;
use termimad::MadSkin;

/// Resets terminal styling.
const RESET: &str = "\x1b[0m";
/// Turns on bold styling.
const BOLD: &str = "\x1b[1m";

/// Renders markdown to terminal-friendly output (auto-detects terminal).
pub fn render_markdown(response: &str) -> String {
    render_markdown_auto(response, std::io::stdout().is_terminal())
}

/// Renders markdown depending on whether the destination is a terminal.
fn render_markdown_auto(response: &str, is_tty: bool) -> String {
    if !is_tty {
        return response.to_string();
    }
    let skin = MadSkin::default();
    skin.term_text(response).to_string()
}

/// Prints a prompt as it is, adding a final newline when it lacks one.
pub fn print_prompt(prompt: &str) {
    print!("{prompt}");
    if !prompt.ends_with('\n') {
        println!();
    }
}

/// Prints a bold label followed by a text block, used by verbose mode.
pub fn print_labeled(label: &str, text: &str) {
    println!("{BOLD}{label}:{RESET}\n\n{text}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn piped_output_is_not_rendered() {
        assert_eq!(render_markdown_auto("**crudo**", false), "**crudo**");
        assert!(!render_markdown_auto("**crudo**", true).contains("**"));
    }
}
