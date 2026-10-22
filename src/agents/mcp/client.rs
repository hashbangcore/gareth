//! A client of the Model Context Protocol, using rmcp for stdio and HTTP.

use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::process::Stdio;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use http::{HeaderName, HeaderValue};
use rmcp::{
    ClientHandler, ClientLifecycleMode, ClientServiceExt, RoleClient,
    model::{
        CallToolRequestParams, ClientCapabilities, ClientConfig, Implementation, ProtocolVersion,
    },
    service::{ClientInitializeError, MaybeSendFuture, NotificationContext, RunningService},
    transport::{
        StreamableHttpClientTransport, TokioChildProcess,
        streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{ChildStderr, Command},
};

use super::config::{Server, Transport};
use super::protocol::{Initialize, Tool};

/// The name gareth gives to MCP servers.
const CLIENT_NAME: &str = "gareth";

/// The version gareth gives to MCP servers.
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_STDERR_BYTES: usize = 32 * 1024;

/// Why an MCP connection or request could not be completed.
#[derive(Debug)]
pub enum McpError {
    /// The server process could not be started.
    Start {
        /// The command that was run.
        command: String,
        /// What went wrong.
        source: io::Error,
    },
    /// The MCP transport or server returned an error.
    Transport {
        /// What went wrong.
        message: String,
    },
    /// A server returned data Gareth could not parse.
    Parse {
        /// What went wrong.
        source: serde_json::Error,
    },
    /// The server answered with an error.
    Failed {
        /// What failed, the name of the tool when there is one.
        subject: String,
        /// What the server reported.
        message: String,
    },
}

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Start { command, source } => write!(f, "could not run {command}: {source}"),
            Self::Transport { message } => write!(f, "MCP transport failed: {message}"),
            Self::Parse { source } => write!(f, "the server returned unreadable data: {source}"),
            Self::Failed { subject, message } => write!(f, "the {subject} failed: {message}"),
        }
    }
}

impl std::error::Error for McpError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Start { source, .. } => Some(source),
            Self::Parse { source } => Some(source),
            Self::Transport { .. } | Self::Failed { .. } => None,
        }
    }
}

/// Receives MCP notifications and provides Gareth's client identity.
struct GarethClientHandler {
    info: ClientConfig,
    tools_changed: Arc<AtomicBool>,
}

impl GarethClientHandler {
    fn new(tools_changed: Arc<AtomicBool>, protocol_version: ProtocolVersion) -> Self {
        Self {
            info: ClientConfig::new(
                ClientCapabilities::default(),
                Implementation::new(CLIENT_NAME, CLIENT_VERSION),
            )
            .with_protocol_version(protocol_version),
            tools_changed,
        }
    }
}

impl ClientHandler for GarethClientHandler {
    fn get_info(&self) -> ClientConfig {
        self.info.clone()
    }

    fn on_tool_list_changed(
        &self,
        _context: NotificationContext<RoleClient>,
    ) -> impl Future<Output = ()> + MaybeSendFuture + '_ {
        self.tools_changed.store(true, Ordering::Release);
        std::future::ready(())
    }
}

/// A running MCP client for either stdio or Streamable HTTP.
pub struct Client {
    service: RunningService<RoleClient, GarethClientHandler>,
    tools_changed: Arc<AtomicBool>,
    stderr: Arc<std::sync::Mutex<String>>,
}

impl Client {
    /// Connects using the transport specified in the server configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the child process cannot start or MCP initialization fails.
    pub async fn start(server: &Server) -> Result<Self, McpError> {
        let tools_changed = Arc::new(AtomicBool::new(false));
        let stderr = Arc::new(std::sync::Mutex::new(String::new()));
        let lifecycle = ClientLifecycleMode::Auto {
            preferred_versions: vec![ProtocolVersion::LATEST],
            legacy_version: Some(ProtocolVersion::V_2025_11_25),
        };

        let service = match start_service(
            server,
            tools_changed.clone(),
            stderr.clone(),
            lifecycle,
            ProtocolVersion::LATEST,
        )
        .await
        {
            Ok(service) => service,
            Err(StartError::Transport(error)) if should_retry_legacy(&error) => {
                // Restart with a clean legacy handshake when discovery responses are miscorrelated.
                start_service(
                    server,
                    tools_changed.clone(),
                    stderr.clone(),
                    ClientLifecycleMode::Initialize,
                    ProtocolVersion::V_2025_11_25,
                )
                .await
                .map_err(|error| error.into_mcp_error(&stderr))?
            }
            Err(error) => return Err(error.into_mcp_error(&stderr)),
        };

        Ok(Self {
            service,
            tools_changed,
            stderr,
        })
    }

