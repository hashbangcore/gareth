//! A client of the Model Context Protocol, talking over stdio.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::io;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use http::{HeaderName, HeaderValue};
use rmcp::{
    ClientLifecycleMode, ClientServiceExt, RoleClient,
    model::{
        CallToolRequestParams, ClientCapabilities, ClientConfig, Implementation, ProtocolVersion,
    },
    service::RunningService,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};

use super::config::{Server, Transport};
use super::protocol::{Failure, Initialize, Message, Notification, Request, Tool, Tools};

/// The version of the protocol gareth speaks.
const PROTOCOL_VERSION: &str = "2025-06-18";

/// The name gareth gives to itself.
const CLIENT_NAME: &str = "gareth";

/// The version gareth gives to itself.
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The code the client answers with when the server asks for something it
/// does not offer.
const METHOD_NOT_FOUND: i64 = -32601;

/// Why a call to the server could not be made.
#[derive(Debug)]
pub enum McpError {
    /// The HTTP transport could not be configured or completed a request.
    Transport { message: String },
    /// The server could not be started.
    Start {
        /// The command that was run.
        command: String,
        /// What went wrong.
        source: io::Error,
    },
    /// The server did not give the pipes gareth needs.
    Pipe,
    /// The server could not be written to.
    Write {
        /// What went wrong.
        source: io::Error,
    },
    /// The server could not be read.
    Read {
        /// What went wrong.
        source: io::Error,
    },
    /// The server wrote something that is not a message.
    Parse {
        /// What went wrong.
        source: serde_json::Error,
    },
    /// The server stopped before answering.
    Closed {
        /// What the server wrote to its standard error.
        stderr: String,
    },
    /// The server answered with an error.
    Failed {
        /// What failed, the name of the tool when there is one.
        subject: String,
        /// What the server reported.
        message: String,
    },
}

impl fmt::Display for McpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport { message } => write!(f, "MCP transport failed: {message}"),
            Self::Start { command, source } => {
                write!(f, "could not run {command}: {source}")
            }
            Self::Pipe => f.write_str("the server did not give the pipes gareth needs"),
            Self::Write { source } => write!(f, "could not write to the server: {source}"),
            Self::Read { source } => write!(f, "could not read from the server: {source}"),
            Self::Parse { source } => write!(f, "the server wrote an unreadable message: {source}"),
            Self::Closed { stderr } => {
                if stderr.trim().is_empty() {
                    f.write_str("the server stopped before answering")
                } else {
                    write!(f, "the server stopped before answering: {}", stderr.trim())
                }
            }
            Self::Failed { subject, message } => write!(f, "the {subject} failed: {message}"),
        }
    }
}

impl std::error::Error for McpError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transport { .. } => None,
            Self::Start { source, .. } => Some(source),
            Self::Write { source } | Self::Read { source } => Some(source),
            Self::Parse { source } => Some(source),
            _ => None,
        }
    }
}

/// A running server, ready to answer.
pub struct StdioClient {
    /// The process of the server.
    child: Child,
    /// Where the requests are written.
    stdin: ChildStdin,
    /// Where the answers are read from.
    reader: BufReader<ChildStdout>,
    /// What the server wrote to its standard error.
    stderr: Arc<Mutex<String>>,
    /// The identifier of the next request.
    next_id: u64,
    /// Whether the server announced that its tool list changed.
    tools_changed: bool,
}

