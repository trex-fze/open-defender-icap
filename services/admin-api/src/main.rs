mod audit;
mod auth;
mod cache;
mod cache_entries_api;
mod classification_requests;
mod cli_logs;
mod metrics;
mod page_contents;
mod pagination;
mod policies;
mod reporting;
mod reporting_es;
mod taxonomy;

use anyhow::{Context, Result};
use audit::{AuditEvent, AuditLogger, ElasticExporter};
use auth::{
    enforce_admin, require_roles, AdminAuth, AuthSettings, UserContext, ROLE_OVERRIDES_DELETE,
    ROLE_OVERRIDES_VIEW, ROLE_OVERRIDES_WRITE, ROLE_REVIEW_RESOLVE, ROLE_REVIEW_VIEW,
};
use axum::{
    extract::{Path, State},
    http::{header, HeaderValue, Method, Request, StatusCode},
    middleware,
    response::Response,
    routing::{delete, get, post, put},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{self, Value};
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
use metrics::ReviewMetrics;
use reporting_es::{ElasticReportingClient, ReportingConfig};

#[derive(Debug, Clone, Deserialize)]
struct AdminApiConfig {
    pub host: String,
    pub port: u16,
    pub database_url: Option<String>,
    pub admin_token: Option<String>,
    pub redis_url: Option<String>,
    pub cache_channel: Option<String>,
    #[serde(default)]
    pub auth: AuthSettings,
    #[serde(default)]
    pub audit: AuditExportConfig,
    #[serde(default)]
    pub metrics: MetricsConfig,
    #[serde(default)]
    pub reporting: ReportingConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct AuditExportConfig {
    pub elastic_url: Option<String>,
    pub index: Option<String>,
    pub api_key: Option<String>,
}

impl AuditExportConfig {
    fn merge_env(mut self) -> Self {
        if let Ok(url) = env::var("OD_AUDIT_ELASTIC_URL") {
            self.elastic_url = Some(url);
        }
        if let Ok(index) = env::var("OD_AUDIT_ELASTIC_INDEX") {
            self.index = Some(index);
        }
        if let Ok(key) = env::var("OD_AUDIT_ELASTIC_API_KEY") {
            self.api_key = Some(key);
        }
        self
    }
}

#[derive(Debug, Clone, Deserialize)]
struct MetricsConfig {
    #[serde(default = "default_review_sla_seconds")]
    pub review_sla_seconds: u64,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            review_sla_seconds: default_review_sla_seconds(),
        }
    }
}

impl MetricsConfig {
    fn merge_env(mut self) -> Self {
        if let Ok(value) = env::var("OD_REVIEW_SLA_SECONDS") {
            if let Ok(parsed) = value.parse::<u64>() {
                self.review_sla_seconds = parsed;
            }
        }
        self
    }
}

fn default_review_sla_seconds() -> u64 {
    4 * 60 * 60
}

#[derive(Clone)]
pub struct AppState {
    pool: PgPool,
    cache_invalidator: Option<Arc<CacheInvalidator>>,
    audit_logger: AuditLogger,
    metrics: ReviewMetrics,
    reporting_client: Option<ElasticReportingClient>,
}

impl AppState {
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

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

    pub async fn invalidate_policy_cache(&self) {
        if let Some(cache) = &self.cache_invalidator {
            if let Err(err) = cache.invalidate_policy().await {
                warn!(target = "svc-admin", %err, "failed to invalidate caches after policy change");
            }
        }
    }

    pub async fn invalidate_cache_key(&self, cache_key: &str) {
        if let Some(cache) = &self.cache_invalidator {
            if let Err(err) = cache.invalidate_key(cache_key).await {
                warn!(
                    target = "svc-admin",
                    %err,
                    cache_key,
                    "failed to purge cache entry"
                );
            }
        }
    }

