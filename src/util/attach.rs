use std::env;
use std::fs;

pub use super::files::Attachment;
use super::files::expand_path;
use super::text::{indent_block, looks_like_path, split_args};

/// Reads a path only when it is a regular file, ignoring directories and pipes.
fn read_regular_file(path: &str) -> Option<String> {
    match fs::metadata(path) {
        Ok(meta) if meta.is_file() => fs::read_to_string(path).ok(),
        _ => None,
    }
}

/// Extracts file attachments from tokenized input.
fn extract_attachments_from_tokens(
    tokens: &[String],
    read: impl Fn(&str) -> Option<String>,
    home: Option<&str>,
) -> (Vec<String>, Vec<Attachment>) {
    let mut remaining = Vec::new();
    let mut attachments = Vec::new();

    for token in tokens {
        if !looks_like_path(token) {
            remaining.push(token.clone());
            continue;
        }

        match read(&expand_path(token, home)) {
            Some(content) => attachments.push(Attachment {
                path: token.clone(),
                content,
            }),
            None => remaining.push(token.clone()),
        }
    }

    (remaining, attachments)
}

/// Extracts file attachments from a raw input string.
pub fn extract_attachments_from_input(input: &str) -> Vec<Attachment> {
    let home = env::var("HOME").ok();
    let tokens = split_args(input);
    let (_remaining, attachments) =
        extract_attachments_from_tokens(&tokens, read_regular_file, home.as_deref());
    attachments
}

/// Formats attachments into a single block, compatible with stdin attachments.
pub fn format_attachments(attachments: &[Attachment]) -> Option<String> {
    if attachments.is_empty() {
        return None;
    }
    let mut out = String::new();
    for attachment in attachments {
        out.push_str("\n-- FILE: ");
        out.push_str(&attachment.path);
        out.push_str(" --\n");
        out.push_str(&attachment.content);
        out.push('\n');
    }
    Some(out)
}

/// Formats stdin and file attachments into a single attached files block.
pub fn format_attached_files(stdin: Option<&str>, attachments: &[Attachment]) -> Option<String> {
    let mut sections = Vec::new();
    if let Some(content) = stdin.filter(|content| !content.trim().is_empty()) {
        sections.push(format!(
            "-- FILE: STDIN --\n{}",
            indent_block(content, "      ")
        ));
    }
    for attachment in attachments {
        sections.push(format!(
            "-- FILE: {} --\n{}",
            attachment.path,
            indent_block(&attachment.content, "      ")
        ));
    }
    if sections.is_empty() {
        return None;
    }
    let mut out = String::new();
    out.push_str(":: ATTACHED FILES ::\n\n");
    out.push_str(&sections.join("\n\n"));
    out.push_str("\n\n:: END ATTACHED FILES ::");
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a fake reader that serves the given path/content pairs.
    fn reader_for(files: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let files = files.to_vec();
        move |path| {
            files
                .iter()
                .find(|(candidate, _)| *candidate == path)
                .map(|(_, content)| content.to_string())
        }
    }

    #[test]
    fn extracts_only_readable_paths_and_keeps_the_original_token() {
        let read = reader_for(&[("/etc/hosts", "127.0.0.1"), ("./notas.txt", "primera nota")]);
        let tokens = split_args("lee /etc/hosts y ./notas.txt y nada");
        let (remaining, attachments) = extract_attachments_from_tokens(&tokens, read, None);

        assert_eq!(remaining, vec!["lee", "y", "y", "nada"]);
        assert_eq!(attachments.len(), 2);
        assert_eq!(attachments[0].path, "/etc/hosts");
        assert_eq!(attachments[0].content, "127.0.0.1");
        assert_eq!(attachments[1].path, "./notas.txt");
        assert_eq!(attachments[1].content, "primera nota");
    }

    #[test]
    fn unreadable_paths_stay_in_the_input() {
        let read = reader_for(&[]);
        let tokens = split_args("lee /no/existe.txt");
        let (remaining, attachments) = extract_attachments_from_tokens(&tokens, read, None);

        assert_eq!(remaining, vec!["lee", "/no/existe.txt"]);
        assert!(attachments.is_empty());
    }

    #[test]
    fn reads_the_expanded_path_but_keeps_the_token_in_the_attachment() {
        let read = reader_for(&[("/home/gareth/notas.txt", "contenido")]);
        let tokens = split_args("lee ~/notas.txt");
        let (remaining, attachments) =
            extract_attachments_from_tokens(&tokens, read, Some("/home/gareth"));

        assert_eq!(remaining, vec!["lee"]);
        assert_eq!(attachments[0].path, "~/notas.txt");
        assert_eq!(attachments[0].content, "contenido");
    }

    #[test]
    fn expands_home_only_when_available() {
        assert_eq!(
            expand_path("~/notas.txt", Some("/home/gareth")),
            "/home/gareth/notas.txt"
        );
        assert_eq!(expand_path("~/notas.txt", None), "~/notas.txt");
        assert_eq!(
            expand_path("/etc/hosts", Some("/home/gareth")),
            "/etc/hosts"
        );
        assert_eq!(expand_path("notas.txt", Some("/home/gareth")), "notas.txt");
    }

    #[test]
    fn formats_attachments_as_file_blocks() {
        assert!(format_attachments(&[]).is_none());

        let attachments = vec![Attachment {
            path: "notas.txt".to_string(),
            content: "hola".to_string(),
        }];
        assert_eq!(
            format_attachments(&attachments).as_deref(),
            Some("\n-- FILE: notas.txt --\nhola\n")
        );
    }

    #[test]
    fn formats_stdin_before_attachments() {
        assert!(format_attached_files(Some("  \n"), &[]).is_none());
        assert!(format_attached_files(None, &[]).is_none());

        let attachments = vec![Attachment {
            path: "notas.txt".to_string(),
            content: "uno\ndos".to_string(),
        }];
        let block = format_attached_files(Some("por pipe"), &attachments).unwrap();

        assert!(block.starts_with(":: ATTACHED FILES ::\n\n-- FILE: STDIN --\n"));
        assert!(block.contains("      por pipe"));
        assert!(block.contains("-- FILE: notas.txt --\n      uno\n      dos"));
        assert!(block.ends_with("\n\n:: END ATTACHED FILES ::"));
    }
}
