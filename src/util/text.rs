//! Helpers to tokenize text and to shape the blocks of a prompt.

/// Splits a command line into arguments, honoring quotes and backslash escapes.
pub fn split_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escape = false;

    for ch in input.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }

        if ch == '\\' && quote != Some('\'') {
            escape = true;
            continue;
        }

        if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }

        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }

        if ch.is_whitespace() {
            if !current.is_empty() {
                args.push(std::mem::take(&mut current));
            }
            continue;
        }

        current.push(ch);
    }

    if !current.is_empty() {
        args.push(current);
    }

    args
}

/// Returns true if the token looks like a file path.
pub fn looks_like_path(token: &str) -> bool {
    token.starts_with('/')
        || token.starts_with("./")
        || token.starts_with("../")
        || token.starts_with("~/")
}

/// Wraps a block in section markers so the model can parse it apart.
pub fn section(title: &str, content: &str) -> String {
    let title = title.to_uppercase();
    format!(":: {title} ::\n{content}\n:: END {title} ::")
}

/// Prefixes every line of a block, preserving its trailing newline.
pub fn indent_block(content: &str, prefix: &str) -> String {
    if content.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    let mut lines = content.lines().peekable();
    while let Some(line) = lines.next() {
        out.push_str(prefix);
        out.push_str(line);
        if lines.peek().is_some() {
            out.push('\n');
        }
    }
    if content.ends_with('\n') {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(prefix);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_quotes_and_escapes() {
        assert_eq!(split_args(""), Vec::<String>::new());
        assert_eq!(split_args("   "), Vec::<String>::new());
        assert_eq!(split_args("read two.txt"), vec!["read", "two.txt"]);
        assert_eq!(split_args("\"a file\" and"), vec!["a file", "and"]);
        assert_eq!(split_args("a\\ b"), vec!["a b"]);
        // An empty quoted token is dropped.
        assert_eq!(split_args("\"\" x"), vec!["x"]);
        // An unclosed quote swallows the rest of the input.
        assert_eq!(split_args("\"sin cerrar"), vec!["sin cerrar"]);
    }

    #[test]
    fn backslash_does_not_escape_inside_single_quotes() {
        assert_eq!(split_args("'a\\b'"), vec!["a\\b"]);
        assert_eq!(split_args("'con espacio'"), vec!["con espacio"]);
    }

    #[test]
    fn detects_path_candidates() {
        assert!(looks_like_path("/etc/hosts"));
        assert!(looks_like_path("./src/main.rs"));
        assert!(looks_like_path("../arriba"));
        assert!(looks_like_path("~/notes.txt"));

        assert!(!looks_like_path("notes.txt"));
        assert!(!looks_like_path("src/main.rs"));
        assert!(!looks_like_path(""));
    }

    #[test]
    fn indents_every_line_and_keeps_the_trailing_newline() {
        assert_eq!(indent_block("", "  "), "");
        assert_eq!(indent_block("one", "  "), "  one");
        assert_eq!(indent_block("one\ntwo\n", "  "), "  one\n  two\n  ");
    }

    #[test]
    fn indents_blank_lines_too() {
        assert_eq!(indent_block("one\n\ntwo", "  "), "  one\n  \n  two");
    }
}

#[cfg(test)]
mod section_tests {
    use super::section;

    #[test]
    fn wraps_content_in_uppercase_markers() {
        assert_eq!(
            section("user hint", "hello"),
            ":: USER HINT ::\nhello\n:: END USER HINT ::"
        );
    }
}
