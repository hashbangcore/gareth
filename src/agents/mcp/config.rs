//! The configuration that names the MCP servers.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::util::env::env_var;

/// A server that gareth can start, as it is written in the configuration.
#[derive(Clone, Debug)]
pub struct Server {
    /// The program to run.
    pub command: String,
    /// The arguments passed to the program.
    pub args: Vec<String>,
    /// The environment variables the program gets.
    pub env: BTreeMap<String, String>,
}

/// The servers of the configuration file.
#[derive(Clone, Debug, Default)]
pub struct Config {
    /// The servers, by name.
    pub servers: BTreeMap<String, Server>,
}

/// Why a configuration file could not be used.
#[derive(Debug)]
pub enum ConfigError {
    /// There is no file there.
    NotFound {
        /// The path that was looked at.
        path: PathBuf,
    },
    /// The file could not be read.
    Read {
        /// The path of the file.
        path: PathBuf,
        /// What went wrong.
        source: io::Error,
    },
    /// The file does not hold the expected shape.
    Shape {
        /// The path of the file.
        path: PathBuf,
        /// What is wrong with it.
        reason: String,
    },
    /// The configuration has no server with that name.
    Missing {
        /// The name that was asked for.
        name: String,
        /// The names the configuration has.
        available: Vec<String>,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { path } => {
                write!(f, "{}: no such file", path.display())
            }
            Self::Read { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Shape { path, reason } => write!(f, "{}: {reason}", path.display()),
            Self::Missing { name, available } => {
                write!(f, "no server named {name}")?;
                if !available.is_empty() {
                    write!(f, ", the configuration has {}", available.join(", "))?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// A server as it is written in the file.
#[derive(Debug, Deserialize)]
struct RawServer {
    /// The program to run.
    command: String,
    /// The arguments passed to the program.
    #[serde(default)]
    args: Vec<String>,
    /// The environment variables the program gets.
    #[serde(default)]
    env: BTreeMap<String, String>,
}

impl Config {
    /// Reads the configuration of a file.
    ///
    /// # Errors
    ///
    /// Returns an error when the file is not there, when it cannot be read or
    /// when it does not hold the expected shape.
    pub fn read(path: &Path) -> Result<Self, ConfigError> {
        let text = fs::read_to_string(path).map_err(|source| match source.kind() {
            io::ErrorKind::NotFound => ConfigError::NotFound {
                path: path.to_path_buf(),
            },
            _ => ConfigError::Read {
                path: path.to_path_buf(),
                source,
            },
        })?;

        parse(path, &text)
    }

    /// Returns the server with that name.
    ///
    /// # Errors
    ///
    /// Returns an error when the configuration has no server with that name.
    pub fn server(&self, name: &str) -> Result<&Server, ConfigError> {
        self.servers.get(name).ok_or_else(|| ConfigError::Missing {
            name: name.to_string(),
            available: self.servers.keys().cloned().collect(),
        })
    }

    /// Returns the path where the configuration lives by default.
    pub fn default_path() -> PathBuf {
        let base = env_var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| env_var("HOME").map(|home| PathBuf::from(home).join(".config")))
            .unwrap_or_else(|| PathBuf::from(".config"));

        base.join("gareth").join("mcp.json")
    }
}

/// Reads the servers of a configuration file.
fn parse(path: &Path, text: &str) -> Result<Config, ConfigError> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|source| ConfigError::Shape {
            path: path.to_path_buf(),
            reason: source.to_string(),
        })?;

    let servers = value.get("mcpServers").unwrap_or(&value);

    let Some(map) = servers.as_object() else {
        return Err(ConfigError::Shape {
            path: path.to_path_buf(),
            reason: "the servers are not an object".to_string(),
        });
    };

    let mut out = BTreeMap::new();

    for (name, raw) in map {
        let raw: RawServer = serde_json::from_value(raw.clone()).map_err(|source| {
            ConfigError::Shape {
                path: path.to_path_buf(),
                reason: format!("the server {name} is wrong: {source}"),
            }
        })?;

        out.insert(
            name.clone(),
            Server {
                command: raw.command,
                args: raw.args,
                env: raw.env,
            },
        );
    }

    Ok(Config { servers: out })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape_error(error: ConfigError) -> String {
        error.to_string()
    }

    #[test]
    fn reads_the_servers_of_the_standard_shape() {
        let text = r#"{
            "mcpServers": {
                "nixos": { "command": "mcp-nixos", "args": [], "env": {} },
                "vulnx": { "command": "vulnx", "args": ["mcp"] }
            }
        }"#;

        let config = parse(Path::new("mcp.json"), text).unwrap();

        assert_eq!(config.servers.len(), 2);
        assert_eq!(config.server("nixos").unwrap().command, "mcp-nixos");
        assert_eq!(config.server("vulnx").unwrap().args, vec!["mcp"]);
    }

    #[test]
    fn ignores_the_fields_it_does_not_know() {
        let text = r#"{
            "mcpServers": {
                "nixos": { "type": "local", "command": "mcp-nixos" }
            }
        }"#;

        let config = parse(Path::new("mcp.json"), text).unwrap();

        assert_eq!(config.server("nixos").unwrap().command, "mcp-nixos");
    }

    #[test]
    fn reads_a_file_that_holds_the_servers_alone() {
        let text = r#"{ "nixos": { "command": "mcp-nixos" } }"#;

        let config = parse(Path::new("mcp.json"), text).unwrap();

        assert_eq!(config.servers.len(), 1);
    }

    #[test]
    fn keeps_the_environment_of_a_server() {
        let text = r#"{
            "mcpServers": {
                "nixos": { "command": "mcp-nixos", "env": { "KEY": "value" } }
            }
        }"#;

        let config = parse(Path::new("mcp.json"), text).unwrap();

        assert_eq!(
            config.server("nixos").unwrap().env.get("KEY").map(String::as_str),
            Some("value")
        );
    }

    #[test]
    fn refuses_a_file_that_is_not_json() {
        let err = parse(Path::new("mcp.json"), "not json").unwrap_err();

        assert!(shape_error(err).contains("mcp.json"));
    }

    #[test]
    fn refuses_a_server_without_a_command() {
        let text = r#"{ "mcpServers": { "nixos": { "args": [] } } }"#;

        let err = parse(Path::new("mcp.json"), text).unwrap_err();

        assert!(shape_error(err).contains("nixos"));
    }

    #[test]
    fn names_the_servers_it_has_when_one_is_missing() {
        let text = r#"{ "mcpServers": { "nixos": { "command": "mcp-nixos" } } }"#;
        let config = parse(Path::new("mcp.json"), text).unwrap();

        let err = config.server("vulnx").unwrap_err();

        assert_eq!(err.to_string(), "no server named vulnx, the configuration has nixos");
    }

    #[test]
    fn reports_a_file_that_is_not_there() {
        let path = std::env::temp_dir().join(format!(
            "gareth-mcp-test-no-existe-{}.json",
            std::process::id()
        ));

        let err = Config::read(&path).unwrap_err();

        assert!(matches!(err, ConfigError::NotFound { .. }));
        assert!(err.to_string().contains("no such file"));
    }
}
