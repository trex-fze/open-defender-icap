mod auth;
mod cache;

use anyhow::{Context, Result};
use auth::{enforce_admin, AdminAuth};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    middleware,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::types::chrono::{DateTime, Utc};
use sqlx::{
    postgres::{PgPoolOptions, PgRow},
    PgPool, Row,
};
use std::{env, net::IpAddr, sync::Arc};
use tokio::net::TcpListener;
use tracing::{error, info, warn, Level};
use uuid::Uuid;

use cache::CacheInvalidator;

#[derive(Debug, Deserialize)]
struct AdminApiConfig {
    pub host: String,
    pub port: u16,
    pub database_url: Option<String>,
    pub admin_token: Option<String>,
    pub redis_url: Option<String>,
    pub cache_channel: Option<String>,
}

#[derive(Clone)]
struct AppState {
    pool: PgPool,
    cache_invalidator: Option<Arc<CacheInvalidator>>,
}

impl AppState {
    async fn invalidate_override(&self, scope_type: &str, scope_value: &str) {
        if let Some(cache) = &self.cache_invalidator {
            if let Err(err) = cache.invalidate_override(scope_type, scope_value).await {
                error!(
                    target = "svc-admin",
                    %err,
                    scope_type,
                    scope_value,
                    "failed to invalidate cache for override"
                );
            }
        }
    }

    async fn invalidate_review(&self, normalized_key: &str) {
        if let Some(cache) = &self.cache_invalidator {
            if let Err(err) = cache.invalidate_review(normalized_key).await {
                error!(
                    target = "svc-admin",
                    %err,
                    normalized_key,
                    "failed to invalidate cache for review update"
                );
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    let cfg: AdminApiConfig = config_core::load_config("config/admin-api.json")?;
    let db_url = cfg
        .database_url
        .clone()
        .or_else(|| env::var("OD_ADMIN_DATABASE_URL").ok())
        .or_else(|| env::var("DATABASE_URL").ok())
        .context("database_url required: set config/admin-api.json or OD_ADMIN_DATABASE_URL / DATABASE_URL")?;
    let admin_token = cfg
        .admin_token
        .clone()
        .or_else(|| env::var("OD_ADMIN_TOKEN").ok());
    let redis_url = cfg
        .redis_url
        .clone()
        .or_else(|| env::var("OD_CACHE_REDIS_URL").ok());
    let cache_channel = cfg
        .cache_channel
        .clone()
        .or_else(|| env::var("OD_CACHE_CHANNEL").ok());
    let cache_invalidator = redis_url
        .as_ref()
        .map(|url| CacheInvalidator::new(url.clone(), cache_channel.clone()))
        .transpose()
        .context("failed to initialize cache invalidator")?
        .map(Arc::new);

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    if redis_url.is_none() {
        warn!(
            target = "svc-admin",
            "cache invalidation disabled (redis_url not configured)"
        );
    } else if let Some(ref inv) = cache_invalidator {
        info!(
            target = "svc-admin",
            channel = inv.channel_name(),
            "cache invalidation enabled"
        );
    }

    let state = AppState {
        pool: pool.clone(),
        cache_invalidator,
    };

    let admin_routes = Router::new()
        .route(
            "/api/v1/overrides",
            get(list_overrides).post(create_override),
        )
        .route(
            "/api/v1/overrides/:id",
            delete(delete_override).put(update_override),
        )
        .route("/api/v1/review-queue", get(list_review_queue))
        .route(
            "/api/v1/review-queue/:id/resolve",
            post(resolve_review_item),
        )
        .with_state(state.clone())
        .layer(middleware::from_fn_with_state(
            AdminAuth { token: admin_token },
            enforce_admin,
        ));

    let app = Router::new()
        .route("/health/ready", get(health))
        .route("/health/live", get(health))
        .with_state(state)
        .merge(admin_routes);

    let addr = format!("{}:{}", cfg.host, cfg.port);
    let listener = TcpListener::bind(&addr).await?;
    info!(target = "svc-admin", %addr, "admin api listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> &'static str {
    "OK"
}

async fn list_overrides(
    State(state): State<AppState>,
) -> Result<Json<Vec<OverrideRecord>>, StatusCode> {
    let rows = sqlx::query(
        r#"SELECT id, scope_type, scope_value, action, reason, created_by, expires_at, status, created_at, updated_at
        FROM overrides ORDER BY created_at DESC LIMIT 200"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|err| {
        error!(target = "svc-admin", %err, "list_overrides failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut overrides = Vec::with_capacity(rows.len());
    for row in rows {
        overrides.push(map_override_row(&row).map_err(|err| {
            error!(target = "svc-admin", %err, "failed to map override row");
            StatusCode::INTERNAL_SERVER_ERROR
        })?);
    }

    Ok(Json(overrides))
}

async fn create_override(
    State(state): State<AppState>,
    Json(payload): Json<OverrideUpsertRequest>,
) -> Result<Json<OverrideRecord>, (StatusCode, Json<ApiError>)> {
    let validated = validate_override_payload(payload)?;
    let ValidatedOverridePayload {
        scope_type,
        scope_value,
        action,
        reason,
        created_by,
        expires_at,
        status,
    } = validated;

    let id = Uuid::new_v4();
    let status_value = status.unwrap_or_else(|| "active".to_string());
    sqlx::query(
        r#"INSERT INTO overrides (id, scope_type, scope_value, action, reason, created_by, expires_at, status)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
    )
    .bind(id)
    .bind(&scope_type)
    .bind(&scope_value)
    .bind(&action)
    .bind(&reason)
    .bind(&created_by)
    .bind(expires_at)
    .bind(&status_value)
    .execute(&state.pool)
    .await
    .map_err(|err| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError::new("DB_ERROR", err.to_string())),
    ))?;

    let record = sqlx::query(
        r#"SELECT id, scope_type, scope_value, action, reason, created_by, expires_at, status, created_at, updated_at
        FROM overrides WHERE id = $1"#,
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await
    .map_err(|err| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError::new("DB_ERROR", err.to_string())),
    ))?;

    let mapped = map_override_row(&record).map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("DB_ERROR", err.to_string())),
        )
    })?;

    state
        .invalidate_override(&mapped.scope_type, &mapped.scope_value)
        .await;

    Ok(Json(mapped))
}

