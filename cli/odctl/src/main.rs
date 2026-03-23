use std::{fs, path::PathBuf, time::Duration};

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use common_types::PolicyAction;
use policy_dsl::{Conditions as RuleConditions, PolicyDocument, PolicyRule as DslPolicyRule};
use reqwest::Client;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tracing::info;
use uuid::Uuid;

#[derive(Parser, Debug)]
#[clap(author, version, about = "Open Defender administrative CLI")]
struct Cli {
    /// Override Admin API base URL or set OD_ADMIN_URL
    #[clap(
        long,
        global = true,
        env = "OD_ADMIN_URL",
        default_value = "http://localhost:19000"
    )]
    base_url: String,

    /// Provide admin token (or set OD_ADMIN_TOKEN)
    #[clap(long, global = true, env = "OD_ADMIN_TOKEN")]
    token: Option<String>,

    /// Emit machine-readable JSON output
    #[clap(long, global = true)]
    json: bool,

    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[clap(subcommand)]
    Auth(AuthCmd),
    Env {
        #[clap(long)]
        url: Option<String>,
    },
    #[clap(subcommand)]
    Policy(PolicyCmd),
    #[clap(subcommand)]
    Override(OverrideCmd),
    #[clap(subcommand)]
    Review(ReviewCmd),
    #[clap(subcommand)]
    Cache(CacheCmd),
    #[clap(subcommand)]
    Report(ReportCmd),
    #[clap(subcommand)]
    Logs(LogsCmd),
    Smoke {
        #[clap(long, default_value = "staging")]
        profile: String,
    },
}

#[derive(Subcommand, Debug)]
enum AuthCmd {
    Login,
    Logout,
    Status,
}

