//! The messages of the Model Context Protocol.

use serde::{Deserialize, Serialize};

/// A request sent to the server.
#[derive(Debug, Serialize)]
pub struct Request {
    /// The version of JSON-RPC.
    pub jsonrpc: &'static str,
    /// The identifier of the request.
    pub id: u64,
    /// The method to call.
    pub method: String,
    /// The parameters of the method.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// A notification sent to the server, which expects no answer.
#[derive(Debug, Serialize)]
pub struct Notification {
    /// The version of JSON-RPC.
    pub jsonrpc: &'static str,
    /// The method to call.
    pub method: String,
}

/// A message read from the server.
#[derive(Debug, Deserialize)]
pub struct Message {
    /// The identifier of the request it answers.
    #[serde(default)]
    pub id: Option<serde_json::Value>,
    /// The method, when the server asks for something.
    #[serde(default)]
    pub method: Option<String>,
    /// The result of the call.
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    /// The error the call failed with.
    #[serde(default)]
    pub error: Option<Failure>,
}

/// The error a call failed with.
#[derive(Debug, Deserialize)]
pub struct Failure {
    /// The code of the error.
    #[serde(default)]
    pub code: i64,
    /// What went wrong.
    #[serde(default)]
    pub message: String,
}

/// What the server says about itself.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ServerInfo {
    /// The name of the server.
    #[serde(default)]
    pub name: String,
    /// The version of the server.
    #[serde(default)]
    pub version: String,
}

/// The answer of the initialize method.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Initialize {
    /// The version of the protocol the server speaks.
    #[serde(default)]
    pub protocol_version: String,
    /// What the server says about itself.
    #[serde(default)]
    pub server_info: ServerInfo,
    /// The instructions the server gives to the client.
    #[serde(default)]
    pub instructions: Option<String>,
}

/// A tool the server offers.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    /// The name of the tool.
    pub name: String,
    /// What the tool does.
    #[serde(default)]
    pub description: String,
    /// The schema of the arguments.
    #[serde(default)]
    pub input_schema: Option<serde_json::Value>,
}

impl Tool {
    /// Returns the schema of the arguments, always an object schema.
    pub fn parameters(&self) -> serde_json::Value {
        let Some(schema) = &self.input_schema else {
            return serde_json::json!({ "type": "object", "properties": {} });
        };

        if !schema.is_object() {
            return serde_json::json!({ "type": "object", "properties": {} });
        }

        let mut schema = schema.clone();
        if schema.get("type").is_none() {
            schema["type"] = serde_json::json!("object");
        }
        schema
    }
}

/// The answer of the tools/list method.
#[derive(Clone, Debug, Deserialize)]
pub struct Tools {
    /// The tools the server offers.
    #[serde(default)]
    pub tools: Vec<Tool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_request_with_its_identifier() {
        let request = Request {
            jsonrpc: "2.0",
            id: 3,
            method: "tools/list".to_string(),
            params: Some(serde_json::json!({ "cursor": "next" })),
        };

        let value = serde_json::to_value(&request).unwrap();

        assert_eq!(value["jsonrpc"], "2.0");
        assert_eq!(value["id"], 3);
        assert_eq!(value["method"], "tools/list");
        assert_eq!(value["params"]["cursor"], "next");
    }

    #[test]
    fn writes_a_request_without_parameters() {
        let request = Request {
            jsonrpc: "2.0",
            id: 1,
            method: "tools/list".to_string(),
            params: None,
        };

        let value = serde_json::to_value(&request).unwrap();

        assert!(value.get("params").is_none());
    }

    #[test]
    fn writes_a_notification_without_an_identifier() {
        let notification = Notification {
            jsonrpc: "2.0",
            method: "notifications/initialized".to_string(),
        };

        let value = serde_json::to_value(&notification).unwrap();

        assert_eq!(value["method"], "notifications/initialized");
        assert!(value.get("id").is_none());
    }

    #[test]
    fn reads_the_answer_of_the_server() {
        let answer: Initialize = serde_json::from_value(serde_json::json!({
            "protocolVersion": "2025-06-18",
            "serverInfo": { "name": "fake", "version": "1.0" },
            "instructions": "use the tools"
        }))
        .unwrap();

        assert_eq!(answer.protocol_version, "2025-06-18");
        assert_eq!(answer.server_info.name, "fake");
        assert_eq!(answer.server_info.version, "1.0");
        assert_eq!(answer.instructions.as_deref(), Some("use the tools"));
    }

    #[test]
    fn reads_an_answer_that_leaves_the_optional_parts_out() {
        let answer: Initialize = serde_json::from_value(serde_json::json!({})).unwrap();

        assert_eq!(answer.protocol_version, "");
        assert_eq!(answer.server_info.name, "");
        assert!(answer.instructions.is_none());
    }

    #[test]
    fn reads_the_tools_of_the_server() {
        let answer: Tools = serde_json::from_value(serde_json::json!({
            "tools": [{
                "name": "nix",
                "description": "Query NixOS",
                "inputSchema": { "type": "object", "properties": { "action": {} } }
            }]
        }))
        .unwrap();

        assert_eq!(answer.tools.len(), 1);
        assert_eq!(answer.tools[0].name, "nix");
        assert_eq!(answer.tools[0].description, "Query NixOS");
        let schema = answer.tools[0].input_schema.as_ref().unwrap();
        assert!(schema["properties"]["action"].is_object());
    }

    #[test]
    fn gives_a_tool_without_a_schema_an_empty_object_schema() {
        let tool: Tool = serde_json::from_value(serde_json::json!({ "name": "nix" })).unwrap();

        let parameters = tool.parameters();

        assert_eq!(parameters["type"], "object");
        assert!(parameters["properties"].as_object().unwrap().is_empty());
    }

    #[test]
    fn keeps_the_schema_of_a_tool() {
        let tool: Tool = serde_json::from_value(serde_json::json!({
            "name": "nix",
            "inputSchema": { "type": "object", "properties": { "action": {} } }
        }))
        .unwrap();

        let parameters = tool.parameters();

        assert_eq!(parameters["type"], "object");
        assert!(parameters["properties"].get("action").is_some());
    }

    #[test]
    fn makes_a_schema_without_a_type_an_object_schema() {
        let tool: Tool = serde_json::from_value(serde_json::json!({
            "name": "nix",
            "inputSchema": { "properties": {} }
        }))
        .unwrap();

        assert_eq!(tool.parameters()["type"], "object");
    }
}
