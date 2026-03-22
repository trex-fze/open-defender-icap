use anyhow::{anyhow, Result};
use policy_dsl::PolicyDocument;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{migrate::Migrator, postgres::PgPoolOptions};
use std::env;
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tracing::info;

static POLICY_MIGRATOR: Migrator = sqlx::migrate!("../../services/policy-engine/migrations");
static ADMIN_MIGRATOR: Migrator = sqlx::migrate!("../../services/admin-api/migrations");

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_target(false).init();

    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".to_string());

    match command.as_str() {
        "health" => run_health().await?,
        "smoke" => run_smoke(args).await?,
        "policy" => run_policy(args).await?,
        "override" => run_override(args).await?,
        "review" => run_review(args).await?,
        "migrate" => run_migrate(args).await?,
        "seed" => run_seed(args).await?,
        "help" | "--help" | "-h" => print_help(),
        other => return Err(anyhow!("unknown command: {other}")),
    }

    Ok(())
}

fn print_help() {
    println!(
        "odctl commands:\n  health                         Run health checks\n  smoke [host:port]              Run ICAP smoke test\n  policy list                    List active policy rules\n  policy reload                  Reload policy definitions\n  policy simulate <file>         Simulate policy decision using JSON payload\n  policy import <file> [name] [created_by]  Create a policy from DSL file\n  policy update <id|current> <file> Update policy metadata/rules via JSON payload\n  override list                  List manual overrides\n  override create <file>         Create override from JSON definition\n  override update <id> <file>    Update override via JSON definition\n  override delete <id>           Delete override by UUID\n  review list                    List manual review queue\n  review resolve <id> <file>     Resolve review item via JSON payload\n  migrate run [target]           Run DB migrations (target = admin|policy|all)\n  seed policies [file] [name] [created_by]  Seed policies via Policy API\n"
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
            if let Some(policy_id) = payload.policy_id {
                println!(
                    "Active policy id: {} (version {})",
                    policy_id, payload.version
                );
            } else {
                println!("Active policy version: {}", payload.version);
            }
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
        "import" => {
            let path = args
                .next()
                .ok_or_else(|| anyhow!("provide policy file to import"))?;
            let name = args.next().unwrap_or_else(|| "imported".to_string());
            let created_by = args.next();
            seed_policies(&path, name, created_by).await
        }
        "update" => {
            let target = args
                .next()
                .ok_or_else(|| anyhow!("provide policy id or 'current'"))?;
            let path = args
                .next()
                .ok_or_else(|| anyhow!("provide JSON file for policy update payload"))?;
            let body = fs::read_to_string(&path).await?;
            let url = format!("{}/api/v1/policies/{}", base.trim_end_matches('/'), target);
            let mut req = client
                .put(&url)
                .header("content-type", "application/json")
                .body(body);
            if let Ok(token) = env::var("OD_ADMIN_TOKEN") {
                req = req.header("X-Admin-Token", token);
            }
            req.send().await?.error_for_status()?;
            println!("Policy {target} updated");
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
            println!(
                "policy subcommands: list | reload | simulate <file> | import <file> [name] [created_by] | update <policy_id|current> <json_file>"
            );
            Ok(())
        }
        other => Err(anyhow!("unknown policy subcommand: {other}")),
    }
}