async fn delete_override(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let deleted =
        sqlx::query("DELETE FROM overrides WHERE id = $1 RETURNING scope_type, scope_value")
            .bind(id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|err| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError::new("DB_ERROR", err.to_string())),
                )
            })?;

    let Some(row) = deleted else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::new("NOT_FOUND", "override not found")),
        ));
    };

    let scope_type: String = row.try_get("scope_type").map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("DB_ERROR", err.to_string())),
        )
    })?;
    let scope_value: String = row.try_get("scope_value").map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("DB_ERROR", err.to_string())),
        )
    })?;

    state.invalidate_override(&scope_type, &scope_value).await;

    Ok(())
}

async fn update_override(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<OverrideUpsertRequest>,
) -> Result<Json<OverrideRecord>, (StatusCode, Json<ApiError>)> {
    let validated = validate_override_payload(payload)?;
    let ValidatedOverridePayload {
        scope_type,
        scope_value,
        action,
        reason,
        created_by,
        expires_at,
        status,
    } = validated;

    let row = sqlx::query(
        r#"UPDATE overrides
            SET scope_type = $1,
                scope_value = $2,
                action = $3,
                reason = $4,
                created_by = $5,
                expires_at = $6,
                status = COALESCE($7, status),
                updated_at = NOW()
          WHERE id = $8
          RETURNING id, scope_type, scope_value, action, reason, created_by, expires_at, status, created_at, updated_at"#,
    )
    .bind(&scope_type)
    .bind(&scope_value)
    .bind(&action)
    .bind(&reason)
    .bind(&created_by)
    .bind(expires_at)
    .bind(&status)
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|err| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError::new("DB_ERROR", err.to_string())),
    ))?;

    let Some(row) = row else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::new("NOT_FOUND", "override not found")),
        ));
    };

    let mapped = map_override_row(&row).map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("DB_ERROR", err.to_string())),
        )
    })?;

    state
        .invalidate_override(&mapped.scope_type, &mapped.scope_value)
        .await;

    Ok(Json(mapped))
}

