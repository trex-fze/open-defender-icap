use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use std::{env, fs, path::Path};

#[derive(Debug, Clone)]
pub struct EnvLookup {
    pub value: Option<String>,
    pub source_key: Option<String>,
    pub deprecated_alias: Option<String>,
}

pub fn load_config<T>(path: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    let file = if Path::new(path).exists() {
        Some(fs::read_to_string(path).with_context(|| format!("failed to read config {path}"))?)
    } else {
        None
    };

    if let Some(contents) = file {
        let cfg = serde_json::from_str(&contents)
            .with_context(|| format!("failed to parse config {path} as JSON"))?;
        Ok(cfg)
    } else {
        let env_json = env::var("OD_CONFIG_JSON").unwrap_or_else(|_| "{}".to_string());
        let cfg = serde_json::from_str(&env_json).context("failed to parse OD_CONFIG_JSON")?;
        Ok(cfg)
    }
}

pub fn lookup_env(canonical: &str, deprecated_aliases: &[&str]) -> EnvLookup {
    if let Ok(value) = env::var(canonical) {
        if !value.trim().is_empty() {
            return EnvLookup {
                value: Some(value),
                source_key: Some(canonical.to_string()),
                deprecated_alias: None,
            };
        }
    }

    for alias in deprecated_aliases {
        if let Ok(value) = env::var(alias) {
            if !value.trim().is_empty() {
                return EnvLookup {
                    value: Some(value),
                    source_key: Some((*alias).to_string()),
                    deprecated_alias: Some((*alias).to_string()),
                };
            }
        }
    }

    EnvLookup {
        value: None,
        source_key: None,
        deprecated_alias: None,
    }
}

#[derive(Debug, Clone)]
pub struct ConfigIssue {
    pub key: String,
    pub problem: String,
    pub remediation: String,
}

#[derive(Debug, Clone)]
pub struct ConfigValidator {
    scope: String,
    issues: Vec<ConfigIssue>,
}

impl ConfigValidator {
    pub fn new(scope: impl Into<String>) -> Self {
        Self {
            scope: scope.into(),
            issues: Vec::new(),
        }
    }

    pub fn require_non_empty(&mut self, key: &str, value: Option<&str>, remediation: &str) {
        let missing = value.map(|v| v.trim().is_empty()).unwrap_or(true);
        if missing {
            self.issues.push(ConfigIssue {
                key: key.to_string(),
                problem: "missing required value".to_string(),
                remediation: remediation.to_string(),
            });
        }
    }

    pub fn require_min_len(
        &mut self,
        key: &str,
        value: Option<&str>,
        min: usize,
        remediation: &str,
    ) {
        if let Some(raw) = value {
            if raw.trim().len() < min {
                self.issues.push(ConfigIssue {
                    key: key.to_string(),
                    problem: format!("value shorter than minimum length ({min})"),
                    remediation: remediation.to_string(),
                });
            }
        }
    }

    pub fn forbid_substrings_ci(
        &mut self,
        key: &str,
        value: Option<&str>,
        blocked: &[&str],
        remediation: &str,
    ) {
        let Some(raw) = value else {
            return;
        };
        let lowered = raw.to_ascii_lowercase();
        if blocked
            .iter()
            .any(|needle| lowered.contains(&needle.to_ascii_lowercase()))
        {
            self.issues.push(ConfigIssue {
                key: key.to_string(),
                problem: "value matches blocked default/test pattern".to_string(),
                remediation: remediation.to_string(),
            });
        }
    }

    pub fn extend(&mut self, other: Self) {
        self.issues.extend(other.issues);
    }

    pub fn finish(self) -> Result<()> {
        if self.issues.is_empty() {
            return Ok(());
        }

        let mut rendered = format!(
            "{} config validation failed with {} issue(s):",
            self.scope,
            self.issues.len()
        );
        for issue in self.issues {
            rendered.push_str(&format!(
                "\n- {}: {}. remediation: {}",
                issue.key, issue.problem, issue.remediation
            ));
        }
        Err(anyhow::anyhow!(rendered))
    }
}