async fn run_override(mut args: impl Iterator<Item = String>) -> Result<()> {
    let sub = args.next().unwrap_or_else(|| "help".to_string());
    let base = env::var("OD_ADMIN_URL").unwrap_or_else(|_| "http://localhost:19000".to_string());
    let client = Client::new();

    match sub.as_str() {
        "list" => {
            let url = format!("{}/api/v1/overrides", base.trim_end_matches('/'));
            let mut req = client.get(&url);
            if let Ok(token) = env::var("OD_ADMIN_TOKEN") {
                req = req.header("X-Admin-Token", token);
            }
            let resp = req.send().await?.error_for_status()?;
            let payload = resp.json::<Vec<OverrideRecord>>().await?;
            println!(
                "ID                                   Scope          Action    Status      Expires"
            );
            for item in payload {
                println!(
                    "{}  {:<6} {:<14} {:<8} {:<10} {}",
                    item.id,
                    item.scope_type,
                    item.scope_value,
                    item.action,
                    item.status,
                    item.expires_at.clone().unwrap_or_else(|| "-".into())
                );
            }
            Ok(())
        }
        "create" => {
            let path = args
                .next()
                .ok_or_else(|| anyhow!("provide JSON file for override"))?;
            let body = fs::read_to_string(&path).await?;
            let url = format!("{}/api/v1/overrides", base.trim_end_matches('/'));
            let mut req = client
                .post(&url)
                .header("content-type", "application/json")
                .body(body);
            if let Ok(token) = env::var("OD_ADMIN_TOKEN") {
                req = req.header("X-Admin-Token", token);
            }
            let resp = req.send().await?.error_for_status()?;
            let payload = resp.json::<OverrideRecord>().await?;
            println!(
                "Override created: {} -> {}",
                payload.scope_value, payload.action
            );
            Ok(())
        }
        "update" => {
            let id = args.next().ok_or_else(|| anyhow!("provide override id"))?;
            let path = args
                .next()
                .ok_or_else(|| anyhow!("provide JSON file for override update"))?;
            let body = fs::read_to_string(&path).await?;
            let url = format!("{}/api/v1/overrides/{}", base.trim_end_matches('/'), id);
            let mut req = client
                .put(&url)
                .header("content-type", "application/json")
                .body(body);
            if let Ok(token) = env::var("OD_ADMIN_TOKEN") {
                req = req.header("X-Admin-Token", token);
            }
            let resp = req.send().await?.error_for_status()?;
            let payload = resp.json::<OverrideRecord>().await?;
            println!(
                "Override {id} updated: {} -> {}",
                payload.scope_value, payload.action
            );
            Ok(())
        }
        "delete" => {
            let id = args.next().ok_or_else(|| anyhow!("provide override id"))?;
            let url = format!("{}/api/v1/overrides/{}", base.trim_end_matches('/'), id);
            let mut req = client.delete(&url);
            if let Ok(token) = env::var("OD_ADMIN_TOKEN") {
                req = req.header("X-Admin-Token", token);
            }
            req.send().await?.error_for_status()?;
            println!("Override {id} deleted");
            Ok(())
        }
        "help" => {
            println!(
                "override subcommands: list | create <file> | update <id> <file> | delete <id>"
            );
            Ok(())
        }
        other => Err(anyhow!("unknown override subcommand: {other}")),
    }
}

async fn run_review(mut args: impl Iterator<Item = String>) -> Result<()> {
    let sub = args.next().unwrap_or_else(|| "help".to_string());
    let base = env::var("OD_ADMIN_URL").unwrap_or_else(|_| "http://localhost:19000".to_string());
    let client = Client::new();

    match sub.as_str() {
        "list" => {
            let url = format!("{}/api/v1/review-queue", base.trim_end_matches('/'));
            let mut req = client.get(&url);
            if let Ok(token) = env::var("OD_ADMIN_TOKEN") {
                req = req.header("X-Admin-Token", token);
            }
            let resp = req.send().await?.error_for_status()?;
            let payload = resp.json::<Vec<ReviewRecord>>().await?;
            println!(
                "ID                                   Status      Normalized Key                    Submitter       Assigned"
            );
            for item in payload {
                println!(
                    "{}  {:<10} {:<32} {:<14} {:<14}",
                    item.id,
                    item.status,
                    item.normalized_key,
                    item.submitter.unwrap_or_else(|| "-".into()),
                    item.assigned_to.unwrap_or_else(|| "-".into())
                );
            }
            Ok(())
        }
        "resolve" => {
            let id = args.next().ok_or_else(|| anyhow!("provide review id"))?;
            let path = args
                .next()
                .ok_or_else(|| anyhow!("provide JSON file for resolution payload"))?;
            let body = fs::read_to_string(&path).await?;
            let url = format!(
                "{}/api/v1/review-queue/{}/resolve",
                base.trim_end_matches('/'),
                id
            );
            let mut req = client
                .post(&url)
                .header("content-type", "application/json")
                .body(body);
            if let Ok(token) = env::var("OD_ADMIN_TOKEN") {
                req = req.header("X-Admin-Token", token);
            }
            let resp = req.send().await?.error_for_status()?;
            let payload = resp.json::<ReviewRecord>().await?;
            println!(
                "Review {id} resolved with status {} (decision_action: {})",
                payload.status,
                payload
                    .decision_action
                    .clone()
                    .unwrap_or_else(|| "-".into())
            );
            Ok(())
        }
        "help" => {
            println!("review subcommands: list | resolve <id> <file>");
            Ok(())
        }
        other => Err(anyhow!("unknown review subcommand: {other}")),
    }
}