impl StdioClient {
    /// Starts the server of the configuration and waits until it can be talked to.
    ///
    /// # Errors
    ///
    /// Returns an error when the server cannot be started.
    async fn start(server: &Server) -> Result<Self, McpError> {
        let Transport::Stdio {
            command: program,
            args,
            env,
        } = &server.transport
        else {
            return Err(McpError::Transport {
                message: "stdio client given a non-stdio server".into(),
            });
        };
        let mut command = Command::new(program);
        command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for (key, value) in env {
            command.env(key, value);
        }

        let mut child = command.spawn().map_err(|source| McpError::Start {
            command: program.clone(),
            source,
        })?;

        let stdin = child.stdin.take().ok_or(McpError::Pipe)?;
        let stdout = child.stdout.take().ok_or(McpError::Pipe)?;
        let stderr = child.stderr.take().ok_or(McpError::Pipe)?;

        let buffer = Arc::new(Mutex::new(String::new()));
        tokio::spawn(drain(stderr, buffer.clone()));

        Ok(Self {
            child,
            stdin,
            reader: BufReader::new(stdout),
            stderr: buffer,
            next_id: 0,
            tools_changed: false,
        })
    }

    /// Says hello to the server and asks it for its instructions.
    ///
    /// The answer carries the version of the protocol and the instructions of
    /// the server.
    ///
    /// # Errors
    ///
    /// Returns an error when the server does not answer.
    pub async fn initialize(&mut self) -> Result<Initialize, McpError> {
        let params = serde_json::json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": { "listChanged": true } },
            "clientInfo": { "name": CLIENT_NAME, "version": CLIENT_VERSION },
        });

        let result = self
            .request("initialize", Some(params), "initialize")
            .await?;
        let answer: Initialize =
            serde_json::from_value(result).map_err(|source| McpError::Parse { source })?;

        self.notify("notifications/initialized").await?;

        Ok(answer)
    }

    /// Asks the server for the [`Tool`]s it offers.
    ///
    /// # Errors
    ///
    /// Returns an error when the server does not answer.
    pub async fn list_tools(&mut self) -> Result<Vec<Tool>, McpError> {
        self.tools_changed = false;
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        let mut seen_cursors = HashSet::new();

        loop {
            let params = cursor
                .as_ref()
                .map(|cursor| serde_json::json!({ "cursor": cursor }));
            let result = self.request("tools/list", params, "tools/list").await?;
            let answer: Tools =
                serde_json::from_value(result).map_err(|source| McpError::Parse { source })?;
            tools.extend(answer.tools);

            match answer.next_cursor {
                Some(next) if !seen_cursors.insert(next.clone()) => {
                    return Err(McpError::Failed {
                        subject: "tools/list".to_string(),
                        message: format!("the server repeated pagination cursor {next:?}"),
                    });
                }
                Some(next) => cursor = Some(next),
                None => return Ok(tools),
            }
        }
    }

    /// Returns and clears the server's tool-list-changed notification flag.
    pub fn take_tools_changed(&mut self) -> bool {
        std::mem::take(&mut self.tools_changed)
    }

    /// Calls a [`Tool`] of the server and returns what it answered.
    ///
    /// # Errors
    ///
    /// Returns an error when the server does not answer.
    pub async fn call_tool(
        &mut self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<String, McpError> {
        let params = serde_json::json!({ "name": name, "arguments": arguments });

        let subject = format!("tool {name}");
        let result = self.request("tools/call", Some(params), &subject).await?;

        Ok(result_text(&result))
    }

    /// Sends a request and waits for the answer that carries its identifier.
    async fn request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
        subject: &str,
    ) -> Result<serde_json::Value, McpError> {
        self.next_id += 1;
        let id = self.next_id;

        let request = Request {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        };

        self.write(&serde_json::to_string(&request).map_err(|source| McpError::Parse { source })?)
            .await?;

        self.wait_for(id, subject).await
    }

    /// Sends a notification, which the server does not answer.
    async fn notify(&mut self, method: &str) -> Result<(), McpError> {
        let notification = Notification {
            jsonrpc: "2.0",
            method: method.to_string(),
        };

        self.write(
            &serde_json::to_string(&notification).map_err(|source| McpError::Parse { source })?,
        )
        .await
    }

    /// Writes a message to the server.
    async fn write(&mut self, line: &str) -> Result<(), McpError> {
        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|source| McpError::Write { source })?;
        self.stdin
            .write_all(b"\n")
            .await
            .map_err(|source| McpError::Write { source })?;
        self.stdin
            .flush()
            .await
            .map_err(|source| McpError::Write { source })?;
        Ok(())
    }

    /// Reads messages until the answer of the given request shows up.
    async fn wait_for(&mut self, id: u64, subject: &str) -> Result<serde_json::Value, McpError> {
        loop {
            let Some(line) = self.next_line().await? else {
                return Err(McpError::Closed {
                    stderr: self.stderr_text(),
                });
            };

            match read_reply(&line, id) {
                Some(Reply::Answer(result)) => return Ok(result),
                Some(Reply::Failed(failure)) => {
                    return Err(McpError::Failed {
                        subject: subject.to_string(),
                        message: format!("{} ({})", failure.message, failure.code),
                    });
                }
                Some(Reply::Refuse { id, method }) => self.refuse(&id, &method).await?,
                Some(Reply::Notification { method }) => {
                    if method == "notifications/tools/list_changed" {
                        self.tools_changed = true;
                    }
                }
                Some(Reply::Skip) | None => {}
            }
        }
    }

    /// Reads the next line the server writes.
    async fn next_line(&mut self) -> Result<Option<String>, McpError> {
        let mut line = String::new();
        let read = self
            .reader
            .read_line(&mut line)
            .await
            .map_err(|source| McpError::Read { source })?;

        if read == 0 {
            return Ok(None);
        }

        Ok(Some(line))
    }

    /// Answers a request of the server, which gareth does not serve.
    async fn refuse(&mut self, id: &serde_json::Value, method: &str) -> Result<(), McpError> {
        let answer = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": METHOD_NOT_FOUND, "message": format!("gareth does not serve {method}") },
        });

        self.write(&answer.to_string()).await
    }

    /// Returns what the server wrote to its standard error.
    fn stderr_text(&self) -> String {
        self.stderr
            .lock()
            .map(|text| text.clone())
            .unwrap_or_default()
    }
}

