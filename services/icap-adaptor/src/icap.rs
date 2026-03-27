use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct IcapRequest {
    pub method: String,
    pub service: String,
    pub headers: HashMap<String, String>,
    pub http_method: Option<String>,
    pub http_path: Option<String>,
    pub http_scheme: Option<String>,
    pub http_host: Option<String>,
    pub trace_id: Option<String>,
}

impl IcapRequest {
    pub fn parse(raw: &str) -> Result<Self> {
        let mut sections = raw.split("\r\n\r\n");
        let icap_head = sections
            .next()
            .ok_or_else(|| anyhow!("icap message missing header"))?;
        let http_block = sections.next().unwrap_or("");

        let mut icap_lines = icap_head.lines();
        let start_line = icap_lines
            .next()
            .ok_or_else(|| anyhow!("icap request missing start line"))?;
        let mut start_parts = start_line.split_whitespace();
        let method = start_parts
            .next()
            .ok_or_else(|| anyhow!("icap request missing method"))?
            .to_string();
        let service = start_parts
            .next()
            .ok_or_else(|| anyhow!("icap request missing service"))?
            .to_string();

        let mut headers = HashMap::new();
        for line in icap_lines {
            if line.trim().is_empty() {
                continue;
            }
            if let Some((name, value)) = line.split_once(':') {
                headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
            }
        }

        let trace_id = headers.get("x-trace-id").map(|value| value.to_string());

        if method.eq_ignore_ascii_case("OPTIONS") {
            return Ok(Self {
                method,
                service,
                headers,
                http_method: None,
                http_path: None,
                http_scheme: None,
                http_host: None,
                trace_id,
            });
        }

        let (http_method, http_path, http_scheme, http_host) = parse_http_block(http_block)?;

        Ok(Self {
            method,
            service,
            headers,
            http_method: Some(http_method),
            http_path: Some(http_path),
            http_scheme,
            http_host: Some(http_host),
            trace_id,
        })
    }
}

fn parse_http_block(block: &str) -> Result<(String, String, Option<String>, String)> {
    let mut lines = block.lines();
    let request_line = lines
        .next()
        .ok_or_else(|| anyhow!("encapsulated HTTP request missing start line"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| anyhow!("http request missing method"))?
        .to_string();
    let target = parts
        .next()
        .ok_or_else(|| anyhow!("http request missing target"))?;

    let mut http_scheme = None;
    let mut http_host = String::new();
    let mut http_path = target.to_string();

    if method.eq_ignore_ascii_case("CONNECT") {
        http_scheme = Some("https".to_string());
        http_host = clean_host_value(target);
        http_path = "/".to_string();
    } else if target.starts_with("http://") || target.starts_with("https://") {
        let url =
            url::Url::parse(target).context("failed to parse absolute URL in HTTP request")?;
        http_scheme = Some(url.scheme().to_string());
        http_host = url
            .host_str()
            .ok_or_else(|| anyhow!("absolute request missing host"))?
            .to_lowercase();
        http_path = url.path().to_string();
    }

    let mut host_header: Option<String> = None;
    for line in lines {
        if line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("host") {
                host_header = Some(value.trim().to_string());
                break;
            }
        }
    }

    if let Some(host_value) = host_header {
        if http_host.is_empty() || method.eq_ignore_ascii_case("CONNECT") {
            http_host = clean_host_value(&host_value);
        }
    }

    if http_host.is_empty() {
        return Err(anyhow!("HTTP host header missing"));
    }

    Ok((method, http_path, http_scheme, http_host))
}

fn clean_host_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.starts_with('[') {
        if let Some(end) = trimmed.find(']') {
            return trimmed[1..end].to_lowercase();
        }
    }
    if let Some((host_part, port_part)) = trimmed.rsplit_once(':') {
        if port_part.chars().all(|c| c.is_ascii_digit()) {
            return host_part.to_lowercase();
        }
    }
    trimmed.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "REQMOD icap://icap.service/req ICAP/1.0\r\nHost: icap.service\r\nX-Trace-Id: abc123\r\n\r\nGET http://Example.com/path HTTP/1.1\r\nHost: Example.com\r\n\r\n";

    #[test]
    fn parses_basic_icap_request() {
        let req = IcapRequest::parse(SAMPLE).unwrap();
        assert_eq!(req.method, "REQMOD");
        assert_eq!(req.http_method.as_deref(), Some("GET"));
        assert_eq!(req.http_host.as_deref(), Some("example.com"));
        assert_eq!(req.http_path.as_deref(), Some("/path"));
        assert_eq!(req.http_scheme.as_deref(), Some("http"));
        assert_eq!(req.trace_id.as_deref(), Some("abc123"));
    }

    #[test]
    fn parses_relative_target() {
        let raw = "REQMOD icap://icap/req ICAP/1.0\r\nHost: icap\r\n\r\nGET /foo/bar HTTP/1.1\r\nHost: sub.example.com\r\n\r\n";
        let req = IcapRequest::parse(raw).unwrap();
        assert_eq!(req.http_path.as_deref(), Some("/foo/bar"));
        assert_eq!(req.http_host.as_deref(), Some("sub.example.com"));
        assert!(req.http_scheme.is_none());
    }

    #[test]
    fn missing_host_errors() {
        let raw = "REQMOD icap://icap/req ICAP/1.0\r\nHost: icap\r\n\r\nGET /foo HTTP/1.1\r\n\r\n";
        let err = IcapRequest::parse(raw).unwrap_err();
        assert!(err.to_string().contains("HTTP host"));
    }

    #[test]
    fn parses_connect_with_port() {
        let raw = "REQMOD icap://icap/req ICAP/1.0\r\nHost: icap\r\n\r\nCONNECT Facebook.com:443 HTTP/1.1\r\nHost: Facebook.com:443\r\n\r\n";
        let req = IcapRequest::parse(raw).unwrap();
        assert_eq!(req.http_method.as_deref(), Some("CONNECT"));
        assert_eq!(req.http_host.as_deref(), Some("facebook.com"));
        assert_eq!(req.http_scheme.as_deref(), Some("https"));
        assert_eq!(req.http_path.as_deref(), Some("/"));
    }

    #[test]
    fn parses_connect_with_ipv6_host() {
        let raw = "REQMOD icap://icap/req ICAP/1.0\r\nHost: icap\r\n\r\nCONNECT [2001:db8::1]:443 HTTP/1.1\r\nHost: [2001:db8::1]:443\r\n\r\n";
        let req = IcapRequest::parse(raw).unwrap();
        assert_eq!(req.http_host.as_deref(), Some("2001:db8::1"));
        assert_eq!(req.http_scheme.as_deref(), Some("https"));
        assert_eq!(req.http_path.as_deref(), Some("/"));
    }

    #[test]
    fn parses_options_without_http_block() {
        let raw = "OPTIONS icap://icap.service/req ICAP/1.0\r\nHost: icap.service\r\nUser-Agent: Squid\r\n\r\n";
        let req = IcapRequest::parse(raw).unwrap();
        assert_eq!(req.method, "OPTIONS");
        assert_eq!(req.service, "icap://icap.service/req");
        assert!(req.http_host.is_none());
        assert!(req.http_path.is_none());
        assert!(req.http_method.is_none());
    }
}
