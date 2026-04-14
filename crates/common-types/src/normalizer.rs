use addr::parse_domain_name;
use anyhow::{anyhow, Context, Result};
use idna::domain_to_ascii;
use std::collections::{HashMap, HashSet};
use url::Url;

use crate::{EntityLevel, NormalizedTarget};

/// Normalize host/path data to produce a consistent `NormalizedTarget`.
pub fn normalize_target(host: &str, path: &str, scheme: Option<&str>) -> Result<NormalizedTarget> {
    let scheme = scheme.unwrap_or("http");
    if host.trim().is_empty() {
        return Err(anyhow!("host required for normalization"));
    }

    let host_no_port = sanitize_host(host);

    let ascii_host = domain_to_ascii(host_no_port.trim())
        .map_err(|err| anyhow!("invalid host {host}: {err:?}"))?
        .to_lowercase();

    let normalized_path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{}", path)
    };

    let host_for_url = if ascii_host.contains(':') {
        format!("[{}]", ascii_host)
    } else {
        ascii_host.clone()
    };

    let full_url = format!("{}://{}{}", scheme, host_for_url, normalized_path);
    Url::parse(&full_url).context("failed to parse normalized url")?;
    let hostname = ascii_host.clone();

    let registered_domain = derive_registered_domain(&hostname);
    let entity_level = if hostname != registered_domain {
        EntityLevel::Subdomain
    } else {
        EntityLevel::Domain
    };

    let prefix = match entity_level {
        EntityLevel::Domain => "domain",
        EntityLevel::Subdomain => "subdomain",
        EntityLevel::Url => "url",
        EntityLevel::Page => "page",
    };
    let normalized_key = format!("{}:{}", prefix, hostname);

    Ok(NormalizedTarget {
        entity_level,
        normalized_key,
        hostname,
        registered_domain,
        full_url,
    })
}

pub fn derive_registered_domain(hostname: &str) -> String {
    if let Ok(parsed) = parse_domain_name(hostname) {
        if let Some(root) = parsed.root() {
            return root.to_ascii_lowercase();
        }
    }

    let labels: Vec<&str> = hostname.split('.').collect();
    if labels.len() <= 2 {
        hostname.to_string()
    } else {
        labels[labels.len() - 2..].join(".")
    }
}

pub fn canonical_classification_key(normalized_key: &str) -> Option<String> {
    let host = normalized_key
        .strip_prefix("domain:")
        .or_else(|| normalized_key.strip_prefix("subdomain:"))?
        .trim()
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    let registered = derive_registered_domain(&host);
    Some(format!("domain:{}", registered))
}

#[derive(Debug, Clone, Default)]
pub struct CanonicalizationPolicy {
    tenant_domain_exceptions: HashMap<String, HashSet<String>>,
}

impl CanonicalizationPolicy {
    pub fn from_tenant_exceptions(input: HashMap<String, Vec<String>>) -> Self {
        let mut tenant_domain_exceptions: HashMap<String, HashSet<String>> = HashMap::new();
        for (tenant, domains) in input {
            let key = normalize_tenant_key(&tenant);
            let set = tenant_domain_exceptions.entry(key).or_default();
            for domain in domains {
                let normalized = domain.trim().trim_end_matches('.').to_ascii_lowercase();
                if normalized.is_empty() {
                    continue;
                }
                set.insert(derive_registered_domain(&normalized));
            }
        }
        Self {
            tenant_domain_exceptions,
        }
    }

    pub fn keeps_subdomain_granularity(&self, tenant: Option<&str>, host: &str) -> bool {
        if self.tenant_domain_exceptions.is_empty() {
            return false;
        }
        let registered = derive_registered_domain(host);
        let registered = registered.trim().trim_end_matches('.').to_ascii_lowercase();
        if registered.is_empty() {
            return false;
        }
        let tenant_key = tenant.map(normalize_tenant_key);
        let matches = |key: &str| {
            self.tenant_domain_exceptions
                .get(key)
                .map(|set| set.contains(&registered))
                .unwrap_or(false)
        };

        tenant_key.as_deref().map(matches).unwrap_or(false) || matches("*") || matches("default")
    }
}