/// An active stdio or Streamable HTTP MCP client.
pub enum Client {
    Stdio(StdioClient),
    Http(HttpClient),
}

/// Running Streamable HTTP client service.
pub struct HttpClient {
    service: RunningService<RoleClient, ClientConfig>,
}

impl Client {
    /// Connects using the transport specified in the server configuration.
    pub async fn start(server: &Server) -> Result<Self, McpError> {
        match &server.transport {
            Transport::Stdio { .. } => Ok(Self::Stdio(StdioClient::start(server).await?)),
            Transport::Http { url, headers } => {
                let parsed_headers = parse_http_headers(headers)?;
                let transport = StreamableHttpClientTransport::from_config(
                    StreamableHttpClientTransportConfig::with_uri(url.clone())
                        .custom_headers(parsed_headers),
                );
                let client_info = ClientConfig::new(
                    ClientCapabilities::default(),
                    Implementation::new(CLIENT_NAME, CLIENT_VERSION),
                );
                let service = client_info
                    .serve_with_lifecycle(
                        transport,
                        ClientLifecycleMode::Auto {
                            preferred_versions: vec![ProtocolVersion::LATEST],
                            legacy_version: Some(ProtocolVersion::V_2025_11_25),
                        },
                    )
                    .await
                    .map_err(|err| McpError::Transport {
                        message: err.to_string(),
                    })?;
                Ok(Self::Http(HttpClient { service }))
            }
        }
    }

    /// Initializes or retrieves the HTTP peer metadata.
    pub async fn initialize(&mut self) -> Result<Initialize, McpError> {
        match self {
            Self::Stdio(client) => client.initialize().await,
            Self::Http(client) => {
                let info = client
                    .service
                    .peer_info()
                    .ok_or_else(|| McpError::Transport {
                        message: "server did not provide peer information".into(),
                    })?;
                let server = info.server_info.as_ref();
                Ok(Initialize {
                    protocol_version: info.protocol_version.to_string(),
                    server_info: super::protocol::ServerInfo {
                        name: server
                            .map(|s| s.name.clone())
                            .unwrap_or_else(|| "MCP server".into()),
                        version: server.map(|s| s.version.clone()).unwrap_or_default(),
                    },
                    instructions: info.instructions.clone(),
                })
            }
        }
    }

