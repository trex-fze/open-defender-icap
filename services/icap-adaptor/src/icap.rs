use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct IcapRequest {
    pub method: String,
    pub service: String,
    pub headers: HashMap<String, String>,
    pub http_method: String,
    pub http_path: String,
    pub http_scheme: Option<String>,
    pub http_host: String,
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
                headers.insert(name.trim().to_string(), value.trim().to_string());
            }
        }

        let (http_method, http_path, http_scheme, http_host) = parse_http_block(http_block)?;

        Ok(Self {
            method,
            service,
            headers,
            http_method,
            http_path,
            http_scheme,
            http_host,
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

    if target.starts_with("http://") || target.starts_with("https://") {
        let url =
            url::Url::parse(target).context("failed to parse absolute URL in HTTP request")?;
        http_scheme = Some(url.scheme().to_string());
        http_host = url
            .host_str()
            .ok_or_else(|| anyhow!("absolute request missing host"))?
            .to_lowercase();
        http_path = url.path().to_string();
    }

    for line in lines {
        if line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("host") {
                http_host = value.trim().to_lowercase();
                break;
            }
        }
    }

    if http_host.is_empty() {
        return Err(anyhow!("HTTP host header missing"));
    }

    Ok((method, http_path, http_scheme, http_host))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "REQMOD icap://icap.service/req ICAP/1.0\r\nHost: icap.service\r\n\r\nGET http://Example.com/path HTTP/1.1\r\nHost: Example.com\r\n\r\n";

    #[test]
    fn parses_basic_icap_request() {
        let req = IcapRequest::parse(SAMPLE).unwrap();
        assert_eq!(req.method, "REQMOD");
        assert_eq!(req.http_method, "GET");
        assert_eq!(req.http_host, "example.com");
        assert_eq!(req.http_path, "/path");
        assert_eq!(req.http_scheme.as_deref(), Some("http"));
    }

    #[test]
    fn parses_relative_target() {
        let raw = "REQMOD icap://icap/req ICAP/1.0\r\nHost: icap\r\n\r\nGET /foo/bar HTTP/1.1\r\nHost: sub.example.com\r\n\r\n";
        let req = IcapRequest::parse(raw).unwrap();
        assert_eq!(req.http_path, "/foo/bar");
        assert_eq!(req.http_host, "sub.example.com");
        assert!(req.http_scheme.is_none());
    }

    #[test]
    fn missing_host_errors() {
        let raw = "REQMOD icap://icap/req ICAP/1.0\r\nHost: icap\r\n\r\nGET /foo HTTP/1.1\r\n\r\n";
        let err = IcapRequest::parse(raw).unwrap_err();
        assert!(err.to_string().contains("HTTP host"));
    }
}
