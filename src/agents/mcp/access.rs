//! Runs a question against the tools of an MCP server.

use std::path::{Path, PathBuf};

use crate::core;
use crate::util::terminal;

use super::client::Client;
use super::config::{Config, ConfigError};
use super::protocol::{Initialize, Tool};

/// The number of times the model may call tools before gareth gives up.
const MAX_TURNS: usize = 12;

/// What the command line says about the server to use.
pub struct Options<'a> {
    /// The name of the server in the configuration.
    pub server: Option<&'a str>,
    /// The question for the model.
    pub prompt: &'a [String],
    /// The path of the configuration file.
    pub config: Option<&'a str>,
    /// Whether to list the servers and stop.
    pub list: bool,
    /// Whether to list the tools of the server and stop.
    pub tools: bool,
}

/// Asks the model a question and lets it use the tools of a server.
///
/// # Errors
///
/// Returns an error when the configuration cannot be read, when the server
/// cannot be started, when the question is missing or when the model fails.
pub async fn dispatch(
    service: &core::Service,
    args: &core::Cli,
    options: &Options<'_>,
    stdin: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = match options.config {
        Some(path) => PathBuf::from(path),
        None => Config::default_path(),
    };

    let config = match Config::read(&path) {
        Ok(config) => config,
        Err(ConfigError::NotFound { .. }) if options.list => Config::default(),
        Err(err) => return Err(err.into()),
    };

    if options.list {
        list_servers(&config, &path);
        return Ok(());
    }

    let Some(name) = options.server else {
        return Err("no server given, name one of the servers of the configuration".into());
    };

    let server = config.server(name)?;

    let mut client = Client::start(server).await?;
    let answer = client.initialize().await?;
    let offered = client.list_tools().await?;

    if options.tools {
        print_tools(&offered, &answer);
        return Ok(());
    }

    let question = question(options.prompt, &stdin);
    if question.trim().is_empty() {
        return Err("no question given".into());
    }

    let instructions = system_message(
        answer.instructions.as_deref().unwrap_or_default(),
        &answer.server_info.name,
    );
    let tools: Vec<core::Tool> = offered.iter().map(to_tool).collect();
    let mut messages = vec![
        core::Message::system(&instructions),
        core::Message::user(&question),
    ];

    if args.dump {
        terminal::print_prompt(&core::describe(&messages, &tools));
        return Ok(());
    }

    for _ in 0..MAX_TURNS {
        let turn = service.complete_with_tools(&messages, &tools).await?;

        match turn {
            core::Turn::Text(text) => {
                print_answer(args, &text);
                return Ok(());
            }
            core::Turn::ToolCalls(calls) => {
                if args.verbose {
                    for call in &calls {
                        eprintln!(
                            ":: TOOL CALL ::\n{}({})\n:: END TOOL CALL ::",
                            call.function.name, call.function.arguments
                        );
                    }
                }

                messages.push(core::Message::assistant(calls.clone()));

                for call in calls {
                    let result = match client.call_tool(&call.function.name, call.arguments()).await {
                        Ok(result) => result,
                        // The model gets the failure and can try another tool.
                        Err(err) => format!("The tool call failed: {err}"),
                    };

                    if args.verbose {
                        eprintln!(":: TOOL RESULT ::\n{result}\n:: END TOOL RESULT ::");
                    }

                    messages.push(core::Message::tool(&call, &result));
                }
            }
        }
    }

    Err(format!("the model kept asking for tools after {MAX_TURNS} turns").into())
}

/// Builds the question of the user from the command line and the input.
fn question(prompt: &[String], stdin: &str) -> String {
    if !prompt.is_empty() {
        return prompt.join(" ");
    }
    stdin.trim().to_string()
}

/// Builds the instructions the model gets from the server.
fn system_message(instructions: &str, server: &str) -> String {
    let mut text = format!(
        "You can call the tools of the {server} server to answer the question. \
         Use them when the question needs information you do not have, and answer \
         with what you know when you do not need them."
    );

    let instructions = instructions.trim();
    if !instructions.is_empty() {
        text.push_str("\n\n");
        text.push_str(instructions);
    }

    text
}

/// Turns a tool of the server into a tool for the model.
fn to_tool(tool: &Tool) -> core::Tool {
    core::Tool {
        name: tool.name.clone(),
        description: tool.description.clone(),
        parameters: tool.parameters(),
    }
}

/// Prints the servers of the configuration.
fn list_servers(config: &Config, path: &Path) {
    if config.servers.is_empty() {
        println!("no servers in {}", path.display());
        return;
    }

    for (name, server) in &config.servers {
        println!("{name}: {}", server.command);
    }
}

/// Prints the tools a server offers.
fn print_tools(tools: &[Tool], server: &Initialize) {
    println!(
        "{} {} (protocol {})",
        server.server_info.name, server.server_info.version, server.protocol_version
    );

    if tools.is_empty() {
        println!("the server has no tools");
        return;
    }

    for tool in tools {
        println!("{}", tool.name);
        for line in tool.description.lines() {
            println!("    {line}");
        }
        println!();
    }
}

/// Prints the answer of the model.
fn print_answer(args: &core::Cli, text: &str) {
    if args.verbose {
        terminal::print_labeled("LLM", text.trim());
    } else {
        println!("{}", terminal::render_markdown(text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_the_question_from_the_command_line() {
        let prompt = vec!["how".to_string(), "is it".to_string()];
        assert_eq!(question(&prompt, "ignored"), "how is it");
    }

    #[test]
    fn takes_the_question_from_the_input() {
        assert_eq!(question(&[], "  a question\n"), "a question");
    }

    #[test]
    fn keeps_the_instructions_of_the_server() {
        let text = system_message("use the tools", "nixos");
        assert!(text.contains("nixos server"));
        assert!(text.contains("use the tools"));
    }

    #[test]
    fn leaves_out_the_instructions_when_there_are_none() {
        let text = system_message("   ", "nixos");
        assert!(text.contains("nixos server"));
        assert!(!text.contains("   \n\n"));
    }
}