#[derive(Subcommand, Debug)]
enum PolicyCmd {
    List,
    Show {
        id: String,
    },
    Pull {
        id: String,
        #[clap(long)]
        file: PathBuf,
    },
    Push {
        id: String,
        #[clap(long)]
        file: PathBuf,
        #[clap(long)]
        notes: Option<String>,
    },
    Publish {
        id: String,
        #[clap(long)]
        version: Option<String>,
    },
    Validate {
        #[clap(long)]
        file: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
enum OverrideCmd {
    List,
    Create {
        #[clap(long, help = "Scope in the form type:value (domain:user.example)")]
        scope: String,
        #[clap(long)]
        action: String,
        #[clap(long)]
        reason: Option<String>,
        #[clap(long, help = "ISO-8601 timestamp, e.g. 2025-03-01T00:00:00Z")]
        expires_at: Option<String>,
        #[clap(long)]
        status: Option<String>,
    },
    Update {
        id: String,
        #[clap(long, help = "Scope in the form type:value (domain:user.example)")]
        scope: String,
        #[clap(long)]
        action: String,
        #[clap(long)]
        reason: Option<String>,
        #[clap(long)]
        expires_at: Option<String>,
        #[clap(long)]
        status: Option<String>,
    },
    Delete {
        id: String,
    },
}

#[derive(Subcommand, Debug)]
enum ReviewCmd {
    Queue,
    Resolve {
        id: String,
        #[clap(long, default_value = "resolved")]
        status: String,
        #[clap(long)]
        decision_action: Option<String>,
        #[clap(long)]
        notes: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum CacheCmd {
    Get { key: String },
    Delete { key: String },
}

#[derive(Subcommand, Debug)]
enum ReportCmd {
    Summary {
        #[clap(long, default_value = "category")]
        dimension: String,
    },
}

#[derive(Subcommand, Debug)]
enum LogsCmd {
    Cli {
        #[clap(long)]
        operator: Option<String>,
        #[clap(long, default_value_t = 50)]
        limit: u32,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_target(false).init();
    let cli = Cli::parse();
    let client = ApiClient::new(&cli.base_url, cli.token.clone())?;

    match &cli.command {
        Commands::Auth(cmd) => handle_auth(cmd).await?,
        Commands::Env { url } => handle_env(url.as_deref().unwrap_or(&cli.base_url)).await?,
        Commands::Policy(cmd) => handle_policy(cmd, &client, cli.json).await?,
        Commands::Override(cmd) => handle_override(cmd, &client, cli.json).await?,
        Commands::Review(cmd) => handle_review(cmd, &client, cli.json).await?,
        Commands::Cache(cmd) => handle_cache(cmd, &client, cli.json).await?,
        Commands::Report(cmd) => handle_report(cmd, &client, cli.json).await?,
        Commands::Logs(cmd) => handle_logs(cmd, &client, cli.json).await?,
        Commands::Smoke { profile } => run_smoke(profile).await?,
    }

    Ok(())
}

async fn handle_auth(cmd: &AuthCmd) -> Result<()> {
    match cmd {
        AuthCmd::Login => {
            println!("Launching device flow... (placeholder)");
            println!("Visit https://login.example/device and enter code AX1Z-H3TQ");
        }
        AuthCmd::Logout => println!("Tokens cleared (placeholder)."),
        AuthCmd::Status => println!("Authenticated via OD_ADMIN_TOKEN (placeholder)."),
    }
    Ok(())
}

async fn handle_env(url: &str) -> Result<()> {
    let target = format!("{}/health/ready", url.trim_end_matches('/'));
    let resp = reqwest::get(&target).await?;
    if resp.status().is_success() {
        println!("Admin API healthy at {url}");
        Ok(())
    } else {
        Err(anyhow!("Health check failed: {}", resp.status()))
    }
}

async fn handle_policy(cmd: &PolicyCmd, client: &ApiClient, json: bool) -> Result<()> {
    match cmd {
        PolicyCmd::List => {
            let response: Paged<PolicySummary> = client
                .get("/api/v1/policies", &[("include_drafts", "true")])
                .await?;
            if json {
                print_json(&response)?;
            } else {
                let rows = response
                    .data
                    .into_iter()
                    .map(|policy| {
                        vec![
                            policy.id.to_string(),
                            policy.name,
                            policy.version,
                            policy.status,
                            policy.rule_count.to_string(),
                        ]
                    })
                    .collect();
                print_table(&["ID", "Name", "Version", "Status", "Rules"], rows);
            }
        }
        PolicyCmd::Show { id } => {
            let detail: PolicyDetail = client.get(&format!("/api/v1/policies/{id}"), &[]).await?;
            render_policy(&detail, json)?;
        }
        PolicyCmd::Pull { id, file } => {
            let detail: PolicyDetail = client.get(&format!("/api/v1/policies/{id}"), &[]).await?;
            let document = policy_document_from_detail(&detail);
            fs::write(file, serde_json::to_string_pretty(&document)?)
                .with_context(|| format!("failed writing policy to {}", file.display()))?;
            println!(
                "Exported policy {id} (version {}) to {}",
                detail.version,
                file.display()
            );
        }
        PolicyCmd::Push { id, file, notes } => {
            let doc = load_policy_document(&file)?;
            let policy_id = id;
            let detail: PolicyDetail = client
                .get(&format!("/api/v1/policies/{policy_id}"), &[])
                .await?;
            let rule_payloads = rules_from_document(&doc);
            let validation_body = PolicyDraftRequestPayload {
                name: detail.name.clone(),
                version: Some(doc.version.clone()),
                created_by: None,
                notes: notes.clone(),
                rules: rule_payloads.clone(),
            };
            let validation: PolicyValidationResponse = client
                .post("/api/v1/policies/validate", &validation_body)
                .await?;
            if !validation.valid {
                if json {
                    print_json(&validation)?;
                } else {
                    println!("Policy validation failed: {}", validation.errors.join(", "));
                }
                return Err(anyhow!("policy validation failed"));
            }
            let update_body = PolicyUpdateRequestPayload {
                name: None,
                version: Some(doc.version.clone()),
                status: None,
                notes: notes.clone(),
                rules: Some(rule_payloads),
            };
            let updated: PolicyDetail = client
                .put(&format!("/api/v1/policies/{policy_id}"), &update_body)
                .await?;
            if !json {
                println!(
                    "Applied policy document {} to {}",
                    file.display(),
                    updated.name
                );
            }
            render_policy(&updated, json)?;
        }
        PolicyCmd::Publish { id, version } => {
            let body = serde_json::json!({ "version": version });
            let detail: PolicyDetail = client
                .post(&format!("/api/v1/policies/{id}/publish"), &body)
                .await?;
            println!("Published policy {id} -> {}", detail.version);
        }
        PolicyCmd::Validate { file } => {
            let doc = load_policy_document(&file)?;
            let rule_payloads = rules_from_document(&doc);
            let body = PolicyDraftRequestPayload {
                name: "odctl-validate".to_string(),
                version: Some(doc.version.clone()),
                created_by: None,
                notes: Some("CLI validation".to_string()),
                rules: rule_payloads,
            };
            let response: PolicyValidationResponse =
                client.post("/api/v1/policies/validate", &body).await?;
            if json {
                print_json(&response)?;
            } else if response.valid {
                println!("Policy document is valid.");
            } else {
                println!("Validation errors: {}", response.errors.join(", "));
            }
        }
    }
    Ok(())
}

async fn handle_override(cmd: &OverrideCmd, client: &ApiClient, json: bool) -> Result<()> {
    match cmd {
        OverrideCmd::List => {
            let response: Vec<OverrideRecord> = client.get("/api/v1/overrides", &[]).await?;
            if json {
                print_json(&response)?;
            } else {
                let rows = response
                    .into_iter()
                    .map(|item| {
                        vec![
                            item.id.to_string(),
                            format!("{}:{}", item.scope_type, item.scope_value),
                            item.action,
                            item.status,
                            item.expires_at.unwrap_or_else(|| "-".into()),
                        ]
                    })
                    .collect();
                print_table(&["ID", "Scope", "Action", "Status", "Expires"], rows);
            }
        }
        OverrideCmd::Create {
            scope,
            action,
            reason,
            expires_at,
            status,
        } => {
            let (scope_type, scope_value) = parse_scope(scope)?;
            let payload = OverrideUpsertPayload {
                scope_type,
                scope_value,
                action: action.to_ascii_lowercase(),
                reason: reason.clone(),
                created_by: None,
                expires_at: expires_at.clone(),
                status: status.clone(),
            };
            let record: OverrideRecord = client.post("/api/v1/overrides", &payload).await?;
            render_override(&record, json)?;
        }
        OverrideCmd::Update {
            id,
            scope,
            action,
            reason,
            expires_at,
            status,
        } => {
            let (scope_type, scope_value) = parse_scope(scope)?;
            let payload = OverrideUpsertPayload {
                scope_type,
                scope_value,
                action: action.to_ascii_lowercase(),
                reason: reason.clone(),
                created_by: None,
                expires_at: expires_at.clone(),
                status: status.clone(),
            };
            let record: OverrideRecord = client
                .put(&format!("/api/v1/overrides/{id}"), &payload)
                .await?;
            render_override(&record, json)?;
        }
        OverrideCmd::Delete { id } => {
            client.delete(&format!("/api/v1/overrides/{id}")).await?;
            if !json {
                println!("Deleted override {id}");
            }
        }
    }
    Ok(())
}

async fn handle_review(cmd: &ReviewCmd, client: &ApiClient, json: bool) -> Result<()> {
    match cmd {
        ReviewCmd::Queue => {
            let response: Vec<ReviewRecord> = client.get("/api/v1/review-queue", &[]).await?;
            if json {
                print_json(&response)?;
            } else {
                let rows = response
                    .into_iter()
                    .map(|item| {
                        vec![
                            item.id.to_string(),
                            item.normalized_key,
                            item.status,
                            item.assigned_to.unwrap_or_else(|| "-".into()),
                        ]
                    })
                    .collect();
                print_table(&["ID", "Key", "Status", "Assigned"], rows);
            }
        }
        ReviewCmd::Resolve {
            id,
            status,
            decision_action,
            notes,
        } => {
            let payload = ReviewResolvePayload {
                status: status.to_string(),
                decided_by: None,
                decision_notes: notes.clone(),
                decision_action: decision_action.clone(),
            };
            let record: ReviewRecord = client
                .post(&format!("/api/v1/review-queue/{id}/resolve"), &payload)
                .await?;
            if json {
                print_json(&record)?;
            } else {
                println!(
                    "Resolved {} -> status={} action={}",
                    record.id,
                    record.status,
                    record.decision_action.clone().unwrap_or_else(|| "-".into())
                );
            }
        }
    }
    Ok(())
}

async fn handle_cache(cmd: &CacheCmd, client: &ApiClient, json: bool) -> Result<()> {
    match cmd {
        CacheCmd::Get { key } => {
            let record: CacheEntryRecord = client
                .get(&format!("/api/v1/cache-entries/{key}"), &[])
                .await?;
            if json {
                print_json(&record)?;
            } else {
                println!("Cache Key: {}", record.cache_key);
                println!("Expires: {}", record.expires_at);
                println!("Source : {}", record.source.unwrap_or_else(|| "-".into()));
                println!("Value  : {}", record.value);
            }
        }
        CacheCmd::Delete { key } => {
            client
                .delete(&format!("/api/v1/cache-entries/{key}"))
                .await?;
            println!("Deleted cache entry {key}");
        }
    }
    Ok(())
}

async fn handle_report(cmd: &ReportCmd, client: &ApiClient, json: bool) -> Result<()> {
    match cmd {
        ReportCmd::Summary { dimension } => {
            let response: Paged<ReportingAggregate> = client
                .get(
                    "/api/v1/reporting/aggregates",
                    &[("dimension", dimension.as_str()), ("page_size", "5")],
                )
                .await?;
            if json {
                print_json(&response)?;
            } else {
                println!("Dimension: {}", dimension);
                for agg in response.data {
                    println!("- {} :: {}", agg.period_start, agg.metrics);
                }
            }
        }
    }
    Ok(())
}

async fn handle_logs(cmd: &LogsCmd, client: &ApiClient, json: bool) -> Result<()> {
    match cmd {
        LogsCmd::Cli { operator, limit } => {
            let mut params = vec![("limit".to_string(), limit.to_string())];
            if let Some(op) = operator {
                params.push(("operator_id".to_string(), op.clone()));
            }
            let param_refs: Vec<(&str, &str)> = params
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            let response: Vec<CliLogRecord> = client.get("/api/v1/cli-logs", &param_refs).await?;
            if json {
                print_json(&response)?;
            } else {
                let rows = response
                    .into_iter()
                    .map(|log| {
                        vec![
                            log.id.to_string(),
                            log.operator_id.unwrap_or_else(|| "-".into()),
                            log.command,
                            log.result.unwrap_or_else(|| "-".into()),
                            log.created_at,
                        ]
                    })
                    .collect();
                print_table(&["ID", "Operator", "Command", "Result", "Created"], rows);
            }
        }
    }
    Ok(())
}

fn load_policy_document(file: &PathBuf) -> Result<PolicyDocument> {
    let path = file
        .to_str()
        .ok_or_else(|| anyhow!("path contains invalid UTF-8: {}", file.display()))?;
    PolicyDocument::load_from_file(path)
        .with_context(|| format!("failed to parse policy document from {}", file.display()))
}

fn rules_from_document(doc: &PolicyDocument) -> Vec<PolicyRulePayloadDto> {
    doc.rules
        .iter()
        .map(|rule| PolicyRulePayloadDto {
            id: Some(rule.id.clone()),
            description: rule.description.clone(),
            priority: rule.priority,
            action: rule.action.to_string(),
            conditions: rule.conditions.clone(),
        })
        .collect()
}

fn policy_document_from_detail(detail: &PolicyDetail) -> PolicyDocument {
    let rules = detail
        .rules
        .iter()
        .map(|rule| DslPolicyRule {
            id: rule.id.clone(),
            description: rule.description.clone(),
            priority: rule.priority,
            action: rule.action.clone(),
            conditions: rule.conditions.clone(),
        })
        .collect();
    PolicyDocument {
        version: detail.version.clone(),
        rules,
    }
}

fn render_policy(detail: &PolicyDetail, json: bool) -> Result<()> {
    if json {
        print_json(detail)?;
    } else {
        println!(
            "Policy: {} (version {}, status {})",
            detail.name, detail.version, detail.status
        );
        println!("Rules: {}", detail.rule_count);
        let rows = detail
            .rules
            .iter()
            .map(|rule| {
                vec![
                    rule.priority.to_string(),
                    rule.action.to_string(),
                    rule.description.clone().unwrap_or_else(|| "-".into()),
                ]
            })
            .collect();
        print_table(&["Priority", "Action", "Description"], rows);
    }
    Ok(())
}

fn render_override(record: &OverrideRecord, json: bool) -> Result<()> {
    if json {
        print_json(record)?;
    } else {
        println!(
            "Override {} -> {}:{} {} (status {})",
            record.id, record.scope_type, record.scope_value, record.action, record.status
        );
        if let Some(reason) = &record.reason {
            println!("Reason : {}", reason);
        }
        if let Some(expires) = &record.expires_at {
            println!("Expires: {}", expires);
        }
    }
    Ok(())
}

fn parse_scope(scope: &str) -> Result<(String, String)> {
    let mut parts = scope.splitn(2, ':');
    let scope_type = parts
        .next()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("scope must be in the form type:value"))?;
    let scope_value = parts
        .next()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("scope must include a value after ':'"))?;
    Ok((scope_type, scope_value))
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn print_table(headers: &[&str], rows: Vec<Vec<String>>) {
    if rows.is_empty() {
        println!("(no rows)");
        return;
    }
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in &rows {
        for (idx, cell) in row.iter().enumerate() {
            if idx < widths.len() {
                widths[idx] = widths[idx].max(cell.len());
            }
        }
    }
    let mut header_line = String::new();
    for (idx, header) in headers.iter().enumerate() {
        header_line.push_str(&format!("| {:width$} ", header, width = widths[idx]));
    }
    header_line.push('|');
    println!("{header_line}");
    let mut divider = String::new();
    for width in &widths {
        divider.push_str("|-");
        divider.push_str(&"-".repeat(*width));
        divider.push('-');
    }
    divider.push('|');
    println!("{divider}");
    for row in rows {
        let mut line = String::new();
        for (idx, cell) in row.iter().enumerate() {
            line.push_str(&format!("| {:width$} ", cell, width = widths[idx]));
        }
        line.push('|');
        println!("{line}");
    }
}

async fn run_smoke(profile: &str) -> Result<()> {
    info!(profile = profile, "running smoke profile");
    let target = match profile {
        "prod" => "icap.prod:1344",
        "staging" => "127.0.0.1:1344",
        other => other,
    };
    let request = "REQMOD icap://icap.service/req ICAP/1.0\r\nHost: icap.service\r\nX-Trace-Id: odctl-smoke\r\nEncapsulated: req-body=0, null-body=0\r\n\r\nGET http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let mut stream = TcpStream::connect(target).await?;
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

struct ApiClient {
    base_url: String,
    token: Option<String>,
    http: Client,
}

impl ApiClient {
    fn new(base_url: &str, token: Option<String>) -> Result<Self> {
        let http = Client::builder().timeout(Duration::from_secs(30)).build()?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
            http,
        })
    }

    async fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.http.get(url);
        if !query.is_empty() {
            req = req.query(&query);
        }
        let resp = self.send(req).await?;
        Ok(resp.json().await?)
    }

    async fn post<T: DeserializeOwned>(&self, path: &str, body: &impl Serialize) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let req = self.http.post(url).json(body);
        let resp = self.send(req).await?;
        Ok(resp.json().await?)
    }

    async fn put<T: DeserializeOwned>(&self, path: &str, body: &impl Serialize) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let req = self.http.put(url).json(body);
        let resp = self.send(req).await?;
        Ok(resp.json().await?)
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let url = format!("{}{}", self.base_url, path);
        let req = self.http.delete(url);
        self.send(req).await?;
        Ok(())
    }