async fn list_review_queue(
    State(state): State<AppState>,
) -> Result<Json<Vec<ReviewRecord>>, StatusCode> {
    let rows = sqlx::query(
        r#"SELECT id, normalized_key, request_metadata, status, submitter, assigned_to, decided_by, decision_notes, decision_action, created_at, updated_at
        FROM review_queue ORDER BY created_at DESC LIMIT 200"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|err| {
        error!(target = "svc-admin", %err, "list_review_queue failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut records = Vec::with_capacity(rows.len());
    for row in rows {
        records.push(map_review_row(&row).map_err(|err| {
            error!(target = "svc-admin", %err, "failed to map review row");
            StatusCode::INTERNAL_SERVER_ERROR
        })?);
    }

    Ok(Json(records))
}

async fn resolve_review_item(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<ReviewResolveRequest>,
) -> Result<Json<ReviewRecord>, (StatusCode, Json<ApiError>)> {
    if payload.status.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new("VALIDATION_ERROR", "status required")),
        ));
    }

    let rows = sqlx::query(
        r#"UPDATE review_queue
            SET status = $1,
                decided_by = $2,
                decision_notes = $3,
                decision_action = $4,
                updated_at = NOW()
          WHERE id = $5
          RETURNING id, normalized_key, request_metadata, status, submitter, assigned_to, decided_by, decision_notes, decision_action, created_at, updated_at"#,
    )
    .bind(&payload.status)
    .bind(&payload.decided_by)
    .bind(&payload.decision_notes)
    .bind(&payload.decision_action)
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|err| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError::new("DB_ERROR", err.to_string())),
    ))?;

    let Some(row) = rows else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::new("NOT_FOUND", "review item not found")),
        ));
    };

    let mapped = map_review_row(&row).map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("DB_ERROR", err.to_string())),
        )
    })?;

    state.invalidate_review(&mapped.normalized_key).await;

    Ok(Json(mapped))
}

#[derive(Debug, Serialize)]
struct ApiError {
    error_code: &'static str,
    message: String,
}

impl ApiError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            error_code: code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct OverrideUpsertRequest {
    scope_type: String,
    scope_value: String,
    action: String,
    reason: Option<String>,
    created_by: Option<String>,
    expires_at: Option<DateTime<Utc>>,
    status: Option<String>,
}

#[derive(Debug, Serialize)]
struct OverrideRecord {
    id: Uuid,
    scope_type: String,
    scope_value: String,
    action: String,
    reason: Option<String>,
    created_by: Option<String>,
    expires_at: Option<DateTime<Utc>>,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct ReviewRecord {
    id: Uuid,
    normalized_key: String,
    request_metadata: Value,
    status: String,
    submitter: Option<String>,
    assigned_to: Option<String>,
    decided_by: Option<String>,
    decision_notes: Option<String>,
    decision_action: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct ReviewResolveRequest {
    status: String,
    decided_by: Option<String>,
    decision_notes: Option<String>,
    decision_action: Option<String>,
}

struct ValidatedOverridePayload {
    scope_type: String,
    scope_value: String,
    action: String,
    reason: Option<String>,
    created_by: Option<String>,
    expires_at: Option<DateTime<Utc>>,
    status: Option<String>,
}

const ALLOWED_SCOPE_TYPES: &[&str] = &["domain", "user", "ip"];
const ALLOWED_ACTIONS: &[&str] = &[
    "allow",
    "block",
    "warn",
    "monitor",
    "review",
    "require-approval",
];
const ALLOWED_STATUSES: &[&str] = &["active", "inactive", "expired", "revoked"];

fn validate_override_payload(
    payload: OverrideUpsertRequest,
) -> Result<ValidatedOverridePayload, (StatusCode, Json<ApiError>)> {
    let scope_type = normalize_scope_type(&payload.scope_type)?;
    let scope_value = normalize_scope_value(&scope_type, &payload.scope_value)?;
    let action = normalize_action(&payload.action)?;
    let reason = normalize_optional_field(payload.reason);
    let created_by = normalize_optional_field(payload.created_by);
    let expires_at = payload.expires_at;
    let status = match payload.status {
        Some(value) => Some(normalize_status(&value)?),
        None => None,
    };

    Ok(ValidatedOverridePayload {
        scope_type,
        scope_value,
        action,
        reason,
        created_by,
        expires_at,
        status,
    })
}

fn normalize_scope_type(value: &str) -> Result<String, (StatusCode, Json<ApiError>)> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(validation_error("scope_type required"));
    }
    if ALLOWED_SCOPE_TYPES.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        Err(validation_error("scope_type must be one of domain|user|ip"))
    }
}