async fn run_migrate(mut args: impl Iterator<Item = String>) -> Result<()> {
    let sub = args.next().unwrap_or_else(|| "help".to_string());
    match sub.as_str() {
        "run" => {
            let target = args.next().unwrap_or_else(|| "all".to_string());
            match target.as_str() {
                "policy" => {
                    migrate_policy().await?;
                    Ok(())
                }
                "admin" => {
                    migrate_admin().await?;
                    Ok(())
                }
                "all" => {
                    migrate_policy().await?;
                    migrate_admin().await?;
                    Ok(())
                }
                other => Err(anyhow!("unknown migrate target: {other}")),
            }
        }
        "help" => {
            println!("migrate usage: odctl migrate run [admin|policy|all]");
            Ok(())
        }
        other => Err(anyhow!("unknown migrate subcommand: {other}")),
    }
}

async fn migrate_policy() -> Result<()> {
    let url = require_env(&["OD_POLICY_DATABASE_URL", "DATABASE_URL"], "policy")?;
    migrate_with("policy", &url, &POLICY_MIGRATOR).await
}

async fn migrate_admin() -> Result<()> {
    let url = require_env(&["OD_ADMIN_DATABASE_URL", "DATABASE_URL"], "admin")?;
    migrate_with("admin", &url, &ADMIN_MIGRATOR).await
}

async fn migrate_with(label: &str, url: &str, migrator: &Migrator) -> Result<()> {
    println!("Running {label} migrations against {url}");
    let pool = PgPoolOptions::new().max_connections(5).connect(url).await?;
    migrator.run(&pool).await?;
    println!("{label} migrations complete");
    Ok(())
}

fn require_env(keys: &[&str], label: &str) -> Result<String> {
    for key in keys {
        if let Ok(value) = env::var(key) {
            if !value.trim().is_empty() {
                return Ok(value);
            }
        }
    }
    Err(anyhow!(
        "missing database url for {label}; set {}",
        keys.join(" or ")
    ))
}

async fn run_seed(mut args: impl Iterator<Item = String>) -> Result<()> {
    let sub = args.next().unwrap_or_else(|| "help".to_string());
    match sub.as_str() {
        "policies" => {
            let file = args
                .next()
                .unwrap_or_else(|| "config/policies.json".to_string());
            let name = args.next().unwrap_or_else(|| "default".to_string());
            let created_by = args.next();
            seed_policies(&file, name, created_by).await
        }
        "help" => {
            println!("seed usage: odctl seed policies [file] [name] [created_by]");
            Ok(())
        }
        other => Err(anyhow!("unknown seed subcommand: {other}")),
    }
}

async fn seed_policies(path: &str, name: String, created_by: Option<String>) -> Result<()> {
    let doc = PolicyDocument::load_from_file(path)?;
    let request = PolicySeedRequest {
        name,
        version: doc.version.clone(),
        created_by,
        rules: doc.rules,
    };

    let base = env::var("OD_POLICY_URL").unwrap_or_else(|_| "http://localhost:19010".to_string());
    let url = format!("{}/api/v1/policies", base.trim_end_matches('/'));
    let client = Client::new();
    let mut req = client
        .post(&url)
        .header("content-type", "application/json")
        .json(&request);
    if let Ok(token) = env::var("OD_ADMIN_TOKEN") {
        req = req.header("X-Admin-Token", token);
    }
    req.send().await?.error_for_status()?;
    println!(
        "Seeded policy '{}' version {} from {}",
        request.name, request.version, path
    );
    Ok(())
}

#[derive(Debug, Deserialize)]
struct PolicyListResponse {
    policy_id: Option<String>,
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

#[derive(Debug, Deserialize)]
struct OverrideRecord {
    id: String,
    scope_type: String,
    scope_value: String,
    action: String,
    reason: Option<String>,
    created_by: Option<String>,
    expires_at: Option<String>,
    status: String,
}

#[derive(Debug, Deserialize)]
struct ReviewRecord {
    id: String,
    normalized_key: String,
    request_metadata: Value,
    status: String,
    submitter: Option<String>,
    assigned_to: Option<String>,
    decided_by: Option<String>,
    decision_notes: Option<String>,
    decision_action: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
struct PolicySeedRequest {
    name: String,
    version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_by: Option<String>,
    rules: Vec<policy_dsl::PolicyRule>,
}
