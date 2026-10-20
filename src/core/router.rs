//! Talks to an OpenAI-compatible endpoint.

use std::error::Error;
use std::fmt;
use std::fmt::Write as _;
use std::time::Duration;

use crate::core::config::Config;
use crate::core::trace::send_trace;

use reqwest::Client;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

/// Maximum number of body characters kept in error messages.
const SNIPPET_LIMIT: usize = 400;

/// Seconds allowed for the TCP connection to be established.
///
/// A total timeout is deliberately not set: long answers and slow models
/// are normal, while a connection that never completes is not.
const CONNECT_TIMEOUT_SECS: u64 = 10;

/// Holds everything needed to talk to an OpenAI-compatible endpoint.
pub struct Service {
    http: Client,
    apikey: Option<String>,
    endpoint: String,
    model: String,
}

/// The role of a message in the conversation.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// The instructions given to the model.
    System,
    /// The question of the user.
    User,
    /// The answer of the model.
    Assistant,
    /// The result of a tool call.
    Tool,
}

impl Role {
    /// Returns the name of the role, as it is shown.
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "SYSTEM",
            Self::User => "USER",
            Self::Assistant => "ASSISTANT",
            Self::Tool => "TOOL",
        }
    }
}

/// A message of the conversation, as it travels to the endpoint.
#[derive(Clone, Debug, Serialize)]
pub struct Message {
    /// The role of the message.
    pub role: Role,
    /// The text of the message.
    pub content: String,
    /// The tool calls an assistant message asks for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// The name of the tool that produced a tool message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The call a tool message answers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    /// Builds a message with a role and a text.
    fn with_role(role: Role, content: &str) -> Self {
        Self {
            role,
            content: content.to_string(),
            tool_calls: None,
            name: None,
            tool_call_id: None,
        }
    }

    /// Builds a message that carries the instructions of the client.
    pub fn system(content: &str) -> Self {
        Self::with_role(Role::System, content)
    }

    /// Builds a message with the question of the user.
    pub fn user(content: &str) -> Self {
        Self::with_role(Role::User, content)
    }

    /// Builds the message where the assistant asks for the given calls.
    pub fn assistant(calls: Vec<ToolCall>) -> Self {
        Self {
            role: Role::Assistant,
            content: String::new(),
            tool_calls: Some(calls),
            name: None,
            tool_call_id: None,
        }
    }

    /// Builds the message with the result of a tool call.
    pub fn tool(call: &ToolCall, content: &str) -> Self {
        Self {
            role: Role::Tool,
            content: content.to_string(),
            tool_calls: None,
            name: Some(call.function.name.clone()),
            tool_call_id: Some(call.id.clone()),
        }
    }
}

/// A tool the model can call.
#[derive(Clone, Debug)]
pub struct Tool {
    /// The name of the tool.
    pub name: String,
    /// What the tool does.
    pub description: String,
    /// The schema of the arguments.
    pub parameters: serde_json::Value,
}

/// The kind of a tool call.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CallKind {
    /// A call to the function of a tool.
    #[default]
    Function,
}

/// A tool call the model asks for.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCall {
    /// The identifier of the call.
    pub id: String,
    /// The kind of the call.
    #[serde(rename = "type", default)]
    pub kind: CallKind,
    /// The function to call.
    pub function: ToolFunction,
}

impl ToolCall {
    /// Returns the arguments of the call as they are.
    ///
    /// Arguments that are not a JSON text are read as no arguments at all.
    pub fn arguments(&self) -> serde_json::Value {
        serde_json::from_str(&self.function.arguments).unwrap_or_else(|_| serde_json::json!({}))
    }
}

/// The function of a tool call.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolFunction {
    /// The name of the tool.
    pub name: String,
    /// The arguments, as a JSON text.
    pub arguments: String,
}

/// What the model answered.
#[derive(Clone, Debug)]
pub enum Turn {
    /// The model wrote an answer.
    Text(String),
    /// The model asks for some tools to be called.
    ToolCalls(Vec<ToolCall>),
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: String,
    messages: &'a [Message],
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ToolRequest>,
}

#[derive(Serialize)]
struct ToolRequest {
    #[serde(rename = "type")]
    kind: &'static str,
    function: ToolDefinition,
}

#[derive(Serialize)]
struct ToolDefinition {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

impl From<&Tool> for ToolRequest {
    fn from(tool: &Tool) -> Self {
        Self {
            kind: "function",
            function: ToolDefinition {
                name: tool.name.clone(),
                description: tool.description.clone(),
                parameters: tool.parameters.clone(),
            },
        }
    }
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Clone, Deserialize)]
struct ResponseMessage {
    #[serde(default, deserialize_with = "null_as_empty")]
    content: String,
    #[serde(default)]
    tool_calls: Option<Vec<ToolCall>>,
}

