use crate::core;
use crate::util::files;
use crate::util::section;
use crate::util::terminal;

use super::utils::{comment, normalize_commit_message, prompt as prompt_for};
use crate::agents::repo::git::{check_files, staged_changes};

/// The text shown in place of the changes when there is nothing to describe.
const NO_CHANGES: &str = "No staged changes";

fn template(
    hint: Option<&str>,
    context: Option<&str>,
    convention_text: &str,
    status_text: &str,
) -> String {
    let mut sections: Vec<(&str, String)> = vec![
        (
            "INSTRUCTION",
            instruction(hint.is_some(), context.is_some()),
        ),
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

/// Builds the instructions, adding the ones of the sections that are given.
fn instruction(has_hint: bool, has_context: bool) -> String {
    let mut parts = vec![prompt_for("instruction").trim_end().to_string()];

    if has_hint {
        parts.push(prompt_for("hint").trim().to_string());
    }

    if has_context {
        parts.push(prompt_for("context").trim().to_string());
    }

    parts.join("\n\n")
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

/// Tells the model that the changes are limited to some files.
fn note(files: &[String]) -> String {
    if files.is_empty() {
        return String::new();
    }
    format!(
        "Only the changes under the selected files are shown: {}",
        files.join(", ")
    )
}

/// Builds the repository status, noting the files it was limited to.
///
/// `Ok(None)` means there is nothing to describe.
fn status_text(files: &[String]) -> Result<Option<String>, String> {
    let Some(changes) = staged_changes(files)? else {
        return Ok(None);
    };

    let note = note(files);
    if note.is_empty() {
        return Ok(Some(changes));
    }
    Ok(Some(format!("{note}\n\n{changes}")))
}

/// Chooses the text of the repository status.
///
/// The dump is for inspecting the prompt, so it is shown even when there is
/// nothing to describe.
fn status_for(status: Option<String>, dump: bool) -> Result<String, Box<dyn std::error::Error>> {
    match status {
        Some(status) => Ok(status),
        None if dump => Ok(NO_CHANGES.to_string()),
        None => Err("no changes staged for commit".into()),
    }
}

/// Builds a commit prompt, calls the model, and prints the final message.
pub async fn dispatch(
    service: &core::Service,
    args: &core::Cli,
    hint: Option<&str>,
    context: Option<&str>,
    convention_path: Option<&str>,
    files: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let convention_text = if let Some(path) = convention_path {
        std::fs::read_to_string(path)?
    } else {
        prompt_for("convention")
    };

    check_files(files)?;

    let hint = resolve_text(hint)?;
    let context = resolve_text(context)?;

    // Nothing to describe: gareth says so, without asking the model.
    let status = status_for(status_text(files)?, args.dump)?;

    let prompt = template(
        hint.as_deref(),
        context.as_deref(),
        &convention_text,
        &status,
    );

    if args.dump {
        terminal::print_prompt(&prompt);
        return Ok(());
    }

    if args.verbose {
        println!("{}\n\n", prompt);
    }

    let result = service.complete(&prompt).await?;
    let result = normalize_commit_message(&result);

    // The model does not report the errors, but if it does the commit stops.
    if result.contains("Error: no changes staged for commit") {
        return Err("no changes staged for commit".into());
    }

    println!("{}\n\n\n{}", result.trim_end(), comment(&convention_text));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{NO_CHANGES, note, resolve_text, status_for, template};

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

    #[test]
    fn notes_the_files_the_status_was_limited_to() {
        let note = note(&["src/a.rs".to_string(), "src/b.rs".to_string()]);
        assert_eq!(
            note,
            "Only the changes under the selected files are shown: src/a.rs, src/b.rs"
        );
    }

    #[test]
    fn does_not_note_any_file_when_the_whole_index_is_described() {
        assert!(note(&[]).is_empty());
    }

    #[test]
    fn adds_the_hint_instructions_only_when_the_hint_is_given() {
        let prompt = template(Some("es un refactor"), None, CONVENTION, STATUS);
        assert!(prompt.contains("The USER HINT complements the other instructions"));
        assert!(prompt.contains(":: USER HINT ::\nes un refactor\n:: END USER HINT ::"));
    }

    #[test]
    fn adds_the_context_instructions_only_when_the_context_is_given() {
        let prompt = template(None, Some("proyecto rust"), CONVENTION, STATUS);
        assert!(prompt.contains("The PROJECT CONTEXT complements the other instructions"));
        assert!(prompt.contains(":: PROJECT CONTEXT ::\nproyecto rust\n:: END PROJECT CONTEXT ::"));
    }

    #[test]
    fn does_not_add_the_optional_instructions_when_nothing_is_given() {
        let prompt = template(None, None, CONVENTION, STATUS);
        assert!(!prompt.contains("The USER HINT complements the other instructions"));
        assert!(!prompt.contains("The PROJECT CONTEXT complements the other instructions"));
    }

    #[test]
    fn keeps_the_status_when_there_is_something_to_describe() {
        let status = status_for(Some("el diff".to_string()), false).unwrap();
        assert_eq!(status, "el diff");
    }

    #[test]
    fn stops_when_there_is_nothing_to_describe() {
        let err = status_for(None, false).unwrap_err();
        assert!(
            err.to_string().contains("no changes staged"),
            "err was {err}"
        );
    }

    #[test]
    fn shows_the_empty_status_when_the_prompt_is_dumped() {
        let status = status_for(None, true).unwrap();
        assert_eq!(status, NO_CHANGES);
    }
}
