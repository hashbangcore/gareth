//! MCP data Gareth passes to its agent layer.

use serde::Deserialize;

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
#[derive(Clone, Debug, Deserialize, PartialEq)]
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

#[cfg(test)]
mod tests {
    use super::*;

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