    async fn send(&self, req: reqwest::RequestBuilder) -> Result<reqwest::Response> {
        let mut builder = req;
        if let Some(token) = &self.token {
            builder = builder.header("X-Admin-Token", token);
        }
        let resp = builder.send().await?;
        if resp.status().is_success() {
            Ok(resp)
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_else(|_| "<empty>".into());
            Err(anyhow!("request failed: {status} -> {body}"))
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct Paged<T> {
    data: Vec<T>,
    meta: PageMeta,
}

#[derive(Debug, Deserialize, Serialize)]
struct PageMeta {
    page: u32,
    page_size: u32,
    total: i64,
    has_more: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct PolicySummary {
    id: Uuid,
    name: String,
    version: String,
    status: String,
    rule_count: i64,
}

#[derive(Debug, Deserialize, Serialize)]
struct PolicyDetail {
    id: Uuid,
    name: String,
    version: String,
    status: String,
    rule_count: i64,
    rules: Vec<PolicyRuleResponse>,
}

#[derive(Debug, Deserialize, Serialize)]
struct PolicyRuleResponse {
    id: String,
    description: Option<String>,
    priority: u32,
    action: PolicyAction,
    #[serde(default)]
    conditions: RuleConditions,
}

#[derive(Debug, Deserialize, Serialize)]
struct PolicyValidationResponse {
    valid: bool,
    errors: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
struct PolicyRulePayloadDto {
    id: Option<String>,
    description: Option<String>,
    priority: u32,
    action: String,
    #[serde(default)]
    conditions: RuleConditions,
}

#[derive(Debug, Serialize)]
struct PolicyDraftRequestPayload {
    name: String,
    version: Option<String>,
    created_by: Option<String>,
    notes: Option<String>,
    rules: Vec<PolicyRulePayloadDto>,
}

#[derive(Debug, Serialize)]
struct PolicyUpdateRequestPayload {
    name: Option<String>,
    version: Option<String>,
    status: Option<String>,
    notes: Option<String>,
    rules: Option<Vec<PolicyRulePayloadDto>>,
}

#[derive(Debug, Deserialize, Serialize)]
struct OverrideRecord {
    id: Uuid,
    scope_type: String,
    scope_value: String,
    action: String,
    status: String,
    reason: Option<String>,
    created_by: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    expires_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct OverrideUpsertPayload {
    scope_type: String,
    scope_value: String,
    action: String,
    reason: Option<String>,
    created_by: Option<String>,
    expires_at: Option<String>,
    status: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ReviewRecord {
    id: Uuid,
    normalized_key: String,
    status: String,
    assigned_to: Option<String>,
    decided_by: Option<String>,
    decision_notes: Option<String>,
    decision_action: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct ReviewResolvePayload {
    status: String,
    decided_by: Option<String>,
    decision_notes: Option<String>,
    decision_action: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct CacheEntryRecord {
    cache_key: String,
    value: serde_json::Value,
    expires_at: String,
    source: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ReportingAggregate {
    id: Uuid,
    dimension: String,
    period_start: String,
    metrics: serde_json::Value,
    created_at: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct CliLogRecord {
    id: Uuid,
    operator_id: Option<String>,
    command: String,
    result: Option<String>,
    created_at: String,
}