    async fn log_override_event<T>(
        &self,
        action: &str,
        actor: Option<String>,
        target_id: String,
        payload: T,
    ) where
        T: serde::Serialize,
    {
        let payload = serde_json::to_value(payload).ok();
        self.audit_logger
            .log(AuditEvent {
                actor,
                action: action.to_string(),
                target_type: Some("override".into()),
                target_id: Some(target_id),
                payload,
            })
            .await;
    }

    async fn log_review_event<T>(
        &self,
        action: &str,
        actor: Option<String>,
        target_id: String,
        payload: T,
    ) where
        T: serde::Serialize,
    {
        let payload = serde_json::to_value(payload).ok();
        self.audit_logger
            .log(AuditEvent {
                actor,
                action: action.to_string(),
                target_type: Some("review".into()),
                target_id: Some(target_id),
                payload,
            })
            .await;
    }

    pub async fn log_policy_event<T>(
        &self,
        action: &str,
        actor: Option<String>,
        target_id: Option<String>,
        payload: T,
    ) where
        T: serde::Serialize,
    {
        let payload = serde_json::to_value(payload).ok();
        self.audit_logger
            .log(AuditEvent {
                actor,
                action: action.to_string(),
                target_type: Some("policy".into()),
                target_id,
                payload,
            })
            .await;
    }

