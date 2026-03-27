use anyhow::{anyhow, Context, Result};
use idna::domain_to_ascii;
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

fn derive_registered_domain(hostname: &str) -> String {
    let labels: Vec<&str> = hostname.split('.').collect();
    if labels.len() <= 2 {
        hostname.to_string()
    } else {
        labels[labels.len() - 2..].join(".")
    }
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
}
