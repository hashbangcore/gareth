//! Reading environment variables.

use std::env;

/// Returns the value of an environment variable, ignoring blank values.
pub fn env_var(key: &str) -> Option<String> {
    match env::var(key) {
        Ok(value) if !value.trim().is_empty() => Some(value.trim().to_string()),
        _ => None,
    }
}