    /// Lists all tools from the connected server.
    pub async fn list_tools(&mut self) -> Result<Vec<Tool>, McpError> {
        match self {
            Self::Stdio(client) => client.list_tools().await,
            Self::Http(client) => {
                let listed =
                    client
                        .service
                        .list_all_tools()
                        .await
                        .map_err(|err| McpError::Transport {
                            message: err.to_string(),
                        })?;
                listed
                    .into_iter()
                    .map(|tool| {
                        let schema = serde_json::to_value(&tool.input_schema)
                            .map_err(|source| McpError::Parse { source })?;
                        Ok(Tool {
                            name: tool.name.to_string(),
                            description: tool
                                .description
                                .map(|value| value.to_string())
                                .unwrap_or_default(),
                            input_schema: Some(schema),
                        })
                    })
                    .collect()
            }
        }
    }

    /// Checks whether the server announced a tool-list change.
    pub fn take_tools_changed(&mut self) -> bool {
        match self {
            Self::Stdio(client) => client.take_tools_changed(),
            Self::Http(_) => false,
        }
    }

    /// Calls a tool and returns its text content.
    pub async fn call_tool(
        &mut self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<String, McpError> {
        match self {
            Self::Stdio(client) => client.call_tool(name, arguments).await,
            Self::Http(client) => {
                let arguments = arguments
                    .as_object()
                    .cloned()
                    .ok_or_else(|| McpError::Failed {
                        subject: format!("tool {name}"),
                        message: "tool arguments must be a JSON object".into(),
                    })?;
                let result = client
                    .service
                    .call_tool(
                        CallToolRequestParams::new(name.to_string()).with_arguments(arguments),
                    )
                    .await
                    .map_err(|err| McpError::Transport {
                        message: err.to_string(),
                    })?;
                let value =
                    serde_json::to_value(result).map_err(|source| McpError::Parse { source })?;
                Ok(result_text(&value))
            }
        }
    }
}

fn resolve_header_value(value: &str) -> Result<String, McpError> {
    let mut result = String::with_capacity(value.len());
    let mut remaining = value;
    while let Some(start) = remaining.find("${") {
        result.push_str(&remaining[..start]);
        let expression = &remaining[start + 2..];
        let Some(end) = expression.find('}') else {
            return Err(McpError::Transport {
                message: "unterminated ${...} environment reference in HTTP header".into(),
            });
        };
        let name = &expression[..end];
        if name.is_empty() {
            return Err(McpError::Transport {
                message: "empty environment variable name in HTTP header".into(),
            });
        }
        let secret = std::env::var(name).map_err(|_| McpError::Transport {
            message: format!("environment variable {name} referenced by an HTTP header is not set"),
        })?;
        result.push_str(&secret);
        remaining = &expression[end + 1..];
    }
    result.push_str(remaining);
    Ok(result)
}

fn parse_http_headers(
    configured: &std::collections::BTreeMap<String, String>,
) -> Result<HashMap<HeaderName, HeaderValue>, McpError> {
    let mut parsed = HashMap::with_capacity(configured.len());
    for (name, configured_value) in configured {
        let value = resolve_header_value(configured_value)?;
        let name = HeaderName::try_from(name.as_str()).map_err(|err| McpError::Transport {
            message: format!("invalid HTTP header name: {err}"),
        })?;
        let value = HeaderValue::try_from(value.as_str()).map_err(|err| McpError::Transport {
            message: format!("invalid value for HTTP header {}: {err}", name),
        })?;
        parsed.insert(name, value);
    }
    Ok(parsed)
}

#[cfg(test)]
mod http_config_tests {
    use super::*;

