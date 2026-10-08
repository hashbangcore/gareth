use crate::core::router::ServiceError;
use crate::util::env::env_var;

/// Configuration resolved from environment variables and CLI flags.
///
/// Gareth is provider agnostic: endpoint, model and key always come from the
/// environment, there is no built-in provider.
pub struct Config {
    pub endpoint: String,
    pub model: String,
    pub apikey: Option<String>,
}

impl Config {
    /// Loads configuration from environment variables.
    pub fn from_env() -> Result<Self, ServiceError> {
        let endpoint = env_var("GARETH_URL");
        let model = env_var("GARETH_MODEL");
        let apikey = env_var("GARETH_API_KEY");

        let missing = missing_vars(endpoint.as_deref(), model.as_deref());
        if !missing.is_empty() {
            return Err(ServiceError::Config(missing_message(&missing)));
        }

        Ok(Self {
            endpoint: endpoint.unwrap_or_default(),
            model: model.unwrap_or_default(),
            apikey,
        })
    }
}

/// Lists the variables that still need to be set.
fn missing_vars(endpoint: Option<&str>, model: Option<&str>) -> Vec<&'static str> {
    [("GARETH_URL", endpoint), ("GARETH_MODEL", model)]
        .into_iter()
        .filter_map(|(name, value)| value.is_none().then_some(name))
        .collect()
}

/// Builds the message that names every missing variable.
fn missing_message(missing: &[&str]) -> String {
    format!("missing {}", missing.join(" and "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_both_variables_when_nothing_is_set() {
        assert_eq!(missing_vars(None, None), vec!["GARETH_URL", "GARETH_MODEL"]);
    }

    #[test]
    fn lists_only_the_missing_one() {
        assert_eq!(
            missing_vars(Some("http://x/v1"), None),
            vec!["GARETH_MODEL"]
        );
        assert_eq!(missing_vars(None, Some("model")), vec!["GARETH_URL"]);
    }

    #[test]
    fn lists_nothing_when_both_are_set() {
        assert!(missing_vars(Some("http://x/v1"), Some("model")).is_empty());
    }

    #[test]
    fn names_every_missing_variable_in_one_message() {
        assert_eq!(
            missing_message(&missing_vars(None, None)),
            "missing GARETH_URL and GARETH_MODEL"
        );
        assert_eq!(
            missing_message(&missing_vars(Some("http://x/v1"), None)),
            "missing GARETH_MODEL"
        );
    }
}
