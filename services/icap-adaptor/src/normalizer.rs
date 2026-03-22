use anyhow::{anyhow, Context, Result};
use common_types::{EntityLevel, NormalizedTarget};
use idna::domain_to_ascii;
use url::Url;

/// Normalize host/path data coming from Squid/ICAP metadata.
pub fn normalize_target(host: &str, path: &str, scheme: Option<&str>) -> Result<NormalizedTarget> {
    let scheme = scheme.unwrap_or("http");
    if host.trim().is_empty() {
        return Err(anyhow!("host required for normalization"));
    }

    let ascii_host = domain_to_ascii(host.trim())
        .map_err(|err| anyhow!("invalid host {host}: {err:?}"))?
        .to_lowercase();

    let normalized_path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{}", path)
    };

    let full_url = format!("{}://{}{}", scheme, ascii_host, normalized_path);
    let parsed = Url::parse(&full_url).context("failed to parse normalized url")?;
    let hostname = parsed
        .host_str()
        .ok_or_else(|| anyhow!("normalized url missing host"))?
        .to_string();

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
}