/// Reads a content that the endpoint sends as null as an empty text.
fn null_as_empty<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

/// Every way a request to the endpoint can fail, with a message meant to be read.
#[derive(Debug)]
pub enum ServiceError {
    /// The HTTP client could not be built.
    Client { source: reqwest::Error },
    /// The endpoint could not be reached.
    Transport {
        endpoint: String,
        source: reqwest::Error,
    },
    /// The response body could not be read.
    Read { source: reqwest::Error },
    /// The endpoint answered with a non-success status.
    Http {
        status: u16,
        reason: String,
        /// Message reported by the server, when it provides one.
        message: Option<String>,
        /// Truncated body, used when the server explains nothing.
        detail: Option<String>,
    },
    /// The body could not be read as a chat completion.
    UnexpectedBody { reason: String, snippet: String },
    /// The completion carried no choices.
    NoChoices,
    /// The first choice carried empty content.
    EmptyContent,
    /// Configuration is incomplete.
    Config(String),
}

impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Client { source } => {
                write!(f, "could not build the HTTP client ({source})")
            }
            Self::Transport { endpoint, source } => {
                write!(f, "could not connect to {endpoint} ({source})")
            }
            Self::Read { source } => {
                write!(f, "the response was cut short while reading ({source})")
            }
            Self::Http {
                status,
                reason,
                message,
                detail,
            } => {
                let reason = if reason.is_empty() {
                    String::new()
                } else {
                    format!(" {reason}")
                };
                match message {
                    Some(message) => write!(f, "{status}{reason}: {message}"),
                    None => match detail {
                        Some(detail) => write!(f, "{status}{reason}: {detail}"),
                        None => write!(f, "{status}{reason}"),
                    },
                }
            }
            Self::UnexpectedBody { reason, snippet } => {
                write!(f, "unrecognized response from the endpoint ({reason})")?;
                if !snippet.is_empty() {
                    write!(f, "\n{snippet}")?;
                }
                Ok(())
            }
            Self::NoChoices => f.write_str("the endpoint response carried no choices"),
            Self::EmptyContent => f.write_str("the model returned an empty response"),
            Self::Config(message) => f.write_str(message),
        }
    }
}