    /// Returns the identity, protocol version, and instructions of the server.
    pub async fn initialize(&mut self) -> Result<Initialize, McpError> {
        let info = self
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
                    .map(|server| server.name.clone())
                    .unwrap_or_else(|| "MCP server".into()),
                version: server
                    .map(|server| server.version.clone())
                    .unwrap_or_default(),
            },
            instructions: info.instructions.clone(),
        })
    }

    /// Lists every tool offered by the connected server.
    pub async fn list_tools(&mut self) -> Result<Vec<Tool>, McpError> {
        self.tools_changed.store(false, Ordering::Release);
        self.service
            .list_all_tools()
            .await
            .map_err(|err| McpError::Transport {
                message: error_with_stderr(err.to_string(), &self.stderr),
            })?
            .into_iter()
            .map(|tool| {
                let input_schema = serde_json::to_value(&tool.input_schema)
                    .map_err(|source| McpError::Parse { source })?;
                Ok(Tool {
                    name: tool.name.to_string(),
                    description: tool
                        .description
                        .map(|description| description.to_string())
                        .unwrap_or_default(),
                    input_schema: Some(input_schema),
                })
            })
            .collect()
    }

    /// Returns and clears the server's tool-list-changed notification flag.
    pub fn take_tools_changed(&mut self) -> bool {
        self.tools_changed.swap(false, Ordering::AcqRel)
    }

    /// Calls a tool and returns its text content.
    pub async fn call_tool(
        &mut self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<String, McpError> {
        let arguments = arguments
            .as_object()
            .cloned()
            .ok_or_else(|| McpError::Failed {
                subject: format!("tool {name}"),
                message: "tool arguments must be a JSON object".into(),
            })?;
        let result = self
            .service
            .call_tool(CallToolRequestParams::new(name.to_string()).with_arguments(arguments))
            .await
            .map_err(|err| McpError::Transport {
                message: error_with_stderr(err.to_string(), &self.stderr),
            })?;
        let value = serde_json::to_value(result).map_err(|source| McpError::Parse { source })?;
        Ok(result_text(&value))
    }
}

enum StartError {
    Process { command: String, source: io::Error },
    Transport(Box<ClientInitializeError>),
    Configuration(McpError),
}

fn should_retry_legacy(error: &ClientInitializeError) -> bool {
    match error {
        ClientInitializeError::UncorrelatedErrorResponse { .. } => true,
        ClientInitializeError::LegacyFallbackFailed { fallback, .. } => matches!(
            fallback.as_ref(),
            ClientInitializeError::UncorrelatedErrorResponse { .. }
        ),
        _ => false,
    }
}

impl StartError {
    fn into_mcp_error(self, stderr: &std::sync::Mutex<String>) -> McpError {
        match self {
            Self::Process { command, source } => McpError::Start { command, source },
            Self::Transport(source) => McpError::Transport {
                message: error_with_stderr(source.to_string(), stderr),
            },
            Self::Configuration(error) => error,
        }
    }
}

async fn start_service(
    server: &Server,
    tools_changed: Arc<AtomicBool>,
    stderr: Arc<std::sync::Mutex<String>>,
    lifecycle: ClientLifecycleMode,
    protocol_version: ProtocolVersion,
) -> Result<RunningService<RoleClient, GarethClientHandler>, StartError> {
    let handler = GarethClientHandler::new(tools_changed, protocol_version);
    match &server.transport {
        Transport::Stdio { command, args, env } => {
            let mut command_process = Command::new(command);
            command_process.args(args).envs(env);
            let (transport, child_stderr) = TokioChildProcess::builder(command_process)
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|source| StartError::Process {
                    command: command.clone(),
                    source,
                })?;
            if let Some(child_stderr) = child_stderr {
                tokio::spawn(capture_stderr(child_stderr, stderr));
            }
            handler
                .serve_with_lifecycle(transport, lifecycle)
                .await
                .map_err(|error| StartError::Transport(Box::new(error)))
        }
        Transport::Http { url, headers } => {
            let parsed_headers = parse_http_headers(headers).map_err(StartError::Configuration)?;
            let transport = StreamableHttpClientTransport::from_config(
                StreamableHttpClientTransportConfig::with_uri(url.clone())
                    .custom_headers(parsed_headers),
            );
            handler
                .serve_with_lifecycle(transport, lifecycle)
                .await
                .map_err(|error| StartError::Transport(Box::new(error)))
        }
    }
}

async fn capture_stderr(stderr: ChildStderr, buffer: Arc<std::sync::Mutex<String>>) {
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let mut text = buffer.lock().unwrap_or_else(|error| error.into_inner());
        text.push_str(&line);
        text.push('\n');
        if text.len() > MAX_STDERR_BYTES {
            let excess = text.len() - MAX_STDERR_BYTES;
            let boundary = text
                .char_indices()
                .find(|(index, _)| *index >= excess)
                .map(|(index, _)| index)
                .unwrap_or(text.len());
            text.drain(..boundary);
        }
    }
}