    pub fn reporting_client(&self) -> Option<&ElasticReportingClient> {
        self.reporting_client.as_ref()
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
    let admin_auth = Arc::new(
        AdminAuth::from_config(admin_token.clone(), cfg.auth.clone())
            .await
            .context("failed to initialize auth")?,
    );

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    let audit_cfg = cfg.audit.clone().merge_env();
    let metrics_cfg = cfg.metrics.clone().merge_env();
    let reporting_cfg = cfg.reporting.clone().merge_env();

    let elastic_exporter = if let (Some(url), Some(index)) =
        (audit_cfg.elastic_url.clone(), audit_cfg.index.clone())
    {
        Some(
            ElasticExporter::new(url, index, audit_cfg.api_key.clone())
                .context("failed to initialize elastic exporter")?,
        )
    } else {
        None
    };

    let review_metrics = ReviewMetrics::new(metrics_cfg.review_sla_seconds);
    if let Err(err) = review_metrics.sync_from_db(&pool).await {
        warn!(target = "svc-admin", %err, "failed to initialize review metrics gauge");
    }

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

    let reporting_client = ElasticReportingClient::from_config(&reporting_cfg)
        .context("failed to initialize reporting client")?;

    let state = AppState {
        pool: pool.clone(),
        cache_invalidator,
        audit_logger: AuditLogger::new(pool.clone(), elastic_exporter),
        metrics: review_metrics,
        reporting_client,
    };

    let auth_layer = {
        let auth = admin_auth.clone();
        middleware::from_fn(move |req, next| {
            let auth = auth.clone();
            async move { enforce_admin(auth, req, next).await }
        })
    };

    let cors_allow_origin = env::var("OD_ADMIN_CORS_ALLOW_ORIGIN")
        .unwrap_or_else(|_| "http://localhost:19001".to_string());
    let cors_layer = {
        let allow_origin = cors_allow_origin.clone();
        middleware::from_fn(move |req, next| {
            let allow_origin = allow_origin.clone();
            async move { cors_middleware(req, next, &allow_origin).await }
        })
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
        .route(
            "/api/v1/policies",
            get(policies::list_policies).post(policies::create_policy),
        )
        .route("/api/v1/policies/validate", post(policies::validate_policy))
        .route(
            "/api/v1/policies/:id",
            get(policies::get_policy).put(policies::update_policy),
        )
        .route(
            "/api/v1/policies/:id/publish",
            post(policies::publish_policy),
        )
        .route(
            "/api/v1/taxonomy/categories",
            get(taxonomy::list_categories).post(taxonomy::create_category),
        )
        .route(
            "/api/v1/taxonomy/categories/:id",
            put(taxonomy::update_category).delete(taxonomy::delete_category),
        )
        .route(
            "/api/v1/taxonomy/subcategories",
            get(taxonomy::list_subcategories).post(taxonomy::create_subcategory),
        )
        .route(
            "/api/v1/taxonomy/subcategories/:id",
            put(taxonomy::update_subcategory).delete(taxonomy::delete_subcategory),
        )
        .route(
            "/api/v1/reporting/aggregates",
            get(reporting::list_aggregates),
        )
        .route("/api/v1/reporting/traffic", get(reporting::traffic_summary))
        .route(
            "/api/v1/cache-entries/:cache_key",
            get(cache_entries_api::get_entry).delete(cache_entries_api::delete_entry),
        )
        .route("/api/v1/cli-logs", get(cli_logs::list_cli_logs))
        .route(
            "/api/v1/classifications/pending",
            get(classification_requests::list_pending),
        )
        .route(
            "/api/v1/classifications/:normalized_key/unblock",
            post(classification_requests::manual_unblock),
        )
        .route(
            "/api/v1/page-contents/:normalized_key",
            get(page_contents::get_page_content),
        )
        .route(
            "/api/v1/page-contents/:normalized_key/history",
            get(page_contents::list_page_content_history),
        )
        .with_state(state.clone())
        .layer(auth_layer);

    let app = Router::new()
        .route("/health/ready", get(health))
        .route("/health/live", get(health))
        .route("/metrics", get(metrics_endpoint))
        .with_state(state)
        .merge(admin_routes)
        .layer(cors_layer);

    let addr = format!("{}:{}", cfg.host, cfg.port);
    let listener = TcpListener::bind(&addr).await?;
    info!(target = "svc-admin", %addr, "admin api listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn cors_middleware(
    req: Request<axum::body::Body>,
    next: middleware::Next,
    allow_origin: &str,
) -> Response {
    let origin = req.headers().get(header::ORIGIN).cloned();

    if req.method() == Method::OPTIONS {
        let mut response = Response::new(axum::body::Body::empty());
        *response.status_mut() = StatusCode::NO_CONTENT;
        apply_cors_headers(response, origin.as_ref(), allow_origin)
    } else {
        let response = next.run(req).await;
        apply_cors_headers(response, origin.as_ref(), allow_origin)
    }
}

fn apply_cors_headers(
    mut response: Response,
    request_origin: Option<&HeaderValue>,
    allow_origin: &str,
) -> Response {
    let allow_origin_header = if allow_origin == "*" {
        Some(HeaderValue::from_static("*"))
    } else {
        request_origin.and_then(|origin| {
            if origin == allow_origin {
                Some(origin.clone())
            } else {
                None
            }
        })
    };

    if let Some(value) = allow_origin_header {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET,POST,PUT,DELETE,OPTIONS"),
        );
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("Authorization,Content-Type,X-Admin-Token"),
        );
    }

    response
}

async fn health() -> &'static str {
    "OK"
}

async fn metrics_endpoint(
    State(state): State<AppState>,
) -> Result<(StatusCode, String), StatusCode> {
    if let Err(err) = state.metrics.sync_from_db(&state.pool).await {
        error!(target = "svc-admin", %err, "failed to sync review metrics gauge");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    state
        .metrics
        .render()
        .map(|body| (StatusCode::OK, body))
        .map_err(|err| {
            error!(target = "svc-admin", %err, "failed to render metrics");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn list_overrides(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
) -> Result<Json<Vec<OverrideRecord>>, StatusCode> {
    require_roles(&user, ROLE_OVERRIDES_VIEW)?;
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
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Json(mut payload): Json<OverrideUpsertRequest>,
) -> Result<Json<OverrideRecord>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_OVERRIDES_WRITE)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;
    if payload.created_by.is_none() {
        payload.created_by = Some(user.actor.clone());
    }
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
    state
        .log_override_event(
            "override.create",
            mapped
                .created_by
                .clone()
                .or_else(|| Some(user.actor.clone())),
            mapped.id.to_string(),
            &mapped,
        )
        .await;

    Ok(Json(mapped))
}

async fn delete_override(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_OVERRIDES_DELETE)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;
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
    state
        .log_override_event(
            "override.delete",
            Some(user.actor.clone()),
            id.to_string(),
            serde_json::json!({
                "scope_type": scope_type,
                "scope_value": scope_value
            }),
        )
        .await;

    Ok(())
}

async fn update_override(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(mut payload): Json<OverrideUpsertRequest>,
) -> Result<Json<OverrideRecord>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_OVERRIDES_WRITE)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;
    if payload.created_by.is_none() {
        payload.created_by = Some(user.actor.clone());
    }
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
    state
        .log_override_event(
            "override.update",
            mapped
                .created_by
                .clone()
                .or_else(|| Some(user.actor.clone())),
            mapped.id.to_string(),
            &mapped,
        )
        .await;

    Ok(Json(mapped))
}

