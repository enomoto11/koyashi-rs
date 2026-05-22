//! Loading of the optional `koyashi.toml` configuration file.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::model::Classification;

/// File name looked up at the workspace root.
const CONFIG_FILE: &str = "koyashi.toml";

/// Contents of a workspace's `koyashi.toml`.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KoyashiConfig {
    /// Maps a `Struct::field` name to the classifications suppressed for it.
    #[serde(default)]
    suppressions: BTreeMap<String, Vec<Classification>>,
}

impl KoyashiConfig {
    /// Load `koyashi.toml` from `workspace_root`. An absent file yields an
    /// empty configuration that suppresses nothing.
    pub fn load(workspace_root: &Path) -> Result<Self> {
        let path = workspace_root.join(CONFIG_FILE);
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
    }

    /// Whether `classification` is suppressed for the field named `field`.
    pub fn is_suppressed(&self, field: &str, classification: Classification) -> bool {
        self.suppressions
            .get(field)
            .is_some_and(|classes| classes.contains(&classification))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_matches_suppressions() {
        let config: KoyashiConfig =
            toml::from_str("[suppressions]\n\"Telemetry::trace_id\" = [\"derive-only\"]\n")
                .unwrap();
        assert!(config.is_suppressed("Telemetry::trace_id", Classification::DeriveOnly));
        // Other classifications and other fields are not suppressed.
        assert!(!config.is_suppressed("Telemetry::trace_id", Classification::Unused));
        assert!(!config.is_suppressed("Other::field", Classification::DeriveOnly));
    }

    #[test]
    fn empty_config_suppresses_nothing() {
        let config = KoyashiConfig::default();
        assert!(!config.is_suppressed("Any::field", Classification::Unused));
    }

    #[test]
    fn rejects_an_unknown_classification() {
        let result: Result<KoyashiConfig, _> =
            toml::from_str("[suppressions]\n\"X::y\" = [\"bogus\"]\n");
        assert!(result.is_err());
    }

    #[test]
    fn rejects_an_unknown_table() {
        let result: Result<KoyashiConfig, _> = toml::from_str("[supressions]\n");
        assert!(result.is_err());
    }
}