fn error_with_stderr(error: String, stderr: &std::sync::Mutex<String>) -> String {
    let details = stderr
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .trim()
        .to_string();
    if details.is_empty() {
        error
    } else {
        format!("{error}; server stderr: {details}")
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

/// Turns an MCP tool result into the text the model reads.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_legacy_for_uncorrelated_discover_or_fallback_errors() {
        let uncorrelated = ClientInitializeError::UncorrelatedErrorResponse {
            expected: rmcp::model::RequestId::Number(1),
            received: rmcp::model::RequestId::Number(0),
        };
        assert!(should_retry_legacy(&uncorrelated));

        let fallback_error = ClientInitializeError::LegacyFallbackFailed {
            discover: Box::new(ClientInitializeError::JsonRpcError(
                rmcp::model::ErrorData::new(
                    rmcp::model::ErrorCode::METHOD_NOT_FOUND,
                    "server/discover",
                    None,
                ),
            )),
            fallback: Box::new(uncorrelated),
        };
        assert!(should_retry_legacy(&fallback_error));

        assert!(!should_retry_legacy(
            &ClientInitializeError::ConnectionClosed("disconnected".into())
        ));
    }

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

    #[cfg(unix)]
    #[tokio::test]
    async fn stdio_transport_initializes_lists_and_invokes_with_an_embedded_mock_server() {
        let script = r#"
printf 'fixture stderr diagnostic\n' >&2
while IFS= read -r request; do
  id=$(printf '%s\n' "$request" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')
  [ -n "$id" ] || continue
  case "$request" in
    *'"method":"server/discover"'*)
      printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32601,"message":"legacy MCP server"}}\n' "$id"
      ;;
    *'"method":"initialize"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-11-25","capabilities":{"tools":{"listChanged":true}},"serverInfo":{"name":"stdio-fixture","version":"1"},"instructions":"Use the fixture tool."}}\n' "$id"
      ;;
    *'"method":"tools/list"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"fixture_echo","description":"Echo fixture input","inputSchema":{"type":"object","properties":{"text":{"type":"string"}}}}]}}\n' "$id"
      ;;
    *'"method":"tools/call"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"%s"}]}}\n' "$id" "$GARETH_FIXTURE_MESSAGE"
      ;;
  esac
done
"#;
        let server = Server {
            transport: Transport::Stdio {
                command: "/bin/sh".into(),
                args: vec!["-c".into(), script.into()],
                env: [(
                    "GARETH_FIXTURE_MESSAGE".into(),
                    "stdio fixture called".into(),
                )]
                .into_iter()
                .collect(),
            },
        };

        let mut client = Client::start(&server).await.unwrap();
        let info = client.initialize().await.unwrap();
        assert_eq!(info.server_info.name, "stdio-fixture");
        assert_eq!(info.instructions.as_deref(), Some("Use the fixture tool."));

        let tools = client.list_tools().await.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "fixture_echo");
        assert_eq!(tools[0].description, "Echo fixture input");
        assert_eq!(
            tools[0].parameters()["properties"]["text"]["type"],
            "string"
        );

        let result = client
            .call_tool("fixture_echo", serde_json::json!({ "text": "test" }))
            .await
            .unwrap();
        assert_eq!(result, "stdio fixture called");
        assert!(
            client
                .stderr
                .lock()
                .unwrap()
                .contains("fixture stderr diagnostic")
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn retries_legacy_initialization_when_auto_fallback_is_uncorrelated() {
        let marker =
            std::env::temp_dir().join(format!("gareth-mcp-fallback-{}.marker", std::process::id()));
        if marker.exists() {
            std::fs::remove_file(&marker).unwrap();
        }
        let script = r#"
if [ ! -f "$GARETH_FIXTURE_MARKER" ]; then
  touch "$GARETH_FIXTURE_MARKER"
  while IFS= read -r request; do
    id=$(printf '%s\n' "$request" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')
    case "$request" in
      *'"method":"server/discover"'*)
        printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32601,"message":"legacy MCP server"}}\n' "$id"
        ;;
      *'"method":"initialize"'*)
        printf '{"jsonrpc":"2.0","id":0,"error":{"code":-32602,"message":"uncorrelated legacy response"}}\n'
        exit 0
        ;;
    esac
  done
else
  while IFS= read -r request; do
    id=$(printf '%s\n' "$request" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')
    [ -n "$id" ] || continue
    case "$request" in
      *'"method":"initialize"'*)
        printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"legacy-fixture","version":"1"}}}\n' "$id"
        ;;
    esac
  done
fi
"#;
        let server = Server {
            transport: Transport::Stdio {
                command: "/bin/sh".into(),
                args: vec!["-c".into(), script.into()],
                env: [(
                    "GARETH_FIXTURE_MARKER".into(),
                    marker.to_string_lossy().into_owned(),
                )]
                .into_iter()
                .collect(),
            },
        };

        let mut client = Client::start(&server).await.unwrap();
        let info = client.initialize().await.unwrap();

        assert_eq!(info.server_info.name, "legacy-fixture");
        assert_eq!(info.protocol_version, "2025-11-25");
        std::fs::remove_file(marker).unwrap();
    }

    #[test]
    fn appends_captured_stderr_to_transport_errors() {
        let stderr = std::sync::Mutex::new("useful child diagnostic\n".into());
        let error = error_with_stderr("MCP initialization failed".into(), &stderr);
        assert!(error.contains("MCP initialization failed"));
        assert!(error.contains("useful child diagnostic"));
    }
}