    #[test]
    fn leaves_literal_http_header_values_unchanged() {
        assert_eq!(
            resolve_header_value("application/json").unwrap(),
            "application/json"
        );
    }

    #[test]
    fn expands_http_header_environment_references_with_literal_prefixes() {
        let path = std::env::var("PATH").expect("test environment provides PATH");
        assert_eq!(
            resolve_header_value("Bearer ${PATH}").unwrap(),
            format!("Bearer {path}")
        );
    }

    #[test]
    fn validates_http_header_names_and_values_before_connecting() {
        let invalid_name = [("not a header".to_string(), "value".to_string())]
            .into_iter()
            .collect();
        assert!(parse_http_headers(&invalid_name).is_err());

        let invalid_value = [("x-token".to_string(), "line\nbreak".to_string())]
            .into_iter()
            .collect();
        assert!(parse_http_headers(&invalid_value).is_err());
    }

    #[test]
    fn reports_missing_http_header_environment_variables_without_leaking_values() {
        let result = resolve_header_value("Bearer ${GARETH_TEST_MISSING_MCP_TOKEN_83F1}");
        let error = result.unwrap_err().to_string();
        assert!(error.contains("GARETH_TEST_MISSING_MCP_TOKEN_83F1"));
        assert!(!error.contains("Bearer"));
    }

    #[test]
    fn rejects_malformed_http_header_environment_references() {
        assert!(resolve_header_value("Bearer ${TOKEN").is_err());
        assert!(resolve_header_value("Bearer ${}").is_err());
    }
}

impl Drop for StdioClient {
    fn drop(&mut self) {
        // The server stops when gareth is done with it.
        let _ = self.child.start_kill();
    }
}

/// Reads the standard error of the server, so it never blocks.
async fn drain(pipe: ChildStderr, buffer: Arc<Mutex<String>>) {
    let mut lines = BufReader::new(pipe).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        let mut text = buffer.lock().unwrap_or_else(|err| err.into_inner());
        text.push_str(&line);
        text.push('\n');
    }
}

/// What a message from the server means.
#[derive(Debug)]
enum Reply {
    /// The answer of the request that is being waited for.
    Answer(serde_json::Value),
    /// The answer of a request that failed.
    Failed(Failure),
    /// A message that is not the answer.
    Skip,
    /// A request of the server that gareth does not serve.
    Refuse {
        /// The identifier the answer must carry.
        id: serde_json::Value,
        /// The method the server asks for.
        method: String,
    },
    /// A notification sent by the server.
    Notification {
        /// The notification method.
        method: String,
    },
}

/// Reads what a line written by the server means.
///
/// A request of the server has an identifier and a method, and a notification
/// has only a method. Lines that are not part of the protocol are skipped, and
/// the answers of other requests are left alone.
fn read_reply(line: &str, id: u64) -> Option<Reply> {
    let message: Message = serde_json::from_str(line).ok()?;

    if let (Some(answer), Some(method)) = (&message.id, &message.method) {
        return Some(Reply::Refuse {
            id: answer.clone(),
            method: method.clone(),
        });
    }

    if let Some(method) = message.method {
        return Some(Reply::Notification { method });
    }

    match message.id {
        None => Some(Reply::Skip),
        Some(answer) if answer != serde_json::json!(id) => Some(Reply::Skip),
        Some(_) => match message.error {
            Some(failure) => Some(Reply::Failed(failure)),
            None => Some(Reply::Answer(
                message.result.unwrap_or(serde_json::Value::Null),
            )),
        },
    }
}

