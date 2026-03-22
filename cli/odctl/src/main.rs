use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use std::env;
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_target(false).init();

    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".to_string());

    match command.as_str() {
        "health" => run_health().await?,
        "smoke" => run_smoke(args).await?,
        "policy" => run_policy(args).await?,
        "help" | "--help" | "-h" => print_help(),
        other => return Err(anyhow!("unknown command: {other}")),
    }

    Ok(())
}

fn print_help() {
    println!(
        "odctl commands:\n  health            Run health checks\n  smoke [host:port] Run ICAP smoke test\n  policy list       List active policy rules\n  policy reload     Reload policy definitions\n  policy simulate <file>  Simulate policy decision using JSON payload\n"
    );
}

async fn run_health() -> Result<()> {
    info!("odctl health placeholder");
    Ok(())
}

async fn run_smoke(mut args: impl Iterator<Item = String>) -> Result<()> {
    let target = args.next().unwrap_or_else(|| "127.0.0.1:1344".to_string());
    let request = "REQMOD icap://icap.service/req ICAP/1.0\r\nHost: icap.service\r\nX-Trace-Id: odctl-smoke\r\nEncapsulated: req-body=0, null-body=0\r\n\r\nGET http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n";

    let mut stream = TcpStream::connect(&target).await?;
    stream.write_all(request.as_bytes()).await?;
    let mut buf = vec![0u8; 1024];
    let n = stream.read(&mut buf).await?;
    let response = String::from_utf8_lossy(&buf[..n]);
    if response.starts_with("ICAP/1.0 204") || response.starts_with("ICAP/1.0 200") {
        println!("Smoke test succeeded against {target}: {response}");
        Ok(())
    } else {
        Err(anyhow!("unexpected ICAP response: {response}"))
    }
}

async fn run_policy(mut args: impl Iterator<Item = String>) -> Result<()> {
    let sub = args.next().unwrap_or_else(|| "help".to_string());
    let base = env::var("OD_POLICY_URL").unwrap_or_else(|_| "http://localhost:19010".to_string());
    let client = Client::new();

    match sub.as_str() {
        "list" => {
            let url = format!("{}/api/v1/policies", base.trim_end_matches('/'));
            let mut req = client.get(&url);
            if let Ok(token) = env::var("OD_ADMIN_TOKEN") {
                req = req.header("X-Admin-Token", token);
            }
            let resp = req.send().await?.error_for_status()?;
            let payload = resp.json::<PolicyListResponse>().await?;
            println!("Active policy version: {}", payload.version);
            println!("Priority  Action     Rule ID           Description");
            for rule in payload.rules {
                println!(
                    "{:>8}  {:<10} {:<18} {}",
                    rule.priority,
                    rule.action,
                    rule.id,
                    rule.description.unwrap_or_default()
                );
            }
            Ok(())
        }
        "reload" => {
            let url = format!("{}/api/v1/policies/reload", base.trim_end_matches('/'));
            let mut req = client.post(&url);
            if let Ok(token) = env::var("OD_ADMIN_TOKEN") {
                req = req.header("X-Admin-Token", token);
            }
            req.send().await?.error_for_status()?;
            println!("Policy reload requested against {base}");
            Ok(())
        }
        "simulate" => {
            let path = args
                .next()
                .ok_or_else(|| anyhow!("provide JSON file path for simulation"))?;
            let body = fs::read_to_string(&path).await?;
            let url = format!("{}/api/v1/policies/simulate", base.trim_end_matches('/'));
            let mut req = client
                .post(&url)
                .header("content-type", "application/json")
                .body(body);
            if let Ok(token) = env::var("OD_ADMIN_TOKEN") {
                req = req.header("X-Admin-Token", token);
            }
            let resp = req.send().await?.error_for_status()?;
            let payload = resp.json::<PolicySimulationResponse>().await?;
            println!(
                "Action: {}, Rule: {:?}, Version: {}",
                payload.decision.action, payload.matched_rule_id, payload.policy_version
            );
            Ok(())
        }
        "help" => {
            println!("policy subcommands: list | reload | simulate <file>");
            Ok(())
        }
        other => Err(anyhow!("unknown policy subcommand: {other}")),
    }
}

#[derive(Debug, Deserialize)]
struct PolicyListResponse {
    version: String,
    rules: Vec<PolicySummary>,
}

#[derive(Debug, Deserialize)]
struct PolicySummary {
    id: String,
    description: Option<String>,
    priority: u32,
    action: String,
}

#[derive(Debug, Deserialize)]
struct PolicySimulationResponse {
    decision: PolicyDecisionData,
    matched_rule_id: Option<String>,
    policy_version: String,
}

#[derive(Debug, Deserialize)]
struct PolicyDecisionData {
    action: String,
    cache_hit: bool,
    verdict: Option<Value>,
}