pub fn canonical_classification_key_with_policy(
    normalized_key: &str,
    policy: &CanonicalizationPolicy,
    tenant: Option<&str>,
) -> Option<String> {
    let host = normalized_key
        .strip_prefix("domain:")
        .or_else(|| normalized_key.strip_prefix("subdomain:"))?
        .trim()
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    if policy.keeps_subdomain_granularity(tenant, &host) {
        return Some(format!("domain:{}", host));
    }
    let registered = derive_registered_domain(&host);
    Some(format!("domain:{}", registered))
}

fn normalize_tenant_key(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

fn sanitize_host(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.starts_with('[') {
        if let Some(end) = trimmed.find(']') {
            return trimmed[1..end].to_string();
        }
    }
    if let Some((host_part, port_part)) = trimmed.rsplit_once(':') {
        if port_part.chars().all(|c| c.is_ascii_digit()) {
            return host_part.to_string();
        }
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_basic_domain() {
        let result = normalize_target("Example.COM", "/path", Some("https")).unwrap();
        assert_eq!(result.hostname, "example.com");
        assert_eq!(result.entity_level, EntityLevel::Domain);
        assert_eq!(result.normalized_key, "domain:example.com");
        assert_eq!(result.full_url, "https://example.com/path");
    }

    #[test]
    fn normalizes_punycode() {
        let result = normalize_target("bücher.de", "", None).unwrap();
        assert_eq!(result.hostname, "xn--bcher-kva.de");
    }

    #[test]
    fn subdomain_detection() {
        let result = normalize_target("app.service.example.com", "/", None).unwrap();
        assert_eq!(result.entity_level, EntityLevel::Subdomain);
        assert_eq!(result.registered_domain, "example.com");
    }

    #[test]
    fn missing_host_errors() {
        let err = normalize_target("   ", "/", None).unwrap_err();
        assert!(err.to_string().contains("host"));
    }

    #[test]
    fn canonical_key_promotes_subdomain_to_domain() {
        assert_eq!(
            canonical_classification_key("subdomain:www.example.com").as_deref(),
            Some("domain:example.com")
        );
        assert_eq!(
            canonical_classification_key("domain:example.com").as_deref(),
            Some("domain:example.com")
        );
        assert!(canonical_classification_key("url:https://example.com").is_none());
    }

    #[test]
    fn strips_port_from_host() {
        let result =
            normalize_target("Example.com:443", "/", Some("https")).expect("normalize with port");
        assert_eq!(result.hostname, "example.com");
    }

    #[test]
    fn strips_port_from_ipv6_with_brackets() {
        let result = normalize_target("[2001:db8::1]:443", "/", Some("https"))
            .expect("normalize ipv6 with port");
        assert_eq!(result.hostname, "2001:db8::1");
    }

    #[test]
    fn derives_registered_domain_with_public_suffixes() {
        assert_eq!(
            derive_registered_domain("api.service.example.co.uk"),
            "example.co.uk"
        );
        assert_eq!(
            derive_registered_domain("portal.example.com.au"),
            "example.com.au"
        );
    }

    #[test]
    fn canonicalization_policy_keeps_granularity_for_configured_tenant_domain() {
        let policy = CanonicalizationPolicy::from_tenant_exceptions(HashMap::from([
            (
                "tenant-acme".to_string(),
                vec!["example.co.uk".to_string(), "example.com".to_string()],
            ),
            ("*".to_string(), vec!["global.example".to_string()]),
        ]));

        assert_eq!(
            canonical_classification_key_with_policy(
                "subdomain:api.example.co.uk",
                &policy,
                Some("tenant-acme")
            )
            .as_deref(),
            Some("domain:api.example.co.uk")
        );
        assert_eq!(
            canonical_classification_key_with_policy(
                "subdomain:cdn.global.example",
                &policy,
                Some("tenant-foo")
            )
            .as_deref(),
            Some("domain:cdn.global.example")
        );
        assert_eq!(
            canonical_classification_key_with_policy(
                "subdomain:api.other.co.uk",
                &policy,
                Some("tenant-acme")
            )
            .as_deref(),
            Some("domain:other.co.uk")
        );
    }
}