fn normalize_scope_value(
    scope_type: &str,
    value: &str,
) -> Result<String, (StatusCode, Json<ApiError>)> {
    match scope_type {
        "domain" => normalize_domain_scope(value),
        "user" => normalize_user_scope(value),
        "ip" => normalize_ip_scope(value),
        _ => Err(validation_error("unsupported scope_type")),
    }
}

fn normalize_domain_scope(value: &str) -> Result<String, (StatusCode, Json<ApiError>)> {
    let lowered = value.trim().to_ascii_lowercase();
    if lowered.is_empty() {
        return Err(validation_error(
            "scope_value required for domain overrides",
        ));
    }

    let (wildcard_prefix, domain_part) = if let Some(rest) = lowered.strip_prefix("*.") {
        ("*.", rest)
    } else {
        ("", lowered.as_str())
    };

    if domain_part.is_empty() || !domain_part.contains('.') {
        return Err(validation_error(
            "domain scope must include a valid hostname",
        ));
    }

    if domain_part.len() > 253 {
        return Err(validation_error("domain scope exceeds maximum length"));
    }

    if !domain_part
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
    {
        return Err(validation_error(
            "domain scope may only include alphanumeric, '-', '.' characters",
        ));
    }

    Ok(format!("{}{}", wildcard_prefix, domain_part))
}

fn normalize_user_scope(value: &str) -> Result<String, (StatusCode, Json<ApiError>)> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(validation_error("scope_value required for user overrides"));
    }
    if trimmed.len() > 256 {
        return Err(validation_error("user scope exceeds maximum length"));
    }
    if trimmed.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(validation_error(
            "user scope cannot contain whitespace or control characters",
        ));
    }
    Ok(trimmed.to_string())
}

fn normalize_ip_scope(value: &str) -> Result<String, (StatusCode, Json<ApiError>)> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(validation_error("scope_value required for ip overrides"));
    }
    let parsed: IpAddr = trimmed
        .parse()
        .map_err(|_| validation_error("scope_value must be a valid IP address"))?;
    Ok(parsed.to_string())
}

fn normalize_action(value: &str) -> Result<String, (StatusCode, Json<ApiError>)> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(validation_error("action required"));
    }
    if ALLOWED_ACTIONS.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        Err(validation_error(
            "action must be one of allow|block|warn|monitor|review|require-approval",
        ))
    }
}

fn normalize_status(value: &str) -> Result<String, (StatusCode, Json<ApiError>)> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(validation_error("status cannot be empty"));
    }
    if ALLOWED_STATUSES.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        Err(validation_error(
            "status must be one of active|inactive|expired|revoked",
        ))
    }
}

fn normalize_optional_field(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let trimmed = v.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn validation_error(message: &str) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError::new("VALIDATION_ERROR", message.to_string())),
    )
}

fn map_override_row(row: &PgRow) -> sqlx::Result<OverrideRecord> {
    Ok(OverrideRecord {
        id: row.try_get("id")?,
        scope_type: row.try_get("scope_type")?,
        scope_value: row.try_get("scope_value")?,
        action: row.try_get("action")?,
        reason: row.try_get("reason")?,
        created_by: row.try_get("created_by")?,
        expires_at: row.try_get("expires_at")?,
        status: row.try_get("status")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn map_review_row(row: &PgRow) -> sqlx::Result<ReviewRecord> {
    Ok(ReviewRecord {
        id: row.try_get("id")?,
        normalized_key: row.try_get("normalized_key")?,
        request_metadata: row.try_get("request_metadata")?,
        status: row.try_get("status")?,
        submitter: row.try_get("submitter")?,
        assigned_to: row.try_get("assigned_to")?,
        decided_by: row.try_get("decided_by")?,
        decision_notes: row.try_get("decision_notes")?,
        decision_action: row.try_get("decision_action")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
