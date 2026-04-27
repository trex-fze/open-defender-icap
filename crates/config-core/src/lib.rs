use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use std::{env, fs, path::Path};
use url::Url;

const INSECURE_DEV_MODE_ENV: &str = "OD_ALLOW_INSECURE_DEV_SECRETS";
const DEFAULT_SECRET_MIN_LEN: usize = 12;
const COMMON_BLOCKED_EXACT: &[&str] = &[
    "changeme",
    "defender",
    "password",
    "secret",
    "admin",
    "test",
    "default",
    "example",
    "sample",
    "placeholder",
    "insecure",
    "dummy",
];
const COMMON_BLOCKED_TOKENS: &[&str] = &["defender"];
const KNOWN_EXAMPLE_VALUES: &[&str] = &[
    "changeme-admin",
    "changeme-ingest",
    "changeme-elastic",
    "changeme-local-admin-password",
    "changeme-local-jwt-secret",
    "changeme-local-jwt-secret-min-32-chars",
    "defender",
    "AAEAAWVsYXN0aWMva2liYW5hL29kLXN0YWNrOk1MUUJvVnlnU19TSmNDT0Z6OVZMeGc",
];

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
    warnings: Vec<ConfigIssue>,
}

impl ConfigValidator {
    pub fn new(scope: impl Into<String>) -> Self {
        Self {
            scope: scope.into(),
            issues: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn insecure_dev_mode_enabled(&self) -> bool {
        insecure_dev_mode_enabled()
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
        self.warnings.extend(other.warnings);
    }

    pub fn require_strong_secret(
        &mut self,
        key: &str,
        value: Option<&str>,
        min_len: usize,
        remediation: &str,
    ) {
        let findings = secret_findings(key, value, min_len.max(DEFAULT_SECRET_MIN_LEN), &[], true);
        self.push_secret_findings(findings, remediation);
    }

    pub fn require_strong_secret_with_blocklist(
        &mut self,
        key: &str,
        value: Option<&str>,
        min_len: usize,
        blocked_values: &[&str],
        remediation: &str,
    ) {
        let findings = secret_findings(
            key,
            value,
            min_len.max(DEFAULT_SECRET_MIN_LEN),
            blocked_values,
            true,
        );
        self.push_secret_findings(findings, remediation);
    }

    pub fn validate_optional_secret(
        &mut self,
        key: &str,
        value: Option<&str>,
        min_len: usize,
        remediation: &str,
    ) {
        let findings = secret_findings(key, value, min_len.max(DEFAULT_SECRET_MIN_LEN), &[], false);
        self.push_secret_findings(findings, remediation);
    }

    pub fn validate_optional_secret_with_blocklist(
        &mut self,
        key: &str,
        value: Option<&str>,
        min_len: usize,
        blocked_values: &[&str],
        remediation: &str,
    ) {
        let findings = secret_findings(
            key,
            value,
            min_len.max(DEFAULT_SECRET_MIN_LEN),
            blocked_values,
            false,
        );
        self.push_secret_findings(findings, remediation);
    }

    pub fn require_auth_url(
        &mut self,
        key: &str,
        value: Option<&str>,
        require_username: bool,
        require_password: bool,
        min_password_len: usize,
        remediation: &str,
    ) {
        let mut local = Vec::new();
        let Some(raw) = value.map(str::trim).filter(|v| !v.is_empty()) else {
            local.push(ConfigIssue {
                key: key.to_string(),
                problem: "missing required value".to_string(),
                remediation: remediation.to_string(),
            });
            self.push_secret_findings(local, remediation);
            return;
        };

        let parsed = match Url::parse(raw) {
            Ok(url) => url,
            Err(_) => {
                local.push(ConfigIssue {
                    key: key.to_string(),
                    problem: "invalid URL format".to_string(),
                    remediation: remediation.to_string(),
                });
                self.push_secret_findings(local, remediation);
                return;
            }
        };

        let username = parsed.username().trim();
        let password = parsed.password().map(str::trim);
        if require_username && username.is_empty() {
            local.push(ConfigIssue {
                key: key.to_string(),
                problem: "URL is missing username credentials".to_string(),
                remediation: remediation.to_string(),
            });
        }
        if require_password && password.unwrap_or("").is_empty() {
            local.push(ConfigIssue {
                key: key.to_string(),
                problem: "URL is missing password credentials".to_string(),
                remediation: remediation.to_string(),
            });
        }
        if !username.is_empty() {
            local.extend(secret_findings(key, Some(username), 3, &[], false));
        }
        if let Some(pass) = password {
            local.extend(secret_findings(
                key,
                Some(pass),
                min_password_len.max(DEFAULT_SECRET_MIN_LEN),
                &[],
                false,
            ));
        }

        self.push_secret_findings(local, remediation);
    }

    pub fn finish(self) -> Result<()> {
        if !self.warnings.is_empty() {
            eprintln!(
                "warning: {} insecure development mode enabled via {}. {} unsafe secret value(s) were accepted:",
                self.scope,
                INSECURE_DEV_MODE_ENV,
                self.warnings.len()
            );
            for warning in &self.warnings {
                eprintln!(
                    "- {}: {}. remediation: {}",
                    warning.key, warning.problem, warning.remediation
                );
            }
        }

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

    fn push_secret_findings(&mut self, findings: Vec<ConfigIssue>, remediation: &str) {
        if findings.is_empty() {
            return;
        }
        if self.insecure_dev_mode_enabled() {
            for finding in findings {
                self.warnings.push(ConfigIssue {
                    key: finding.key,
                    problem: finding.problem,
                    remediation: remediation.to_string(),
                });
            }
            return;
        }
        self.issues.extend(findings);
    }
}

pub fn insecure_dev_mode_enabled() -> bool {
    parse_bool_env(INSECURE_DEV_MODE_ENV).unwrap_or(false)
}

fn parse_bool_env(key: &str) -> Option<bool> {
    let raw = env::var(key).ok()?;
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn secret_findings(
    key: &str,
    value: Option<&str>,
    min_len: usize,
    blocked_values: &[&str],
    required: bool,
) -> Vec<ConfigIssue> {
    let mut findings = Vec::new();
    let candidate = value.map(str::trim);
    if candidate.map(|v| v.is_empty()).unwrap_or(true) {
        if required {
            findings.push(ConfigIssue {
                key: key.to_string(),
                problem: "missing required value".to_string(),
                remediation: "set a unique strong secret".to_string(),
            });
        }
        return findings;
    }

    let raw = candidate.unwrap_or_default();
    if raw.len() < min_len {
        findings.push(ConfigIssue {
            key: key.to_string(),
            problem: format!("value shorter than minimum length ({min_len})"),
            remediation: "use a longer random secret".to_string(),
        });
    }

    let lowered = raw.to_ascii_lowercase();
    if KNOWN_EXAMPLE_VALUES
        .iter()
        .chain(blocked_values.iter())
        .any(|blocked| lowered == blocked.to_ascii_lowercase())
    {
        findings.push(ConfigIssue {
            key: key.to_string(),
            problem: "value matches repository example/default secret".to_string(),
            remediation: "replace with deployment-specific secret".to_string(),
        });
    }

    if contains_weak_pattern(&lowered) {
        findings.push(ConfigIssue {
            key: key.to_string(),
            problem: "value matches blocked default/test pattern".to_string(),
            remediation: "replace with high-entropy secret".to_string(),
        });
    }

    findings
}

fn contains_weak_pattern(lowered: &str) -> bool {
    if lowered.contains("changeme") {
        return true;
    }
    if COMMON_BLOCKED_EXACT
        .iter()
        .any(|pattern| lowered == *pattern)
    {
        return true;
    }

    lowered
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(|token| {
            !token.is_empty()
                && COMMON_BLOCKED_TOKENS
                    .iter()
                    .any(|pattern| token.eq_ignore_ascii_case(pattern))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn strong_secret_fails_when_default_used_without_dev_mode() {
        let _guard = env_lock().lock().expect("env lock");
        std::env::remove_var(INSECURE_DEV_MODE_ENV);
        let mut validator = ConfigValidator::new("unit-test");
        validator.require_strong_secret("OD_ADMIN_TOKEN", Some("changeme-admin"), 16, "rotate");
        let err = validator
            .finish()
            .expect_err("must fail with insecure default");
        let rendered = format!("{err:#}");
        assert!(rendered.contains("OD_ADMIN_TOKEN"));
    }

    #[test]
    fn strong_secret_passes_with_valid_value_without_dev_mode() {
        let _guard = env_lock().lock().expect("env lock");
        std::env::remove_var(INSECURE_DEV_MODE_ENV);
        let mut validator = ConfigValidator::new("unit-test");
        validator.require_strong_secret(
            "OD_ADMIN_TOKEN",
            Some("prod-token-01a5a7ca4f9c2d8e"),
            16,
            "rotate",
        );
        assert!(validator.finish().is_ok());
    }

    #[test]
    fn insecure_dev_mode_allows_insecure_with_warning() {
        let _guard = env_lock().lock().expect("env lock");
        std::env::set_var(INSECURE_DEV_MODE_ENV, "true");
        let mut validator = ConfigValidator::new("unit-test");
        validator.require_strong_secret(
            "OD_FILEBEAT_SECRET",
            Some("changeme-ingest"),
            16,
            "rotate",
        );
        assert!(validator.finish().is_ok());
        std::env::remove_var(INSECURE_DEV_MODE_ENV);
    }

    #[test]
    fn auth_url_requires_password() {
        let _guard = env_lock().lock().expect("env lock");
        std::env::remove_var(INSECURE_DEV_MODE_ENV);
        let mut validator = ConfigValidator::new("unit-test");
        validator.require_auth_url(
            "OD_PAGE_FETCH_REDIS_URL",
            Some("redis://redis:6379"),
            false,
            true,
            16,
            "set password-authenticated redis URL",
        );
        let err = validator
            .finish()
            .expect_err("redis without password must fail");
        assert!(format!("{err:#}").contains("OD_PAGE_FETCH_REDIS_URL"));
    }
}