async fn list_review_queue(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
) -> Result<Json<Vec<ReviewRecord>>, StatusCode> {
    require_roles(&user, ROLE_REVIEW_VIEW)?;
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
    let pending = records
        .iter()
        .filter(|r| r.status.eq_ignore_ascii_case("pending"))
        .count() as i64;
    state.metrics.set_open_count(pending);

    Ok(Json(records))
}

async fn resolve_review_item(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(mut payload): Json<ReviewResolveRequest>,
) -> Result<Json<ReviewRecord>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_REVIEW_RESOLVE)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;
    if payload.status.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new("VALIDATION_ERROR", "status required")),
        ));
    }
    if payload.decided_by.is_none() {
        payload.decided_by = Some(user.actor.clone());
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
    state
        .log_review_event(
            "review.resolve",
            payload
                .decided_by
                .clone()
                .or_else(|| Some(user.actor.clone())),
            mapped.id.to_string(),
            &mapped,
        )
        .await;
    let duration = mapped
        .updated_at
        .signed_duration_since(mapped.created_at)
        .num_seconds()
        .max(0) as f64;
    state.metrics.record_resolution(duration);
    if let Err(err) = state.metrics.sync_from_db(&state.pool).await {
        warn!(target = "svc-admin", %err, "failed to refresh review metrics gauge");
    }

    Ok(Json(mapped))
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    error_code: &'static str,
    message: String,
}

impl ApiError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            error_code: code,
            message: message.into(),
        }
    }

    pub fn forbidden() -> Self {
        Self::new("FORBIDDEN", "insufficient privileges")
    }

    pub fn message(&self) -> &str {
        &self.message
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

#[derive(Debug)]
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

pub fn validation_error(message: &str) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError::new("VALIDATION_ERROR", message.to_string())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_request() -> OverrideUpsertRequest {
        OverrideUpsertRequest {
            scope_type: "domain".into(),
            scope_value: "example.com".into(),
            action: "block".into(),
            reason: None,
            created_by: Some("tester".into()),
            expires_at: None,
            status: None,
        }
    }

    #[test]
    fn validates_domain_override() {
        let payload = base_request();
        let result = validate_override_payload(payload).unwrap();
        assert_eq!(result.scope_type, "domain");
        assert_eq!(result.scope_value, "example.com");
        assert_eq!(result.action, "block");
    }

    #[test]
    fn rejects_unknown_scope_type() {
        let mut payload = base_request();
        payload.scope_type = "device".into();
        let err = validate_override_payload(payload).unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert!(err.1.message.contains("scope_type"));
    }

    #[test]
    fn normalizes_wildcard_domain() {
        let mut payload = base_request();
        payload.scope_value = "*.Example.com".into();
        let result = validate_override_payload(payload).unwrap();
        assert_eq!(result.scope_value, "*.example.com");
    }

    #[test]
    fn rejects_invalid_ip_scope() {
        let mut payload = base_request();
        payload.scope_type = "ip".into();
        payload.scope_value = "not-an-ip".into();
        let err = validate_override_payload(payload).unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
    }
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