impl Error for ServiceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Client { source } | Self::Read { source } => Some(source),
            Self::Transport { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl Service {
    /// Builds a service from the configuration in the environment.
    ///
    /// # Errors
    ///
    /// Returns an error when the configuration is incomplete or when the HTTP
    /// client cannot be built.
    pub fn new(verbose: bool) -> Result<Self, ServiceError> {
        let config = Config::from_env()?;

        if verbose {
            println!("model: {:#?}\nurl: {:#?}\n", config.model, config.endpoint);
        }

        let http = Client::builder()
            .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
            .build()
            .map_err(|source| ServiceError::Client { source })?;

        Ok(Self {
            http,
            apikey: config.apikey,
            endpoint: config.endpoint,
            model: config.model,
        })
    }

    /// Builds the request that carries the given body.
    fn post(&self, body: &ChatRequest<'_>) -> reqwest::RequestBuilder {
        let mut req = self.http.post(&self.endpoint).json(body);
        if let Some(key) = &self.apikey {
            req = req.header("Authorization", format!("Bearer {key}"));
        }
        req
    }

    /// Sends a prompt to the endpoint and returns the answer.
    ///
    /// # Errors
    ///
    /// Returns an error when the endpoint cannot be reached, when it answers
    /// with a failure or when the answer cannot be read.
    pub async fn complete(&self, content: &str) -> Result<String, ServiceError> {
        send_trace(":: REQUEST ::", content).await;

        let messages = vec![Message::user(content)];
        let body = ChatRequest {
            model: self.model.clone(),
            messages: &messages,
            tools: Vec::new(),
        };

        let response = self.post(&body).send().await.map_err(|source| {
            ServiceError::Transport {
                endpoint: self.endpoint.clone(),
                source,
            }
        })?;

        let status = response.status();
        let body = response.text().await.map_err(|source| ServiceError::Read { source })?;

        let parsed = parse_response(status, &body);
        if parsed.is_err() {
            trace_failed_response(status, &body).await;
        }

        let content = parsed?;

        send_trace(":: RESPONSE ::", &content).await;
        Ok(content)
    }

    /// Sends a conversation that carries tools and returns what the model did.
    ///
    /// The model either answers with a text or asks for some tools to be
    /// called, and the caller decides what to do with the calls.
    ///
    /// # Errors
    ///
    /// Returns an error when the endpoint cannot be reached, when it answers
    /// with a failure or when the answer cannot be read.
    pub async fn complete_with_tools(
        &self,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<Turn, ServiceError> {
        let request = ChatRequest {
            model: self.model.clone(),
            messages,
            tools: tools.iter().map(ToolRequest::from).collect(),
        };

        send_trace(":: REQUEST ::", &describe(messages, tools)).await;

        let response = self.post(&request).send().await.map_err(|source| {
            ServiceError::Transport {
                endpoint: self.endpoint.clone(),
                source,
            }
        })?;

        let status = response.status();
        let body = response.text().await.map_err(|source| ServiceError::Read { source })?;

        let parsed = parse_turn(status, &body);
        if parsed.is_err() {
            trace_failed_response(status, &body).await;
        }

        let turn = parsed?;

        send_trace(":: RESPONSE ::", &describe_turn(&turn)).await;
        Ok(turn)
    }
}

/// Describes the conversation and the tools for the trace.
pub fn describe(messages: &[Message], tools: &[Tool]) -> String {
    let mut text = String::new();

    for tool in tools {
        let _ = write!(
            text,
            ":: TOOL ::\n{}\n{}\n:: END TOOL ::\n\n",
            tool.name, tool.description
        );
    }

    for message in messages {
        let role = message.role.label();
        let _ = write!(
            text,
            ":: {role} ::\n{}\n:: END {role} ::\n\n",
            message.content
        );
    }

    text
}

/// Describes what the model answered for the trace.
fn describe_turn(turn: &Turn) -> String {
    match turn {
        Turn::Text(text) => text.clone(),
        Turn::ToolCalls(calls) => calls
            .iter()
            .map(|call| {
                format!(
                    "{} {}({})",
                    call.function.name, call.function.arguments, call.id
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// Records a failed response in the trace server.
pub(crate) async fn trace_failed_response(status: StatusCode, body: &str) {
    let entry = format!("{}\n{}", status, snippet(body, SNIPPET_LIMIT));
    send_trace(":: ERROR RESPONSE ::", &entry).await;
}

/// Interprets an endpoint response, turning failures into readable errors.
pub(crate) fn parse_response(status: StatusCode, body: &str) -> Result<String, ServiceError> {
    if !status.is_success() {
        let message = error_message(body);
        return Err(ServiceError::Http {
            status: status.as_u16(),
            reason: status.canonical_reason().unwrap_or_default().to_string(),
            detail: match message {
                Some(_) => None,
                None => non_empty(body).map(|body| snippet(&body, SNIPPET_LIMIT)),
            },
            message,
        });
    }

    parse_completion(body)
}

/// Extracts the assistant reply from a chat completion body.
fn parse_completion(body: &str) -> Result<String, ServiceError> {
    let message = parse_message(body)?;

    if message.content.trim().is_empty() {
        return Err(ServiceError::EmptyContent);
    }
    Ok(message.content)
}

/// Interprets an endpoint response that carries tools.
fn parse_turn(status: StatusCode, body: &str) -> Result<Turn, ServiceError> {
    if !status.is_success() {
        return Err(parse_response(status, body).unwrap_err());
    }

    let message = parse_message(body)?;

    match message.tool_calls {
        Some(calls) if !calls.is_empty() => Ok(Turn::ToolCalls(calls)),
        _ => {
            if message.content.trim().is_empty() {
                return Err(ServiceError::EmptyContent);
            }
            Ok(Turn::Text(message.content))
        }
    }
}

/// Reads the message of the first choice of a chat completion body.
fn parse_message(body: &str) -> Result<ResponseMessage, ServiceError> {
    let response: ChatResponse =
        serde_json::from_str(body).map_err(|err| ServiceError::UnexpectedBody {
            reason: err.to_string(),
            snippet: snippet(body, SNIPPET_LIMIT),
        })?;

    let choice = response.choices.first().ok_or(ServiceError::NoChoices)?;
    Ok(choice.message.clone())
}

/// Extracts the message an endpoint reports when it refuses a request.
fn error_message(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;

    if let Some(message) = value.get("error") {
        match message {
            serde_json::Value::String(text) => return non_empty(text),
            serde_json::Value::Object(_) => {
                let from_error = message
                    .get("message")
                    .or_else(|| message.get("detail"))
                    .or_else(|| message.get("error"));
                if let Some(found) = from_error.and_then(serde_json::Value::as_str) {
                    return non_empty(found);
                }
            }
            _ => {}
        }
    }

    value
        .get("detail")
        .and_then(serde_json::Value::as_str)
        .and_then(non_empty)
}

/// Truncates a body so it stays readable in a terminal.
fn snippet(body: &str, max: usize) -> String {
    let trimmed = body.trim();
    match trimmed.char_indices().nth(max) {
        Some((cut, _)) => format!("{}...", trimmed[..cut].trim_end()),
        None => trimmed.to_string(),
    }
}

/// Trims a string and returns it only when it carries content.
fn non_empty(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// A completion body as an OpenAI-compatible server would return it.
    const OK_BODY: &str = r#"{"id":"chatcmpl-1","object":"chat.completion","created":1,"model":"qwen2.5:0.5b","choices":[{"index":0,"message":{"role":"assistant","content":"Hello, my name is Qwen"},"finish_reason":"stop"}],"usage":{"prompt_tokens":36,"completion_tokens":20,"total_tokens":56}}"#;

    /// A completion body where the model asks for a tool.
    const TOOL_BODY: &str = r#"{"choices":[{"index":0,"message":{"role":"assistant","content":"","tool_calls":[{"id":"call-1","type":"function","function":{"name":"get_weather","arguments":"{\"city\": \"Madrid\"}"}}]},"finish_reason":"tool_calls"}]}"#;

    #[test]
    fn reads_a_turn_that_asks_for_a_tool() {
        let turn = parse_turn(StatusCode::OK, TOOL_BODY).unwrap();

        match turn {
            Turn::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].function.name, "get_weather");
                assert_eq!(calls[0].id, "call-1");
            }
            other => panic!("expected tool calls, got {other:?}"),
        }
    }

    #[test]
    fn reads_a_turn_that_answers_with_a_text() {
        let turn = parse_turn(StatusCode::OK, OK_BODY).unwrap();

        match turn {
            Turn::Text(text) => assert_eq!(text, "Hello, my name is Qwen"),
            other => panic!("expected a text, got {other:?}"),
        }
    }

    #[test]
    fn reports_a_turn_without_content_and_without_calls() {
        let body = r#"{"choices":[{"index":0,"message":{"role":"assistant","content":"  "}}]}"#;

        let err = parse_turn(StatusCode::OK, body).unwrap_err();

        assert!(matches!(err, ServiceError::EmptyContent));
    }

    #[test]
    fn reports_a_turn_that_the_endpoint_refuses() {
        let body = r#"{"error":{"message":"model not found"}}"#;

        let err = parse_turn(StatusCode::NOT_FOUND, body).unwrap_err();

        assert!(err.to_string().contains("model not found"));
    }

    #[test]
    fn writes_the_role_of_every_message() {
        let messages = vec![
            Message::system("rules"),
            Message::user("question"),
            Message::assistant(vec![]),
        ];

        let value = serde_json::to_value(&messages).unwrap();

        assert_eq!(value[0]["role"], "system");
        assert_eq!(value[1]["role"], "user");
        assert_eq!(value[2]["role"], "assistant");
    }

    #[test]
    fn writes_the_name_of_a_tool_message() {
        let call = ToolCall {
            id: "call-1".to_string(),
            kind: CallKind::Function,
            function: ToolFunction {
                name: "get_weather".to_string(),
                arguments: "{}".to_string(),
            },
        };

        let value = serde_json::to_value(Message::tool(&call, "20 degrees")).unwrap();

        assert_eq!(value["role"], "tool");
        assert_eq!(value["name"], "get_weather");
        assert_eq!(value["tool_call_id"], "call-1");
    }

    #[test]
    fn writes_the_kind_of_a_tool_call() {
        let value = serde_json::to_value(ToolCall {
            id: "call-1".to_string(),
            kind: CallKind::Function,
            function: ToolFunction {
                name: "get_weather".to_string(),
                arguments: "{}".to_string(),
            },
        })
        .unwrap();

        assert_eq!(value["type"], "function");
    }

    #[test]
    fn reads_the_arguments_of_a_call() {
        let call = ToolCall {
            id: "call-1".to_string(),
            kind: CallKind::Function,
            function: ToolFunction {
                name: "get_weather".to_string(),
                arguments: "{\"city\": \"Madrid\"}".to_string(),
            },
        };

        assert_eq!(call.arguments()["city"], "Madrid");
    }

    #[test]
    fn reads_no_arguments_when_they_are_not_json() {
        let call = ToolCall {
            id: "call-1".to_string(),
            kind: CallKind::Function,
            function: ToolFunction {
                name: "get_weather".to_string(),
                arguments: "Madrid".to_string(),
            },
        };

        assert!(call.arguments().as_object().unwrap().is_empty());
    }

    #[test]
    fn describes_the_conversation_and_the_tools() {
        let tools = vec![Tool {
            name: "nix".to_string(),
            description: "Query NixOS".to_string(),
            parameters: serde_json::json!({ "type": "object" }),
        }];

        let text = describe(&[Message::user("question")], &tools);

        assert!(text.contains(":: TOOL ::\nnix\nQuery NixOS"));
        assert!(text.contains(":: USER ::\nquestion"));
    }

    #[test]
    fn reads_the_assistant_content_from_a_completion() {
        let content = parse_completion(OK_BODY).unwrap();
        assert_eq!(content, "Hello, my name is Qwen");
    }

    #[test]
    fn reports_a_completion_without_choices() {
        assert!(matches!(
            parse_completion(r#"{"choices":[]}"#),
            Err(ServiceError::NoChoices)
        ));
    }

    #[test]
    fn reports_empty_content() {
        let err = parse_completion(r#"{"choices":[{"message":{"content":"   "}}]}"#).unwrap_err();
        assert!(matches!(err, ServiceError::EmptyContent));
        assert_eq!(err.to_string(), "the model returned an empty response");
    }

    #[test]
    fn reports_unparseable_bodies_with_a_snippet() {
        let err = parse_completion("{\"choices\":[{\"mess").unwrap_err();
        assert!(matches!(err, ServiceError::UnexpectedBody { .. }));
        assert!(err.to_string().contains("{\"choices\":[{\"mess"));

        // A native Ollama reply is valid JSON but not a chat completion.
        let native = parse_completion(r#"{"model":"qwen2.5:0.5b","done":true}"#).unwrap_err();
        assert!(native.to_string().contains("missing field `choices`"));
        assert!(native.to_string().contains("qwen2.5:0.5b"));
    }

    #[test]
    fn accepts_a_successful_response() {
        assert_eq!(
            parse_response(StatusCode::OK, OK_BODY).unwrap(),
            "Hello, my name is Qwen"
        );
    }

    #[test]
    fn surfaces_the_message_the_endpoint_reports() {
        let body = r#"{"error":{"message":"minimax-m2.5 was retired at 2026-07-31 00:00:00 -0700 PDT (ref: 1a338720)","type":"api_error","param":null,"code":null}}"#;
        let err = parse_response(StatusCode::GONE, body).unwrap_err();

        let shown = err.to_string();
        assert!(shown.starts_with("410 Gone: "), "shown was {shown:?}");
        assert!(
            shown.contains("minimax-m2.5 was retired at 2026-07-31"),
            "shown was {shown:?}"
        );
        assert!(shown.contains("1a338720"));
    }

    #[test]
    fn falls_back_to_the_body_when_the_endpoint_explains_nothing() {
        let err = parse_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "<html><body>502 Bad Gateway</body></html>",
        )
        .unwrap_err();

        let shown = err.to_string();
        assert!(
            shown.starts_with("500 Internal Server Error: "),
            "{shown:?}"
        );
        assert!(shown.contains("502 Bad Gateway"));
    }

    #[test]
    fn reads_error_messages_in_every_shape() {
        assert_eq!(
            error_message(r#"{"error":{"message":"model not found"}}"#).as_deref(),
            Some("model not found")
        );
        assert_eq!(
            error_message(r#"{"error":{"code":"x","message":"bad key"}}"#).as_deref(),
            Some("bad key")
        );
        assert_eq!(
            error_message(r#"{"error":"model not found"}"#).as_deref(),
            Some("model not found")
        );
        assert_eq!(
            error_message(r#"{"detail":"unprocessable"}"#).as_deref(),
            Some("unprocessable")
        );
        assert_eq!(error_message(r#"{"choices":[]}"#), None);
        assert_eq!(error_message("<html>500</html>"), None);
        assert_eq!(error_message(r#"{"error":{"message":"  "}}"#), None);
        assert_eq!(error_message(""), None);
    }

    #[test]
    fn truncates_long_bodies() {
        let body = "a".repeat(50);
        assert_eq!(snippet(&body, 50), body);
        assert_eq!(snippet(&body, 10), format!("{}...", "a".repeat(10)));
        assert_eq!(snippet("  limpio  ", 40), "limpio");
        // Truncation counts characters, not bytes.
        assert_eq!(snippet("aàéè", 3), "aàé...");
    }
}
