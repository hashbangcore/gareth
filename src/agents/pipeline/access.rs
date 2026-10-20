use crate::core;
use crate::util::{attach, terminal};

pub async fn dispatch(
    service: &core::Service,
    args: &core::Cli,
    request: &str,
    stdin: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let prompt = build_prompt(request, &stdin);

    if args.dump {
        terminal::print_prompt(&prompt);
        return Ok(());
    }

    let response = service.complete(&prompt).await?;

    if args.verbose {
        terminal::print_labeled("USER", &prompt);
        terminal::print_labeled("LLM", response.trim());
    } else {
        println!("{}", terminal::render_markdown(&response));
    }

    Ok(())
}

/// Builds the prompt that is sent to the model.
fn build_prompt(request: &str, stdin: &str) -> String {
    let attachments = attach::extract_attachments_from_input(request);
    let attached_files = attach::format_attached_files(
        if stdin.trim().is_empty() {
            None
        } else {
            Some(stdin)
        },
        &attachments,
    );

    let mut prompt = format!(":: USER HINT ::\n{}\n:: END USER HINT ::", request.trim());
    if let Some(block) = attached_files {
        prompt.push_str("\n\n");
        prompt.push_str(&block);
    }

    prompt
}

#[cfg(test)]
mod tests {
    use super::build_prompt;

    #[test]
    fn wraps_the_request_in_the_user_hint_block() {
        let prompt = build_prompt("una pregunta", "");
        assert!(prompt.contains(":: USER HINT ::\nuna pregunta\n:: END USER HINT ::"));
        assert!(!prompt.contains("ATTACHED FILES"));
    }

    #[test]
    fn attaches_the_stdin() {
        let prompt = build_prompt("una pregunta", "contenido\n");
        assert!(prompt.contains(":: ATTACHED FILES ::"));
        assert!(prompt.contains("contenido"));
    }
}
