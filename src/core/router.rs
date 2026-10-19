use std::error::Error;
use std::fmt;
use std::time::Duration;

use crate::core::Cli;
use crate::core::config::Config;
use crate::core::trace::send_trace;

use reqwest::Client;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

/// Maximum number of body characters kept in error messages.
const SNIPPET_LIMIT: usize = 400;

/// Seconds allowed for the TCP connection to be established.
///
/// A total timeout is deliberately not set: long answers and slow models are normal,
/// while a connection that never completes is not.
const CONNECT_TIMEOUT_SECS: u64 = 10;

/// Holds everything needed to talk to an OpenAI-compatible endpoint.
pub struct Service {
    http: Client,
    apikey: Option<String>,
    endpoint: String,
    model: String,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: String,
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
    pub fn new(args: &Cli) -> Result<Self, ServiceError> {
        let config = Config::from_env()?;

        if args.verbose {
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

    /// Builds a chat completion request.
    pub(crate) fn request(&self, content: &str) -> reqwest::RequestBuilder {
        let body = ChatRequest {
            model: self.model.clone(),
            messages: vec![Message {
                role: "user".to_string(),
                content: content.to_string(),
            }],
        };

        let mut req = self.http.post(&self.endpoint).json(&body);
        if let Some(key) = &self.apikey {
            req = req.header("Authorization", format!("Bearer {key}"));
        }
        req
    }

    pub async fn complete(&self, content: &str) -> Result<String, ServiceError> {
        // Send request/response to the trace server when enabled.
        send_trace(":: REQUEST ::", content).await;

        let response =
            self.request(content)
                .send()
                .await
                .map_err(|source| ServiceError::Transport {
                    endpoint: self.endpoint.clone(),
                    source,
                })?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|source| ServiceError::Read { source })?;

        let parsed = parse_response(status, &body);
        if parsed.is_err() {
            trace_failed_response(status, &body).await;
        }

        let content = parsed?;

        send_trace(":: RESPONSE ::", &content).await;
        Ok(content)
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
    let response: ChatResponse =
        serde_json::from_str(body).map_err(|err| ServiceError::UnexpectedBody {
            reason: err.to_string(),
            snippet: snippet(body, SNIPPET_LIMIT),
        })?;

    let choice = response.choices.first().ok_or(ServiceError::NoChoices)?;
    if choice.message.content.trim().is_empty() {
        return Err(ServiceError::EmptyContent);
    }
    Ok(choice.message.content.clone())
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
    const OK_BODY: &str = r#"{"id":"chatcmpl-1","object":"chat.completion","created":1,"model":"qwen2.5:0.5b","choices":[{"index":0,"message":{"role":"assistant","content":"Hola, mi nombre es Qwen"},"finish_reason":"stop"}],"usage":{"prompt_tokens":36,"completion_tokens":20,"total_tokens":56}}"#;

    #[test]
    fn reads_the_assistant_content_from_a_completion() {
        let content = parse_completion(OK_BODY).unwrap();
        assert_eq!(content, "Hola, mi nombre es Qwen");
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
            "Hola, mi nombre es Qwen"
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
        assert_eq!(snippet("áéíóú", 3), "áéí...");
    }
}
