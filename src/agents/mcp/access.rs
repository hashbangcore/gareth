//! Runs a question against the tools of an MCP server.

use std::path::{Path, PathBuf};

use crate::core;
use crate::util::{terminal, text};

use super::client::Client;
use super::config::{Config, ConfigError, Transport};
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
    let mut offered = client.list_tools().await?;

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
    let mut tools: Vec<core::Tool> = offered.iter().map(to_tool).collect();
    let mut messages = vec![
        core::Message::system(&instructions),
        core::Message::user(&question),
    ];
    let mut empty_discovery_retries = 0;

    if args.dump {
        terminal::print_prompt(&core::describe(&messages, &tools));
        return Ok(());
    }

    for _ in 0..MAX_TURNS {
        let turn = service.complete_with_tools(&messages, &tools).await?;

        match turn {
            core::Turn::Text(text) => {
                if client.take_tools_changed() {
                    let refreshed = client.list_tools().await?;
                    if refreshed != offered {
                        offered = refreshed;
                        tools = offered.iter().map(to_tool).collect();
                        continue;
                    }
                }
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
                let mut empty_discovery = false;
                let mut catalog_follow_up = None;
                let mut results = Vec::with_capacity(calls.len());

                for call in calls {
                    let result = match client
                        .call_tool(&call.function.name, call.arguments())
                        .await
                    {
                        Ok(result) => result,
                        // The model gets the failure and can try another tool.
                        Err(err) => format!("The tool call failed: {err}"),
                    };

                    if args.verbose {
                        eprintln!(":: TOOL RESULT ::\n{result}\n:: END TOOL RESULT ::");
                    }

                    if offered
                        .iter()
                        .find(|tool| tool.name == call.function.name)
                        .is_some_and(is_tool_discovery)
                        && has_no_discovery_matches(&result)
                    {
                        empty_discovery = true;
                    }

                    if empty_discovery_retries > 0
                        && offered
                            .iter()
                            .find(|tool| tool.name == call.function.name)
                            .is_some_and(is_tool_catalog)
                        && has_tool_catalog_entries(&result)
                    {
                        let invokers = offered
                            .iter()
                            .filter(|tool| is_tool_invoker(tool))
                            .map(|tool| tool.name.as_str())
                            .collect::<Vec<_>>();
                        if !invokers.is_empty() {
                            let names = catalog_tool_names(&result);
                            catalog_follow_up = Some(format!(
                                "The catalog returned these available operations: {names}. Continue the original task. Use the server-specific catalog listing to get a matching operation's full argument schema if needed, then call the available tool execution interface {} with the exact server, operation name, and arguments. Do not stop at the catalog listing or answer that no tools exist without attempting the operation.",
                                invokers.join(", ")
                            ));
                        }
                    }

                    results.push((call, result));
                }

                append_tool_results(&mut messages, &results, catalog_follow_up.as_deref());

                if empty_discovery && empty_discovery_retries < 2 {
                    empty_discovery_retries += 1;
                    messages.push(core::Message::user(
                        "The tool discovery call returned no matching tools. Continue the task instead of answering yet: retry discovery using broad words for the task and its domain, removing any literal ID or specific target value from the search. If discovery still finds nothing, use any tool that lists the server's available tools, then invoke the appropriate operation with the original target as an argument.",
                    ));
                }

                if client.take_tools_changed() {
                    let refreshed = client.list_tools().await?;
                    if refreshed != offered {
                        offered = refreshed;
                        tools = offered.iter().map(to_tool).collect();
                    }
                }
            }
        }
    }

    Err(format!("the model kept asking for tools after {MAX_TURNS} turns").into())
}

/// Appends every response to a batch of tool calls before any user follow-up.
fn append_tool_results(
    messages: &mut Vec<core::Message>,
    results: &[(core::ToolCall, String)],
    follow_up: Option<&str>,
) {
    messages.extend(
        results
            .iter()
            .map(|(call, result)| core::Message::tool(call, result)),
    );

    if let Some(follow_up) = follow_up {
        messages.push(core::Message::user(follow_up));
    }
}

