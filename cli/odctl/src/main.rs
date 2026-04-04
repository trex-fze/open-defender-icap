use std::{
    env, fs,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use common_types::PolicyAction;
use dirs::config_dir;
use policy_dsl::{Conditions as RuleConditions, PolicyDocument, PolicyRule as DslPolicyRule};
use reqwest::{header, Client};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sqlx::{migrate::Migrator, postgres::PgPoolOptions};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::sleep,
};
use tracing::info;
use urlencoding::encode;
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
    Cache(CacheCmd),
    #[clap(subcommand)]
    Migrate(MigrateCmd),
    #[clap(subcommand)]
    Page(PageCmd),
    #[clap(subcommand)]
    Classification(ClassificationCmd),
    #[clap(subcommand)]
    Report(ReportCmd),
    #[clap(subcommand)]
    Logs(LogsCmd),
    #[clap(subcommand)]
    Llm(LlmCmd),
    #[clap(subcommand)]
    Iam(IamCmd),
    Smoke {
        #[clap(long, default_value = "staging")]
        profile: String,
    },
}

#[derive(Subcommand, Debug)]
enum AuthCmd {
    Login {
        #[clap(long, env = "OD_OIDC_CLIENT_ID")]
        client_id: Option<String>,
        #[clap(long = "device-url", env = "OD_OIDC_DEVICE_URL")]
        device_url: Option<String>,
        #[clap(long = "token-url", env = "OD_OIDC_TOKEN_URL")]
        token_url: Option<String>,
        #[clap(long, env = "OD_OIDC_SCOPE")]
        scope: Option<String>,
        #[clap(long, env = "OD_OIDC_AUDIENCE")]
        audience: Option<String>,
    },
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
    List {
        #[clap(long, default_value_t = 50)]
        limit: u32,
        #[clap(long)]
        cursor: Option<String>,
    },
    Create {
        #[clap(long, help = "Scope in the form domain:<hostname> (domain-only mode)")]
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
        #[clap(long, help = "Scope in the form domain:<hostname> (domain-only mode)")]
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
enum CacheCmd {
    Get { key: String },
    Delete { key: String },
}

#[derive(Subcommand, Debug)]
enum MigrateCmd {
    Run {
        #[clap(value_enum, default_value = "all")]
        target: MigrateTarget,
        #[clap(long)]
        admin_url: Option<String>,
        #[clap(long)]
        policy_url: Option<String>,
    },
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum MigrateTarget {
    Admin,
    Policy,
    All,
}

#[derive(Subcommand, Debug)]
enum PageCmd {
    Show {
        key: String,
        #[clap(long)]
        version: Option<i64>,
        #[clap(long, default_value_t = 1_200)]
        excerpt: usize,
    },
    History {
        key: String,
        #[clap(long, default_value_t = 5)]
        limit: u32,
    },
}

#[derive(Subcommand, Debug)]
enum ClassificationCmd {
    Pending {
        #[clap(long, default_value_t = 50)]
        limit: u32,
        #[clap(long)]
        cursor: Option<String>,
        #[clap(long)]
        status: Option<String>,
    },
    Unblock {
        key: String,
        #[clap(long)]
        action: String,
        #[clap(long, help = "Primary category label")]
        category: String,
        #[clap(long, help = "Subcategory label")]
        subcategory: String,
        #[clap(long, help = "Risk level (low/medium/high/critical)")]
        risk: String,
        #[clap(long, default_value_t = 0.9)]
        confidence: f32,
        #[clap(long)]
        reason: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum ReportCmd {
    Summary {
        #[clap(long, default_value = "category")]
        dimension: String,
    },
    Traffic {
        #[clap(long, default_value = "24h")]
        range: String,
        #[clap(long, default_value_t = 10)]
        top: u32,
        #[clap(long)]
        bucket: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum LogsCmd {
    Cli {
        #[clap(long)]
        operator: Option<String>,
        #[clap(long, default_value_t = 50)]
        limit: u32,
        #[clap(long)]
        cursor: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum LlmCmd {
    Providers {
        #[clap(long, default_value = "http://localhost:19015/providers")]
        url: String,
    },
}

#[derive(Subcommand, Debug)]
enum IamCmd {
    #[clap(subcommand)]
    Users(IamUsersCmd),
    #[clap(subcommand)]
    Groups(IamGroupsCmd),
    #[clap(subcommand)]
    Roles(IamRolesCmd),
    #[clap(subcommand)]
    ServiceAccounts(IamServiceAccountCmd),
    Whoami,
}

#[derive(Subcommand, Debug)]
enum IamUsersCmd {
    List {
        #[clap(long, default_value_t = 50)]
        limit: u32,
        #[clap(long)]
        cursor: Option<String>,
    },
    Create {
        #[clap(long)]
        email: String,
        #[clap(long)]
        display_name: Option<String>,
        #[clap(long)]
        subject: Option<String>,
        #[clap(long, default_value = "active")]
        status: String,
    },
    Disable {
        id: String,
    },
    AssignRole {
        id: String,
        role: String,
    },
    RevokeRole {
        id: String,
        role: String,
    },
}

#[derive(Subcommand, Debug)]
enum IamGroupsCmd {
    List {
        #[clap(long, default_value_t = 50)]
        limit: u32,
        #[clap(long)]
        cursor: Option<String>,
    },
    Create {
        #[clap(long)]
        name: String,
        #[clap(long)]
        description: Option<String>,
    },
    Delete {
        id: String,
    },
    AddMember {
        id: String,
        #[clap(long)]
        user_id: String,
    },
    RemoveMember {
        id: String,
        #[clap(long)]
        user_id: String,
    },
    AssignRole {
        id: String,
        role: String,
    },
    RevokeRole {
        id: String,
        role: String,
    },
}

#[derive(Subcommand, Debug)]
enum IamRolesCmd {
    List,
}

#[derive(Subcommand, Debug)]
enum IamServiceAccountCmd {
    List {
        #[clap(long, default_value_t = 50)]
        limit: u32,
        #[clap(long)]
        cursor: Option<String>,
    },
    Create {
        #[clap(long)]
        name: String,
        #[clap(long)]
        description: Option<String>,
        #[clap(long = "role")]
        roles: Vec<String>,
    },
    Rotate {
        id: String,
        #[clap(long = "role")]
        roles: Vec<String>,
    },
    Disable {
        id: String,
    },
}

const DEFAULT_OIDC_SCOPE: &str = "openid profile email offline_access";
const DEFAULT_DEVICE_URL: &str = "https://login.example.com/oauth/device/code";
const DEFAULT_TOKEN_URL: &str = "https://login.example.com/oauth/token";
const DEFAULT_CLIENT_ID: &str = "odctl-dev";

static ADMIN_MIGRATOR: Migrator = sqlx::migrate!("../../services/admin-api/migrations");
static POLICY_MIGRATOR: Migrator = sqlx::migrate!("../../services/policy-engine/migrations");

#[derive(Debug, Clone)]
struct DeviceFlowConfig {
    client_id: String,
    device_url: String,
    token_url: String,
    scope: String,
    audience: Option<String>,
}

impl DeviceFlowConfig {
    fn new(
        client_id: Option<String>,
        device_url: Option<String>,
        token_url: Option<String>,
        scope: Option<String>,
        audience: Option<String>,
    ) -> Self {
        Self {
            client_id: client_id.unwrap_or_else(|| DEFAULT_CLIENT_ID.to_string()),
            device_url: device_url.unwrap_or_else(|| DEFAULT_DEVICE_URL.to_string()),
            token_url: token_url.unwrap_or_else(|| DEFAULT_TOKEN_URL.to_string()),
            scope: scope.unwrap_or_else(|| DEFAULT_OIDC_SCOPE.to_string()),
            audience,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthSession {
    access_token: String,
    refresh_token: Option<String>,
    token_type: Option<String>,
    scope: Option<String>,
    expires_at: Option<i64>,
    obtained_at: Option<i64>,
    client_id: String,
    token_endpoint: String,
    device_endpoint: Option<String>,
    audience: Option<String>,
}

impl AuthSession {
    fn is_expired(&self) -> bool {
        if let Some(exp) = self.expires_at {
            current_timestamp() + 60 >= exp // refresh one minute early
        } else {
            false
        }
    }
}

fn session_file() -> Result<PathBuf> {
    let mut dir = config_dir().ok_or_else(|| anyhow!("unable to locate config directory"))?;
    dir.push("odctl");
    fs::create_dir_all(&dir)?;
    dir.push("session.json");
    Ok(dir)
}

fn load_session() -> Result<Option<AuthSession>> {
    let path = session_file()?;
    if !path.exists() {
        return Ok(None);
    }
    let data = fs::read_to_string(&path)?;
    let session = serde_json::from_str::<AuthSession>(&data)?;
    Ok(Some(session))
}

fn save_session(session: &AuthSession) -> Result<()> {
    let path = session_file()?;
    let data = serde_json::to_string_pretty(session)?;
    fs::write(&path, data)?;
    Ok(())
}

fn delete_session() -> Result<()> {
    let path = session_file()?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

async fn resolve_token(cli_token: Option<String>) -> Result<Option<String>> {
    if let Some(token) = cli_token {
        if !token.trim().is_empty() {
            return Ok(Some(token));
        }
    }
    let mut session = match load_session()? {
        Some(session) => session,
        None => {
            return Err(anyhow!(
                "No stored session. Run `odctl auth login` or provide --token."
            ))
        }
    };
    if session.is_expired() {
        if session.refresh_token.is_none() {
            return Err(anyhow!("Access token expired and no refresh token available. Please run `odctl auth login`."));
        }
        session = refresh_session(&session).await?;
        save_session(&session)?;
    }
    Ok(Some(session.access_token.clone()))
}

async fn refresh_session(session: &AuthSession) -> Result<AuthSession> {
    let refresh = session
        .refresh_token
        .clone()
        .ok_or_else(|| anyhow!("missing refresh token"))?;
    let client = Client::new();
    let mut params = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh.as_str()),
        ("client_id", session.client_id.as_str()),
    ];
    if let Some(scope) = &session.scope {
        params.push(("scope", scope.as_str()));
    }
    if let Some(aud) = &session.audience {
        params.push(("audience", aud.as_str()));
    }
    let resp = client
        .post(&session.token_endpoint)
        .form(&params)
        .send()
        .await?
        .error_for_status()?;
    let token = resp.json::<TokenResponse>().await?;
    build_session_from_token(session, token)
}

fn build_session_from_token(base: &AuthSession, token: TokenResponse) -> Result<AuthSession> {
    let expires_at = token
        .expires_in
        .map(|ttl| current_timestamp() + ttl as i64)
        .or(base.expires_at);
    Ok(AuthSession {
        access_token: token.access_token,
        refresh_token: token.refresh_token.or_else(|| base.refresh_token.clone()),
        token_type: token.token_type.or_else(|| base.token_type.clone()),
        scope: token.scope.or_else(|| base.scope.clone()),
        expires_at,
        obtained_at: Some(current_timestamp()),
        client_id: base.client_id.clone(),
        token_endpoint: base.token_endpoint.clone(),
        device_endpoint: base.device_endpoint.clone(),
        audience: base.audience.clone(),
    })
}

fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default)]
    verification_uri_complete: Option<String>,
    expires_in: u64,
    #[serde(default = "default_interval")]
    interval: u64,
}

const fn default_interval() -> u64 {
    5
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    token_type: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_target(false).init();
    let cli = Cli::parse();
    if let Commands::Auth(cmd) = &cli.command {
        handle_auth(cmd).await?;
        return Ok(());
    }
    if let Commands::Migrate(cmd) = &cli.command {
        handle_migrate(cmd).await?;
        return Ok(());
    }

    let token = resolve_token(cli.token.clone()).await?;
    let client = ApiClient::new(&cli.base_url, token)?;

    match &cli.command {
        Commands::Env { url } => handle_env(url.as_deref().unwrap_or(&cli.base_url)).await?,
        Commands::Policy(cmd) => handle_policy(cmd, &client, cli.json).await?,
        Commands::Override(cmd) => handle_override(cmd, &client, cli.json).await?,
        Commands::Cache(cmd) => handle_cache(cmd, &client, cli.json).await?,
        Commands::Migrate(_) => unreachable!(),
        Commands::Page(cmd) => handle_page(cmd, &client, cli.json).await?,
        Commands::Classification(cmd) => handle_classification(cmd, &client, cli.json).await?,
        Commands::Report(cmd) => handle_report(cmd, &client, cli.json).await?,
        Commands::Logs(cmd) => handle_logs(cmd, &client, cli.json).await?,
        Commands::Llm(cmd) => handle_llm(cmd, cli.json).await?,
        Commands::Iam(cmd) => handle_iam(cmd, &client, cli.json).await?,
        Commands::Smoke { profile } => run_smoke(profile).await?,
        Commands::Auth(_) => unreachable!(),
    }

    Ok(())
}

async fn handle_auth(cmd: &AuthCmd) -> Result<()> {
    match cmd {
        AuthCmd::Login {
            client_id,
            device_url,
            token_url,
            scope,
            audience,
        } => {
            let config = DeviceFlowConfig::new(
                client_id.clone(),
                device_url.clone(),
                token_url.clone(),
                scope.clone(),
                audience.clone(),
            );
            perform_device_login(config).await?;
        }
        AuthCmd::Logout => {
            delete_session()?;
            println!("Logged out and cleared stored tokens.");
        }
        AuthCmd::Status => match load_session()? {
            Some(session) => {
                println!("Client ID : {}", session.client_id);
                if let Some(exp) = session.expires_at {
                    let remaining = exp.saturating_sub(current_timestamp());
                    println!("Access exp: {}s", remaining);
                }
                if session.refresh_token.is_some() {
                    println!("Refresh  : available");
                }
                println!(
                    "Audience  : {}",
                    session.audience.clone().unwrap_or_else(|| "-".into())
                );
            }
            None => println!("No stored session. Use 'odctl auth login' to authenticate."),
        },
    }
    Ok(())
}

async fn perform_device_login(cfg: DeviceFlowConfig) -> Result<()> {
    let client = Client::new();
    let device_resp = client
        .post(&cfg.device_url)
        .form(&DeviceCodeRequest {
            client_id: &cfg.client_id,
            scope: &cfg.scope,
            audience: cfg.audience.as_deref(),
        })
        .send()
        .await?
        .error_for_status()?;
    let device = device_resp.json::<DeviceCodeResponse>().await?;

    println!(
        "Open {}",
        device
            .verification_uri_complete
            .clone()
            .unwrap_or(device.verification_uri.clone())
    );
    println!("Enter code: {}", device.user_code);

    let mut interval = Duration::from_secs(device.interval.max(1));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(device.expires_in);

    let request = DeviceTokenRequest {
        client_id: &cfg.client_id,
        device_code: &device.device_code,
        grant_type: "urn:ietf:params:oauth:grant-type:device_code",
        audience: cfg.audience.as_deref(),
    };

    loop {
        if tokio::time::Instant::now() >= deadline {
            return Err(anyhow!("Device code expired before approval"));
        }
        let resp = client.post(&cfg.token_url).form(&request).send().await?;
        if resp.status().is_success() {
            let token = resp.json::<TokenResponse>().await?;
            let base = AuthSession {
                access_token: String::new(),
                refresh_token: None,
                token_type: None,
                scope: Some(cfg.scope.clone()),
                expires_at: None,
                obtained_at: Some(current_timestamp()),
                client_id: cfg.client_id.clone(),
                token_endpoint: cfg.token_url.clone(),
                device_endpoint: Some(cfg.device_url.clone()),
                audience: cfg.audience.clone(),
            };
            let mut session = build_session_from_token(&base, token)?;
            session.scope = base.scope.clone();
            save_session(&session)?;
            println!("Authentication successful. Tokens stored.");
            return Ok(());
        }

        if resp.status() == reqwest::StatusCode::BAD_REQUEST {
            let err_body = resp.json::<DeviceErrorResponse>().await?;
            match err_body.error.as_str() {
                "authorization_pending" => {
                    sleep(interval).await;
                    continue;
                }
                "slow_down" => {
                    interval += Duration::from_secs(5);
                    sleep(interval).await;
                    continue;
                }
                "expired_token" => return Err(anyhow!("Device code expired")),
                other => {
                    return Err(anyhow!(
                        "Device flow failed: {} {}",
                        other,
                        err_body.error_description.unwrap_or_default()
                    ))
                }
            }
        } else {
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Token request failed: {}", text));
        }
    }
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
        OverrideCmd::List { limit, cursor } => {
            let mut params = vec![("limit".to_string(), limit.to_string())];
            if let Some(value) = cursor {
                params.push(("cursor".to_string(), value.clone()));
            }
            let refs: Vec<(&str, &str)> = params
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            let response: CursorPaged<OverrideRecord> =
                client.get("/api/v1/overrides", &refs).await?;
            if json {
                print_json(&response)?;
            } else {
                let rows = response
                    .data
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
                print_next_cursor(response.meta.next_cursor.as_deref());
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

async fn handle_migrate(cmd: &MigrateCmd) -> Result<()> {
    match cmd {
        MigrateCmd::Run {
            target,
            admin_url,
            policy_url,
        } => match target {
            MigrateTarget::Admin => {
                let url = resolve_db_url(admin_url, "OD_ADMIN_DATABASE_URL", "--admin-url")?;
                run_migrations("admin-api", &url, &ADMIN_MIGRATOR).await?;
            }
            MigrateTarget::Policy => {
                let url = resolve_db_url(policy_url, "OD_POLICY_DATABASE_URL", "--policy-url")?;
                run_migrations("policy-engine", &url, &POLICY_MIGRATOR).await?;
            }
            MigrateTarget::All => {
                let admin = resolve_db_url(admin_url, "OD_ADMIN_DATABASE_URL", "--admin-url")?;
                let policy = resolve_db_url(policy_url, "OD_POLICY_DATABASE_URL", "--policy-url")?;
                run_migrations("admin-api", &admin, &ADMIN_MIGRATOR).await?;
                run_migrations("policy-engine", &policy, &POLICY_MIGRATOR).await?;
            }
        },
    }
    Ok(())
}

fn resolve_db_url(source: &Option<String>, env_key: &str, flag: &str) -> Result<String> {
    if let Some(url) = source {
        if url.trim().is_empty() {
            anyhow::bail!("{} cannot be empty", flag);
        }
        return Ok(url.clone());
    }
    env::var(env_key).with_context(|| format!("set {} or pass {}", env_key, flag))
}

async fn run_migrations(name: &str, url: &str, migrator: &Migrator) -> Result<()> {
    println!("Running {name} migrations...");
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(url)
        .await
        .with_context(|| format!("failed to connect to {name} database"))?;
    migrator
        .run(&pool)
        .await
        .with_context(|| format!("failed to execute {name} migrations"))?;
    println!("Completed {name} migrations.");
    Ok(())
}

async fn handle_page(cmd: &PageCmd, client: &ApiClient, json: bool) -> Result<()> {
    match cmd {
        PageCmd::Show {
            key,
            version,
            excerpt,
        } => {
            let encoded = encode(key);
            let mut params = vec![("max_excerpt".to_string(), excerpt.to_string())];
            if let Some(v) = version {
                params.push(("version".to_string(), v.to_string()));
            }
            let refs: Vec<(&str, &str)> = params
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            let record: PageContentRecord = client
                .get(&format!("/api/v1/page-contents/{encoded}"), &refs)
                .await?;
            if json {
                print_json(&record)?;
            } else {
                render_page_content(&record);
            }
        }
        PageCmd::History { key, limit } => {
            let encoded = encode(key);
            let params = vec![("limit".to_string(), limit.to_string())];
            let refs: Vec<(&str, &str)> = params
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            let history: Vec<PageContentSummary> = client
                .get(&format!("/api/v1/page-contents/{encoded}/history"), &refs)
                .await?;
            if json {
                print_json(&history)?;
            } else if history.is_empty() {
                println!("No page content found for {key}");
            } else {
                let rows: Vec<Vec<String>> = history
                    .iter()
                    .map(|entry| {
                        vec![
                            entry.fetch_version.to_string(),
                            entry.fetch_status.clone(),
                            entry.fetch_reason.clone().unwrap_or_else(|| "-".into()),
                            entry.fetched_at.clone(),
                            entry.expires_at.clone(),
                        ]
                    })
                    .collect();
                print_table(&["Version", "Status", "Reason", "Fetched", "Expires"], rows);
            }
        }
    }
    Ok(())
}

async fn handle_classification(
    cmd: &ClassificationCmd,
    client: &ApiClient,
    json: bool,
) -> Result<()> {
    match cmd {
        ClassificationCmd::Pending {
            limit,
            cursor,
            status,
        } => {
            let mut params = vec![("limit".to_string(), limit.to_string())];
            if let Some(s) = status {
                params.push(("status".to_string(), s.clone()));
            }
            if let Some(value) = cursor {
                params.push(("cursor".to_string(), value.clone()));
            }
            let refs: Vec<(&str, &str)> = params
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            let records: CursorPaged<PendingClassificationRecord> =
                client.get("/api/v1/classifications/pending", &refs).await?;
            if json {
                print_json(&records)?;
            } else if records.data.is_empty() {
                println!("No pending classifications");
            } else {
                let rows: Vec<Vec<String>> = records
                    .data
                    .iter()
                    .map(|row| {
                        vec![
                            row.normalized_key.clone(),
                            row.status.clone(),
                            row.base_url.clone().unwrap_or_else(|| "-".into()),
                            row.updated_at.clone(),
                        ]
                    })
                    .collect();
                print_table(&["Key", "Status", "Base URL", "Updated"], rows);
                print_next_cursor(records.meta.next_cursor.as_deref());
            }
        }
        ClassificationCmd::Unblock {
            key,
            action,
            category,
            subcategory,
            risk,
            confidence,
            reason,
        } => {
            let body = serde_json::json!({
                "action": action,
                "primary_category": category,
                "subcategory": subcategory,
                "risk_level": risk,
                "confidence": confidence,
                "reason": reason,
            });
            let record: ManualClassificationResponse = client
                .post(
                    &format!("/api/v1/classifications/{}/unblock", encode(key)),
                    &body,
                )
                .await?;
            if json {
                print_json(&record)?;
            } else {
                println!("Updated classification for {}", record.normalized_key);
                println!("Action     : {}", record.recommended_action);
                println!(
                    "Category   : {} / {}",
                    record.primary_category, record.subcategory
                );
                println!("Risk       : {}", record.risk_level);
                println!("Confidence : {:.2}", record.confidence);
                println!("Updated At : {}", record.updated_at);
            }
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
        ReportCmd::Traffic { range, top, bucket } => {
            let mut params = vec![
                ("range".to_string(), range.clone()),
                ("top_n".to_string(), top.to_string()),
            ];
            if let Some(b) = bucket {
                params.push(("bucket".to_string(), b.clone()));
            }
            let refs: Vec<(&str, &str)> = params
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            let report: TrafficReportResponse =
                client.get("/api/v1/reporting/traffic", &refs).await?;
            if json {
                print_json(&report)?;
            } else {
                render_traffic_report(&report);
            }
        }
    }
    Ok(())
}

async fn handle_logs(cmd: &LogsCmd, client: &ApiClient, json: bool) -> Result<()> {
    match cmd {
        LogsCmd::Cli {
            operator,
            limit,
            cursor,
        } => {
            let mut params = vec![("limit".to_string(), limit.to_string())];
            if let Some(op) = operator {
                params.push(("operator_id".to_string(), op.clone()));
            }
            if let Some(value) = cursor {
                params.push(("cursor".to_string(), value.clone()));
            }
            let param_refs: Vec<(&str, &str)> = params
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            let response: CursorPaged<CliLogRecord> =
                client.get("/api/v1/cli-logs", &param_refs).await?;
            if json {
                print_json(&response)?;
            } else {
                let rows = response
                    .data
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
                print_next_cursor(response.meta.next_cursor.as_deref());
            }
        }
    }
    Ok(())
}

async fn handle_llm(cmd: &LlmCmd, json: bool) -> Result<()> {
    match cmd {
        LlmCmd::Providers { url } => {
            let summaries: Vec<LlmProviderSummary> = reqwest::get(url).await?.json().await?;
            if json {
                print_json(&summaries)?;
            } else {
                let rows = summaries
                    .iter()
                    .map(|summary| {
                        vec![
                            summary.name.clone(),
                            summary.provider_type.clone(),
                            summary.endpoint.clone(),
                            summary.role.clone(),
                        ]
                    })
                    .collect();
                print_table(&["Name", "Type", "Endpoint", "Role"], rows);
            }
        }
    }
    Ok(())
}

async fn handle_iam(cmd: &IamCmd, client: &ApiClient, json: bool) -> Result<()> {
    match cmd {
        IamCmd::Users(sub) => handle_iam_users(sub, client, json).await?,
        IamCmd::Groups(sub) => handle_iam_groups(sub, client, json).await?,
        IamCmd::Roles(sub) => handle_iam_roles(sub, client, json).await?,
        IamCmd::ServiceAccounts(sub) => handle_iam_service_accounts(sub, client, json).await?,
        IamCmd::Whoami => {
            let response: WhoAmIResponse = client.get("/api/v1/iam/whoami", &[]).await?;
            if json {
                print_json(&response)?;
            } else {
                println!("Actor           : {}", response.actor);
                println!("Principal Type  : {}", response.principal_type);
                if let Some(id) = response.principal_id {
                    println!("Principal ID    : {}", id);
                }
                println!("Roles           : {}", response.roles.join(", "));
                println!("Permissions     : {}", response.permissions.join(", "));
            }
        }
    }
    Ok(())
}

async fn handle_iam_users(cmd: &IamUsersCmd, client: &ApiClient, json: bool) -> Result<()> {
    match cmd {
        IamUsersCmd::List { limit, cursor } => {
            let mut params = vec![("limit".to_string(), limit.to_string())];
            if let Some(value) = cursor {
                params.push(("cursor".to_string(), value.clone()));
            }
            let refs: Vec<(&str, &str)> = params
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            let users: CursorPaged<IamUserDetails> = client.get("/api/v1/iam/users", &refs).await?;
            if json {
                print_json(&users)?;
            } else if users.data.is_empty() {
                println!("No users found");
            } else {
                let rows = users
                    .data
                    .iter()
                    .map(|user| {
                        vec![
                            user.user.id.to_string(),
                            user.user.email.clone(),
                            user.user.status.clone(),
                            user.roles.join(", "),
                        ]
                    })
                    .collect();
                print_table(&["ID", "Email", "Status", "Roles"], rows);
                print_next_cursor(users.meta.next_cursor.as_deref());
            }
        }
        IamUsersCmd::Create {
            email,
            display_name,
            subject,
            status,
        } => {
            let payload = CreateUserPayload {
                email: email.clone(),
                display_name: display_name.clone(),
                subject: subject.clone(),
                status: status.clone(),
            };
            let detail: IamUserDetails = client.post("/api/v1/iam/users", &payload).await?;
            if json {
                print_json(&detail)?;
            } else {
                println!("Created user {} ({})", detail.user.id, detail.user.email);
            }
        }
        IamUsersCmd::Disable { id } => {
            client.delete(&format!("/api/v1/iam/users/{id}")).await?;
            println!("Disabled user {id}");
        }
        IamUsersCmd::AssignRole { id, role } => {
            let payload = RoleChangePayload { role: role.clone() };
            let roles: Vec<String> = client
                .post(&format!("/api/v1/iam/users/{id}/roles"), &payload)
                .await?;
            if json {
                print_json(&roles)?;
            } else {
                println!("Roles now: {}", roles.join(", "));
            }
        }
        IamUsersCmd::RevokeRole { id, role } => {
            let roles: Vec<String> = client
                .delete_json(&format!("/api/v1/iam/users/{}/roles/{}", id, encode(role)))
                .await?;
            if json {
                print_json(&roles)?;
            } else {
                println!("Roles now: {}", roles.join(", "));
            }
        }
    }
    Ok(())
}

async fn handle_iam_groups(cmd: &IamGroupsCmd, client: &ApiClient, json: bool) -> Result<()> {
    match cmd {
        IamGroupsCmd::List { limit, cursor } => {
            let mut params = vec![("limit".to_string(), limit.to_string())];
            if let Some(value) = cursor {
                params.push(("cursor".to_string(), value.clone()));
            }
            let refs: Vec<(&str, &str)> = params
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            let groups: CursorPaged<IamGroupDetails> =
                client.get("/api/v1/iam/groups", &refs).await?;
            if json {
                print_json(&groups)?;
            } else if groups.data.is_empty() {
                println!("No groups found");
            } else {
                let rows = groups
                    .data
                    .iter()
                    .map(|group| {
                        vec![
                            group.group.id.to_string(),
                            group.group.name.clone(),
                            group.members.len().to_string(),
                            group.roles.join(", "),
                        ]
                    })
                    .collect();
                print_table(&["ID", "Name", "Members", "Roles"], rows);
                print_next_cursor(groups.meta.next_cursor.as_deref());
            }
        }
        IamGroupsCmd::Create { name, description } => {
            let payload = CreateGroupPayload {
                name: name.clone(),
                description: description.clone(),
            };
            let detail: IamGroupDetails = client.post("/api/v1/iam/groups", &payload).await?;
            if json {
                print_json(&detail)?;
            } else {
                println!("Created group {}", detail.group.name);
            }
        }
        IamGroupsCmd::Delete { id } => {
            client.delete(&format!("/api/v1/iam/groups/{id}")).await?;
            println!("Deleted group {id}");
        }
        IamGroupsCmd::AddMember { id, user_id } => {
            let payload = AddGroupMemberPayload {
                user_id: user_id.clone(),
            };
            let members: Vec<IamUserRecord> = client
                .post(&format!("/api/v1/iam/groups/{id}/members"), &payload)
                .await?;
            if json {
                print_json(&members)?;
            } else {
                println!("Group {id} now has {} members", members.len());
            }
        }
        IamGroupsCmd::RemoveMember { id, user_id } => {
            client
                .delete(&format!("/api/v1/iam/groups/{id}/members/{user_id}"))
                .await?;
            println!("Removed member {user_id} from group {id}");
        }
        IamGroupsCmd::AssignRole { id, role } => {
            let payload = RoleChangePayload { role: role.clone() };
            let roles: Vec<String> = client
                .post(&format!("/api/v1/iam/groups/{id}/roles"), &payload)
                .await?;
            if json {
                print_json(&roles)?;
            } else {
                println!("Group roles: {}", roles.join(", "));
            }
        }
        IamGroupsCmd::RevokeRole { id, role } => {
            let roles: Vec<String> = client
                .delete_json(&format!("/api/v1/iam/groups/{}/roles/{}", id, encode(role)))
                .await?;
            if json {
                print_json(&roles)?;
            } else {
                println!("Group roles: {}", roles.join(", "));
            }
        }
    }
    Ok(())
}

async fn handle_iam_roles(cmd: &IamRolesCmd, client: &ApiClient, json: bool) -> Result<()> {
    match cmd {
        IamRolesCmd::List => {
            let roles: Vec<IamRoleRecord> = client.get("/api/v1/iam/roles", &[]).await?;
            if json {
                print_json(&roles)?;
            } else if roles.is_empty() {
                println!("No roles found");
            } else {
                let rows = roles
                    .iter()
                    .map(|role| {
                        vec![
                            role.name.clone(),
                            role.permissions.join(", "),
                            role.builtin.to_string(),
                        ]
                    })
                    .collect();
                print_table(&["Role", "Permissions", "Builtin"], rows);
            }
        }
    }
    Ok(())
}

async fn handle_iam_service_accounts(
    cmd: &IamServiceAccountCmd,
    client: &ApiClient,
    json: bool,
) -> Result<()> {
    match cmd {
        IamServiceAccountCmd::List { limit, cursor } => {
            let mut params = vec![("limit".to_string(), limit.to_string())];
            if let Some(value) = cursor {
                params.push(("cursor".to_string(), value.clone()));
            }
            let refs: Vec<(&str, &str)> = params
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            let accounts: CursorPaged<ServiceAccountDetails> =
                client.get("/api/v1/iam/service-accounts", &refs).await?;
            if json {
                print_json(&accounts)?;
            } else if accounts.data.is_empty() {
                println!("No service accounts found");
            } else {
                let rows = accounts
                    .data
                    .iter()
                    .map(|entry| {
                        vec![
                            entry.account.id.to_string(),
                            entry.account.name.clone(),
                            entry.account.status.clone(),
                            entry.roles.join(", "),
                            entry
                                .account
                                .token_hint
                                .clone()
                                .unwrap_or_else(|| "-".into()),
                        ]
                    })
                    .collect();
                print_table(&["ID", "Name", "Status", "Roles", "Token Hint"], rows);
                print_next_cursor(accounts.meta.next_cursor.as_deref());
            }
        }
        IamServiceAccountCmd::Create {
            name,
            description,
            roles,
        } => {
            let payload = CreateServiceAccountPayload {
                name: name.clone(),
                description: description.clone(),
                status: Some("active".into()),
                roles: roles.clone(),
            };
            let record: ServiceAccountWithToken = client
                .post("/api/v1/iam/service-accounts", &payload)
                .await?;
            if json {
                print_json(&record)?;
            } else {
                println!("Created service account {}", record.account.name);
                println!("Token: {}", record.token);
                println!("Copy this token now; it will not be shown again.");
            }
        }
        IamServiceAccountCmd::Rotate { id, roles } => {
            let payload = RotateServiceAccountPayload {
                roles: if roles.is_empty() {
                    None
                } else {
                    Some(roles.clone())
                },
            };
            let record: ServiceAccountWithToken = client
                .post(
                    &format!("/api/v1/iam/service-accounts/{id}/rotate"),
                    &payload,
                )
                .await?;
            if json {
                print_json(&record)?;
            } else {
                println!("Rotated service account {}", record.account.name);
                println!("New token: {}", record.token);
            }
        }
        IamServiceAccountCmd::Disable { id } => {
            client
                .delete(&format!("/api/v1/iam/service-accounts/{id}"))
                .await?;
            println!("Disabled service account {id}");
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
    if scope_type != "domain" {
        return Err(anyhow!("scope type must be 'domain'"));
    }
    Ok((scope_type, scope_value))
}

fn render_traffic_report(report: &TrafficReportResponse) {
    println!(
        "Traffic range: {} (bucket interval {})",
        report.range, report.bucket_interval
    );
    println!("Allow/Block Trend:");
    for series in &report.allow_block_trend {
        let total: i64 = series.buckets.iter().map(|b| b.doc_count).sum();
        let sample = series
            .buckets
            .iter()
            .rev()
            .take(3)
            .map(|bucket| format!("{}:{}", bucket.key_as_string, bucket.doc_count))
            .collect::<Vec<_>>()
            .join(", ");
        println!("- {:<8} total {:<6} ({})", series.action, total, sample);
    }
    println!("\nTop Blocked Domains:");
    let domain_rows = report
        .top_blocked_domains
        .iter()
        .map(|entry| vec![entry.key.clone(), entry.doc_count.to_string()])
        .collect();
    print_table(&["Domain", "Events"], domain_rows);

    println!("\nTop Categories:");
    let category_rows = report
        .top_categories
        .iter()
        .map(|entry| vec![entry.key.clone(), entry.doc_count.to_string()])
        .collect();
    print_table(&["Category", "Events"], category_rows);
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

fn print_next_cursor(cursor: Option<&str>) {
    if let Some(value) = cursor {
        println!("next_cursor: {}", value);
    }
}

fn render_page_content(record: &PageContentRecord) {
    println!("Normalized Key : {}", record.normalized_key);
    println!("Fetch Version  : {}", record.fetch_version);
    println!("Status         : {}", record.fetch_status);
    println!(
        "Reason         : {}",
        record.fetch_reason.as_deref().unwrap_or("-")
    );
    println!("Fetched At     : {}", record.fetched_at);
    println!("Expires At     : {}", record.expires_at);
    println!("TTL (seconds)  : {}", record.ttl_seconds);
    println!(
        "Content Type   : {}",
        record.content_type.as_deref().unwrap_or("-")
    );
    println!(
        "Content Hash   : {}",
        record.content_hash.as_deref().unwrap_or("-")
    );
    let char_text = record
        .char_count
        .map(|v| v.to_string())
        .unwrap_or_else(|| "-".into());
    let byte_text = record
        .byte_count
        .map(|v| v.to_string())
        .unwrap_or_else(|| "-".into());
    println!("Chars / Bytes  : {char_text} / {byte_text}");
    match &record.excerpt {
        Some(text) => {
            if record.excerpt_truncated {
                println!("Excerpt (truncated):\n{}\n", text);
            } else {
                println!("Excerpt:\n{}\n", text);
            }
        }
        None => println!("Excerpt        : (none cached)"),
    }
}

async fn run_smoke(profile: &str) -> Result<()> {
    info!(profile = profile, "running smoke profile");
    let target = match profile {
        "prod" => "icap.prod:1344",
        "staging" => "127.0.0.1:1344",
        "compose" => "icap-adaptor:1344",
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

    async fn delete_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let req = self.http.delete(url);
        let resp = self.send(req).await?;
        Ok(resp.json().await?)
    }

    async fn send(&self, req: reqwest::RequestBuilder) -> Result<reqwest::Response> {
        let mut builder = req;
        if let Some(token) = &self.token {
            if looks_like_jwt(token) {
                builder = builder.header(header::AUTHORIZATION, format!("Bearer {}", token));
            } else {
                builder = builder.header("X-Admin-Token", token);
            }
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

fn looks_like_jwt(token: &str) -> bool {
    let parts: Vec<&str> = token.split('.').collect();
    parts.len() == 3 && parts.iter().all(|segment| !segment.is_empty())
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
struct CursorPaged<T> {
    data: Vec<T>,
    meta: CursorMeta,
}

#[derive(Debug, Deserialize, Serialize)]
struct CursorMeta {
    limit: u32,
    has_more: bool,
    next_cursor: Option<String>,
    prev_cursor: Option<String>,
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
struct CacheEntryRecord {
    cache_key: String,
    value: serde_json::Value,
    expires_at: String,
    source: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct PageContentRecord {
    normalized_key: String,
    fetch_version: i64,
    content_type: Option<String>,
    content_hash: Option<String>,
    char_count: Option<i32>,
    byte_count: Option<i32>,
    fetch_status: String,
    fetch_reason: Option<String>,
    ttl_seconds: i32,
    fetched_at: String,
    expires_at: String,
    excerpt: Option<String>,
    excerpt_truncated: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct PageContentSummary {
    fetch_version: i64,
    fetch_status: String,
    fetch_reason: Option<String>,
    ttl_seconds: i32,
    fetched_at: String,
    expires_at: String,
    char_count: Option<i32>,
    byte_count: Option<i32>,
    content_hash: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct PendingClassificationRecord {
    normalized_key: String,
    status: String,
    base_url: Option<String>,
    last_error: Option<String>,
    requested_at: String,
    updated_at: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ManualClassificationResponse {
    normalized_key: String,
    primary_category: String,
    subcategory: String,
    risk_level: String,
    recommended_action: PolicyAction,
    confidence: f32,
    updated_at: String,
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
struct LlmProviderSummary {
    name: String,
    provider_type: String,
    endpoint: String,
    role: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct TrafficReportResponse {
    range: String,
    bucket_interval: String,
    allow_block_trend: Vec<ActionSeriesResponse>,
    top_blocked_domains: Vec<TopEntryResponse>,
    top_categories: Vec<TopEntryResponse>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ActionSeriesResponse {
    action: String,
    buckets: Vec<TimeBucketResponse>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TimeBucketResponse {
    key_as_string: String,
    doc_count: i64,
}

#[derive(Debug, Deserialize, Serialize)]
struct TopEntryResponse {
    key: String,
    doc_count: i64,
}

#[derive(Debug, Deserialize, Serialize)]
struct CliLogRecord {
    id: Uuid,
    operator_id: Option<String>,
    command: String,
    result: Option<String>,
    created_at: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct IamUserRecord {
    id: Uuid,
    subject: Option<String>,
    email: String,
    display_name: Option<String>,
    status: String,
    last_login_at: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct IamGroupRecord {
    id: Uuid,
    name: String,
    description: Option<String>,
    status: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct IamUserDetails {
    user: IamUserRecord,
    roles: Vec<String>,
    groups: Vec<IamGroupRecord>,
}

#[derive(Debug, Deserialize, Serialize)]
struct IamGroupDetails {
    group: IamGroupRecord,
    members: Vec<IamUserRecord>,
    roles: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct IamRoleRecord {
    id: Uuid,
    name: String,
    description: Option<String>,
    builtin: bool,
    created_at: String,
    permissions: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ServiceAccountRecord {
    id: Uuid,
    name: String,
    description: Option<String>,
    status: String,
    token_hint: Option<String>,
    created_at: String,
    last_rotated_at: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ServiceAccountDetails {
    account: ServiceAccountRecord,
    roles: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ServiceAccountWithToken {
    account: ServiceAccountRecord,
    token: String,
    roles: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct WhoAmIResponse {
    actor: String,
    principal_type: String,
    principal_id: Option<Uuid>,
    roles: Vec<String>,
    permissions: Vec<String>,
}

#[derive(Serialize)]
struct DeviceCodeRequest<'a> {
    client_id: &'a str,
    scope: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    audience: Option<&'a str>,
}

#[derive(Serialize)]
struct DeviceTokenRequest<'a> {
    client_id: &'a str,
    device_code: &'a str,
    grant_type: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    audience: Option<&'a str>,
}

#[derive(Deserialize)]
struct DeviceErrorResponse {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

#[derive(Serialize)]
struct CreateUserPayload {
    email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    subject: Option<String>,
    status: String,
}

#[derive(Serialize)]
struct RoleChangePayload {
    role: String,
}

#[derive(Serialize)]
struct CreateGroupPayload {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

#[derive(Serialize)]
struct AddGroupMemberPayload {
    user_id: String,
}

#[derive(Serialize)]
struct CreateServiceAccountPayload {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    #[serde(default)]
    roles: Vec<String>,
}

#[derive(Serialize)]
struct RotateServiceAccountPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    roles: Option<Vec<String>>,
}
