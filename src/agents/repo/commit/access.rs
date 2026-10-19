use crate::core;
use crate::util::files;
use crate::util::section;

use super::utils::{comment, normalize_commit_message, prompt as prompt_for};
use crate::agents::repo::git::staged_changes;

fn template(
    hint: Option<&str>,
    context: Option<&str>,
    convention_text: &str,
    status_text: &str,
) -> String {
    let mut sections: Vec<(&str, String)> = vec![
        ("INSTRUCTION", prompt_for("instruction")),
        ("CONVENTION", convention_text.to_string()),
        ("SKELETON", prompt_for("skeleton")),
    ];

    if let Some(context) = context {
        sections.push(("PROJECT CONTEXT", context.to_string()));
    }

    if let Some(hint) = hint {
        sections.push(("USER HINT", hint.to_string()));
    }

    sections.push(("REPOSITORY STATUS", status_text.to_string()));

    sections
        .iter()
        .map(|(title, content)| section(title, content))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Resolves a value that may be text or a text file into the text to inject.
///
/// A value with no content is treated as if it had not been given, so an
/// empty flag never injects an empty block.
fn resolve_text(value: Option<&str>) -> Result<Option<String>, files::ReadError> {
    let resolved = value.map(files::resolve_value).transpose()?;
    Ok(resolved
        .map(|value| value.text)
        .filter(|text| !text.trim().is_empty()))
}

/// Builds a commit prompt, calls the model, and prints the final message.
pub async fn dispatch(
    service: &core::Service,
    args: &core::Cli,
    hint: Option<&str>,
    context: Option<&str>,
    convention_path: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let convention_text = if let Some(path) = convention_path {
        std::fs::read_to_string(path)?
    } else {
        prompt_for("convention")
    };

    let hint = resolve_text(hint)?;
    let context = resolve_text(context)?;

    let prompt = template(
        hint.as_deref(),
        context.as_deref(),
        &convention_text,
        &staged_changes(),
    );

    if args.verbose {
        println!("{}\n\n", prompt);
    }

    let result = service.complete(&prompt).await?;
    let result = normalize_commit_message(&result);

    // TODO: manejar de forma más segura
    if result.contains("Error: no changes staged for commit") {
        println!("{}", result);
    } else {
        println!("{}\n\n\n{}", result.trim_end(), comment(&convention_text));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{resolve_text, template};

    const CONVENTION: &str = "feat: new user-visible functionality";
    const STATUS: &str = "no staged changes";

    #[test]
    fn omits_the_optional_sections_when_nothing_is_given() {
        let prompt = template(None, None, CONVENTION, STATUS);
        assert!(!prompt.contains(":: USER HINT ::"));
        assert!(!prompt.contains(":: PROJECT CONTEXT ::"));
    }

    #[test]
    fn injects_the_hint_in_its_own_block() {
        let prompt = template(Some("es un refactor"), None, CONVENTION, STATUS);
        assert!(prompt.contains(":: USER HINT ::\nes un refactor\n:: END USER HINT ::"));
        assert!(!prompt.contains(":: PROJECT CONTEXT ::"));
    }

    #[test]
    fn injects_the_context_in_its_own_block() {
        let prompt = template(None, Some("proyecto rust"), CONVENTION, STATUS);
        assert!(prompt.contains(":: PROJECT CONTEXT ::\nproyecto rust\n:: END PROJECT CONTEXT ::"));
        assert!(!prompt.contains(":: USER HINT ::"));
    }

    #[test]
    fn keeps_both_blocks_when_both_are_given() {
        let prompt = template(
            Some("es un refactor"),
            Some("proyecto rust"),
            CONVENTION,
            STATUS,
        );
        assert!(prompt.contains(":: USER HINT ::\nes un refactor\n:: END USER HINT ::"));
        assert!(prompt.contains(":: PROJECT CONTEXT ::\nproyecto rust\n:: END PROJECT CONTEXT ::"));
    }

    #[test]
    fn ignores_a_value_with_no_content() {
        assert!(resolve_text(None).unwrap().is_none());
        assert!(resolve_text(Some("")).unwrap().is_none());
        assert!(resolve_text(Some("  \n\t ")).unwrap().is_none());
    }

    #[test]
    fn keeps_a_value_with_content() {
        let resolved = resolve_text(Some("es un refactor")).unwrap();
        assert_eq!(resolved.as_deref(), Some("es un refactor"));
    }

    #[test]
    fn ignores_a_file_with_no_content() {
        let path = std::env::temp_dir().join(format!(
            "gareth-commit-test-{}-empty.txt",
            std::process::id()
        ));
        std::fs::write(&path, "").unwrap();

        let resolved = resolve_text(Some(path.to_str().unwrap())).unwrap();
        assert!(resolved.is_none());
        std::fs::remove_file(&path).unwrap();
    }
}