/// Checks whether a tool is a search interface for discovering other tools.
fn is_tool_discovery(tool: &Tool) -> bool {
    let description = tool.description.to_lowercase();
    let name = tool.name.to_lowercase();
    let searches = description.contains("search")
        || description.contains("find")
        || name.contains("search")
        || name.contains("find");
    let mentions_tools = description.contains("tool") || name.contains("tool");
    let has_query_argument = tool
        .input_schema
        .as_ref()
        .and_then(|schema| schema.get("properties"))
        .and_then(|properties| properties.get("query"))
        .is_some();

    searches && mentions_tools && has_query_argument
}

/// Checks whether a tool lists other tools in a server or catalog.
fn is_tool_catalog(tool: &Tool) -> bool {
    let description = tool.description.to_lowercase();
    let name = tool.name.to_lowercase();
    (description.contains("list") || name.contains("list"))
        && (description.contains("tool") || name.contains("tool"))
}

/// Checks whether a tool invokes or calls another tool by name.
fn is_tool_invoker(tool: &Tool) -> bool {
    let description = tool.description.to_lowercase();
    let name = tool.name.to_lowercase();
    (description.contains("invoke")
        || description.contains("call any tool")
        || name.contains("invoke"))
        && (description.contains("tool") || name.contains("tool"))
}

/// Checks whether a catalog result contains at least one operation.
fn has_tool_catalog_entries(result: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(result)
        .ok()
        .and_then(|value| {
            value
                .get("tools")
                .and_then(serde_json::Value::as_array)
                .cloned()
        })
        .is_some_and(|tools| !tools.is_empty())
}

/// Summarizes operation names and server names from a catalog result.
fn catalog_tool_names(result: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(result) else {
        return "the listed tools".to_string();
    };
    let Some(tools) = value.get("tools").and_then(serde_json::Value::as_array) else {
        return "the listed tools".to_string();
    };

    tools
        .iter()
        .filter_map(|tool| {
            let name = tool.get("name")?.as_str()?;
            let server = tool.get("server").and_then(serde_json::Value::as_str);
            Some(match server {
                Some(server) => format!("{server}/{name}"),
                None => name.to_string(),
            })
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Checks common JSON results used by tool-discovery APIs for an empty search.
fn has_no_discovery_matches(result: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(result) else {
        return false;
    };

    value
        .get("matches")
        .and_then(serde_json::Value::as_array)
        .is_some_and(Vec::is_empty)
        || value
            .get("total_available")
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|total| total == 0)
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

    text.push_str(
        "\n\nMCP tool-use requirements: Follow the server's instructions. If it exposes tools whose purpose is to search, find, or list other tools, treat that as a discovery interface: discover the relevant operation, then invoke it. A search for tools must describe the task and data domain, not just copy a literal identifier, package name, or other value from the user's request; pass that value as an argument to the discovered operation. If discovery returns no matches, retry with broader task words and use any available tool-catalog listing before concluding that no tool can help. Use the exact tool names and argument schemas returned by discovery. When a tool returns a plan or instructions to call further tools, carry out those calls and report the resulting information; do not present the plan itself as the completed answer.",
    );

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
        match &server.transport {
            Transport::Stdio { command, .. } => println!("{name}: {command}"),
            Transport::Http { url, .. } => println!("{name}: {url}"),
        }
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
    let text = text::unescape_newlines(text);
    if args.verbose {
        terminal::print_labeled("LLM", text.trim());
    } else {
        println!("{}", terminal::render_markdown(&text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_call(id: &str, name: &str) -> core::ToolCall {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "type": "function",
            "function": { "name": name, "arguments": "{}" }
        }))
        .unwrap()
    }

    #[test]
    fn appends_all_tool_results_before_a_follow_up() {
        let calls = vec![tool_call("call-1", "search"), tool_call("call-2", "list")];
        let mut messages = vec![core::Message::assistant(calls.clone())];
        let results = vec![
            (calls[0].clone(), "first result".to_string()),
            (calls[1].clone(), "second result".to_string()),
        ];

        append_tool_results(&mut messages, &results, Some("continue"));

        assert_eq!(
            messages
                .iter()
                .map(|message| message.role.label())
                .collect::<Vec<_>>(),
            ["ASSISTANT", "TOOL", "TOOL", "USER"]
        );
        assert_eq!(messages[1].tool_call_id.as_deref(), Some("call-1"));
        assert_eq!(messages[2].tool_call_id.as_deref(), Some("call-2"));
    }

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