/// Turns the answer of a tool into the text the model reads.
pub fn result_text(result: &serde_json::Value) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(content) = result.get("content").and_then(serde_json::Value::as_array) {
        for item in content {
            match item.get("text").and_then(serde_json::Value::as_str) {
                Some(text) => parts.push(text.to_string()),
                None => parts.push(item.to_string()),
            }
        }
    }

    let text = if parts.is_empty() {
        result.to_string()
    } else {
        parts.join("\n")
    };

    if result
        .get("isError")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return format!("The tool failed: {text}");
    }

    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_text_of_the_content() {
        let result = serde_json::json!({
            "content": [{ "type": "text", "text": "hello" }]
        });
        assert_eq!(result_text(&result), "hello");
    }

    #[test]
    fn joins_every_text_of_the_content() {
        let result = serde_json::json!({
            "content": [
                { "type": "text", "text": "hello" },
                { "type": "text", "text": "world" }
            ]
        });
        assert_eq!(result_text(&result), "hello\nworld");
    }

    #[test]
    fn keeps_a_part_that_is_not_text() {
        let result = serde_json::json!({
            "content": [{ "type": "image", "data": "abc" }]
        });
        assert!(result_text(&result).contains("image"));
    }

    #[test]
    fn falls_back_to_the_whole_answer() {
        let result = serde_json::json!({ "structuredContent": { "a": 1 } });
        assert!(result_text(&result).contains("structuredContent"));
    }

    #[test]
    fn marks_a_failed_tool() {
        let result = serde_json::json!({
            "content": [{ "type": "text", "text": "it broke" }],
            "isError": true
        });
        assert_eq!(result_text(&result), "The tool failed: it broke");
    }

    #[test]
    fn ignores_a_line_that_is_not_part_of_the_protocol() {
        assert!(read_reply("the server is starting", 1).is_none());
        assert!(read_reply("", 1).is_none());
        assert!(read_reply("{not json", 1).is_none());
    }

    #[test]
    fn reads_a_notification() {
        let reply = read_reply(r#"{"jsonrpc":"2.0","method":"notifications/progress"}"#, 1);
        assert!(matches!(
            reply,
            Some(Reply::Notification { method }) if method == "notifications/progress"
        ));
    }

    #[test]
    fn reads_a_tools_list_changed_notification() {
        let reply = read_reply(
            r#"{"jsonrpc":"2.0","method":"notifications/tools/list_changed"}"#,
            1,
        );
        assert!(matches!(
            reply,
            Some(Reply::Notification { method }) if method == "notifications/tools/list_changed"
        ));
    }

    #[test]
    fn refuses_a_request_of_the_server() {
        let reply = read_reply(r#"{"jsonrpc":"2.0","id":9,"method":"roots/list"}"#, 1);
        match reply {
            Some(Reply::Refuse { id, method }) => {
                assert_eq!(method, "roots/list");
                assert_eq!(id, serde_json::json!(9));
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn skips_the_answer_of_another_request() {
        let reply = read_reply(r#"{"jsonrpc":"2.0","id":7,"result":{"ok":true}}"#, 1);
        assert!(matches!(reply, Some(Reply::Skip)));
    }

    #[test]
    fn reads_the_answer_of_the_request() {
        let reply = read_reply(r#"{"jsonrpc":"2.0","id":1,"result":{"ok":true}}"#, 1);
        match reply {
            Some(Reply::Answer(result)) => assert_eq!(result["ok"], serde_json::json!(true)),
            other => panic!("expected an answer, got {other:?}"),
        }
    }

    #[test]
    fn reads_an_answer_without_a_result() {
        let reply = read_reply(r#"{"jsonrpc":"2.0","id":1}"#, 1);
        match reply {
            Some(Reply::Answer(result)) => assert!(result.is_null()),
            other => panic!("expected an answer, got {other:?}"),
        }
    }

    #[test]
    fn reads_the_failure_of_the_request() {
        let reply = read_reply(
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"tool not found"}}"#,
            1,
        );
        match reply {
            Some(Reply::Failed(failure)) => {
                assert_eq!(failure.message, "tool not found");
                assert_eq!(failure.code, -32602);
            }
            other => panic!("expected a failure, got {other:?}"),
        }
    }
}
