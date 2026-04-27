mod audit;
mod auth;
mod config;
mod evaluator;
mod models;
mod store;

use anyhow::{Context, Result};
use audit::{PolicyAuditEvent, PolicyAuditLogger};
use auth::{
    enforce_admin, require_roles, AdminAuth, AuthSettings, UserContext, ROLE_POLICY_EDITOR_ROLES,
    ROLE_POLICY_VIEWER_ROLES,
};
use axum::{
    extract::{Path, State},
    http::{HeaderValue, StatusCode},
    middleware,
    response::Response,
    routing::{get, post, put},
    Extension, Json, Router,
};
use evaluator::PolicyEvaluator;
use models::{
    DecisionRequest, ErrorResponse, PolicyCreateRequest, PolicyListResponse, PolicyUpdateRequest,
    SimulatePolicyRequest, SimulationMode, SimulationResponse,
};
use policy_dsl::PolicyDocument;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::{
    collections::{HashMap, HashSet},
    env,
    net::SocketAddr,
    path::Path as FsPath,
    sync::Arc,
};
use store::PolicyStore;
use taxonomy::{ActivationState, TaxonomyStore};
use tokio::net::TcpListener;
use tracing::{debug, error, info, warn, Level};
use uuid::Uuid;

fn check_config_mode_enabled() -> bool {
    std::env::args().any(|arg| arg == "--check-config")
}

fn resolve_policy_db_url(cfg: &config::PolicyConfig) -> (Option<String>, Option<String>) {
    if let Some(url) = cfg.database_url.clone() {
        return (Some(url), None);
    }
    let lookup = config_core::lookup_env("OD_POLICY_DATABASE_URL", &["DATABASE_URL"]);
    (lookup.value, lookup.deprecated_alias)
}

fn resolve_activation_db_url(
    cfg: &config::PolicyConfig,
    policy_db_url: Option<String>,
) -> (Option<String>, Option<String>) {
    if let Some(url) = cfg.activation_database_url.clone() {
        return (Some(url), None);
    }
    let lookup = config_core::lookup_env("OD_TAXONOMY_DATABASE_URL", &["OD_ADMIN_DATABASE_URL"]);
    (lookup.value.or(policy_db_url), lookup.deprecated_alias)
}

fn emit_env_alias_warnings(cfg: &config::PolicyConfig) {
    if cfg.database_url.is_none() {
        if let Some(alias) =
            config_core::lookup_env("OD_POLICY_DATABASE_URL", &["DATABASE_URL"]).deprecated_alias
        {
            eprintln!(
                "warning: policy-engine deprecated env alias in use: {} -> OD_POLICY_DATABASE_URL",
                alias
            );
        }
    }
    if cfg.activation_database_url.is_none() {
        if let Some(alias) =
            config_core::lookup_env("OD_TAXONOMY_DATABASE_URL", &["OD_ADMIN_DATABASE_URL"])
                .deprecated_alias
        {
            eprintln!(
                "warning: policy-engine deprecated env alias in use: {} -> OD_TAXONOMY_DATABASE_URL",
                alias
            );
        }
    }
}

fn validate_config(cfg: &config::PolicyConfig) -> Result<()> {
    let mut validator = config_core::ConfigValidator::new("policy-engine");
    validator.require_non_empty(
        "api_host",
        Some(cfg.api_host.as_str()),
        "set api_host in config/policy-engine.json",
    );
    validator.require_non_empty(
        "policy_file",
        Some(cfg.policy_file.as_str()),
        "set policy_file in config/policy-engine.json",
    );
    if cfg.database_url.is_none() && !FsPath::new(&cfg.policy_file).exists() {
        validator.require_non_empty(
            "policy_file_exists",
            None,
            "create the policy file path configured in policy_file or configure database_url",
        );
    }
    let (policy_db_url, _) = resolve_policy_db_url(cfg);
    if policy_db_url.is_some() {
        validator.require_auth_url(
            "OD_POLICY_DATABASE_URL",
            policy_db_url.as_deref(),
            true,
            true,
            12,
            "set OD_POLICY_DATABASE_URL with non-default username/password credentials",
        );
    }
    let (activation_db_url, _) = resolve_activation_db_url(cfg, policy_db_url.clone());
    if activation_db_url.is_some() {
        validator.require_auth_url(
            "OD_TAXONOMY_DATABASE_URL",
            activation_db_url.as_deref(),
            true,
            true,
            12,
            "set OD_TAXONOMY_DATABASE_URL with non-default username/password credentials",
        );
    }
    validator.finish()
}

#[derive(Debug)]
struct OverrideRecord {
    scope_value: String,
    action: String,
}

#[derive(Clone)]
struct AppState {
    evaluator: Arc<PolicyEvaluator>,
    audit_logger: Option<PolicyAuditLogger>,
    classification_pool: Option<PgPool>,
}

impl AppState {
    fn audit_logger(&self) -> Option<&PolicyAuditLogger> {
        self.audit_logger.as_ref()
    }

    async fn log_event(&self, event: PolicyAuditEvent) {
        if let Some(logger) = self.audit_logger() {
            if let Err(err) = logger.log(event).await {
                error!(target = "svc-policy", %err, "failed to persist policy audit event");
            }
        }
    }

    async fn hydrate_request_hints(&self, request: &mut DecisionRequest) {
        if request.category_hint.is_some()
            && request.subcategory_hint.is_some()
            && request.risk_hint.is_some()
            && request.confidence_hint.is_some()
        {
            return;
        }
        let Some(pool) = &self.classification_pool else {
            return;
        };

        let related = related_classification_lookup(&request.normalized_key);
        let row = sqlx::query(
            r#"SELECT primary_category, subcategory, risk_level, confidence::float8 AS confidence
               FROM classifications
               WHERE status = 'active'
                 AND (
                    normalized_key = $1
                    OR ($2::text IS NOT NULL AND normalized_key = $2)
                    OR ($3::text IS NOT NULL AND normalized_key LIKE $3)
                 )
               ORDER BY
                    CASE
                        WHEN normalized_key = $1 THEN 0
                        WHEN $2::text IS NOT NULL AND normalized_key = $2 THEN 1
                        ELSE 2
                    END,
                    updated_at DESC
               LIMIT 1"#,
        )
        .bind(&request.normalized_key)
        .bind(related.ancestor_domain_key)
        .bind(related.descendant_like_pattern)
        .fetch_optional(pool)
        .await;

        match row {
            Ok(Some(row)) => {
                let hints = PersistedClassificationHints {
                    category_hint: row
                        .try_get::<Option<String>, _>("primary_category")
                        .ok()
                        .flatten()
                        .and_then(non_empty_string),
                    subcategory_hint: row
                        .try_get::<Option<String>, _>("subcategory")
                        .ok()
                        .flatten()
                        .and_then(non_empty_string),
                    risk_hint: row
                        .try_get::<Option<String>, _>("risk_level")
                        .ok()
                        .flatten()
                        .and_then(non_empty_string),
                    confidence_hint: row
                        .try_get::<Option<f64>, _>("confidence")
                        .ok()
                        .flatten()
                        .map(|value| value as f32),
                };
                apply_persisted_hints(request, hints);
                debug!(
                    target = "svc-policy",
                    normalized_key = %request.normalized_key,
                    "hydrated decision request hints from persisted classification"
                );
            }
            Ok(None) => {}
            Err(err) => {
                warn!(
                    target = "svc-policy",
                    %err,
                    normalized_key = %request.normalized_key,
                    "failed to hydrate classification hints"
                );
            }
        }
    }

    async fn resolve_override_decision(
        &self,
        request: &DecisionRequest,
    ) -> Option<common_types::PolicyDecision> {
        let pool = self.classification_pool.as_ref()?;
        let host = host_from_normalized_key(&request.normalized_key)?;

        let rows = sqlx::query(
            r#"SELECT scope_value, action, updated_at, created_at
               FROM overrides
               WHERE scope_type = 'domain'
                 AND status = 'active'
                 AND (expires_at IS NULL OR expires_at > NOW())
               ORDER BY updated_at DESC, created_at DESC"#,
        )
        .fetch_all(pool)
        .await
        .ok()?;

        let mut best: Option<(usize, OverrideRecord)> = None;
        let mut seen_exact_actions: HashMap<String, String> = HashMap::new();
        let mut conflicting_scopes: HashSet<String> = HashSet::new();

        for row in rows {
            let scope_value = row
                .try_get::<String, _>("scope_value")
                .ok()
                .and_then(non_empty_string)?;
            if !domain_scope_matches_host(&scope_value, &host) {
                continue;
            }
            let action = row.try_get::<String, _>("action").ok()?;
            let normalized_action = normalize_override_action(&action);
            if let Some(previous_action) = seen_exact_actions.get(&scope_value) {
                if previous_action != &normalized_action {
                    conflicting_scopes.insert(scope_value.clone());
                }
            } else {
                seen_exact_actions.insert(scope_value.clone(), normalized_action.clone());
            }

            let specificity = domain_scope_specificity(&scope_value);
            let record = OverrideRecord {
                scope_value,
                action,
            };
            match &best {
                Some((best_specificity, _)) if *best_specificity > specificity => {}
                Some((best_specificity, best_record)) if *best_specificity == specificity => {
                    let best_rank = override_action_rank(&best_record.action);
                    let candidate_rank = override_action_rank(&record.action);
                    if candidate_rank > best_rank {
                        best = Some((specificity, record));
                    }
                }
                _ => best = Some((specificity, record)),
            }
        }

        if !conflicting_scopes.is_empty() {
            let mut scopes: Vec<String> = conflicting_scopes.into_iter().collect();
            scopes.sort();
            warn!(
                target = "svc-policy",
                normalized_key = %request.normalized_key,
                conflicting_scope_count = scopes.len(),
                conflicting_scopes = %scopes.join(","),
                "detected conflicting active override actions for exact scopes"
            );
        }

        let (_, matched) = best?;
        let action = match matched.action.trim().to_ascii_lowercase().as_str() {
            "allow" => common_types::PolicyAction::Allow,
            "block" => common_types::PolicyAction::Block,
            _ => return None,
        };
        debug!(
            target = "svc-policy",
            normalized_key = %request.normalized_key,
            scope = %matched.scope_value,
            action = %matched.action,
            "applied domain override before policy evaluation"
        );
        Some(common_types::PolicyDecision {
            action: action.clone(),
            cache_hit: false,
            verdict: Some(common_types::ClassificationVerdict {
                primary_category: "override-manual".into(),
                subcategory: format!("domain:{}", matched.scope_value),
                risk_level: "low".into(),
                confidence: 1.0,
                recommended_action: action,
            }),
            decision_source: Some("override".into()),
        })
    }
}

fn host_from_normalized_key(normalized_key: &str) -> Option<String> {
    let raw = normalized_key
        .strip_prefix("domain:")
        .or_else(|| normalized_key.strip_prefix("subdomain:"))?;
    let host = raw.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

fn normalize_scope_host(scope_value: &str) -> Option<String> {
    let lowered = scope_value.trim().to_ascii_lowercase();
    let core = lowered.strip_prefix("*.").unwrap_or(lowered.as_str());
    let host = core.trim_end_matches('.').to_string();
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

fn domain_scope_matches_host(scope_value: &str, host: &str) -> bool {
    let Some(scope_host) = normalize_scope_host(scope_value) else {
        return false;
    };
    if host == scope_host {
        return true;
    }
    let suffix = format!(".{}", scope_host);
    host.ends_with(&suffix)
}

fn domain_scope_specificity(scope_value: &str) -> usize {
    normalize_scope_host(scope_value)
        .map(|host| host.len())
        .unwrap_or(0)
}

fn normalize_override_action(action: &str) -> String {
    action.trim().to_ascii_lowercase()
}

fn override_action_rank(action: &str) -> u8 {
    match normalize_override_action(action).as_str() {
        "block" => 2,
        "allow" => 1,
        _ => 0,
    }
}

#[derive(Debug, Default)]
struct PersistedClassificationHints {
    category_hint: Option<String>,
    subcategory_hint: Option<String>,
    risk_hint: Option<String>,
    confidence_hint: Option<f32>,
}

fn apply_persisted_hints(request: &mut DecisionRequest, hints: PersistedClassificationHints) {
    if request.category_hint.is_none() {
        request.category_hint = hints.category_hint;
    }
    if request.subcategory_hint.is_none() {
        request.subcategory_hint = hints.subcategory_hint;
    }
    if request.risk_hint.is_none() {
        request.risk_hint = hints.risk_hint;
    }
    if request.confidence_hint.is_none() {
        request.confidence_hint = hints.confidence_hint;
    }
}

fn non_empty_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[derive(Debug, Default)]
struct RelatedClassificationLookup {
    ancestor_domain_key: Option<String>,
    descendant_like_pattern: Option<String>,
}

fn related_classification_lookup(normalized_key: &str) -> RelatedClassificationLookup {
    if let Some(host) = normalized_key.strip_prefix("subdomain:") {
        if let Some(registered) = derive_registered_domain(host) {
            let ancestor = format!("domain:{}", registered);
            if ancestor != normalized_key {
                return RelatedClassificationLookup {
                    ancestor_domain_key: Some(ancestor),
                    descendant_like_pattern: None,
                };
            }
        }
        return RelatedClassificationLookup::default();
    }

    if let Some(host) = normalized_key.strip_prefix("domain:") {
        let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
        if !host.is_empty() {
            return RelatedClassificationLookup {
                ancestor_domain_key: None,
                descendant_like_pattern: Some(format!("subdomain:%.{}", host)),
            };
        }
    }

    RelatedClassificationLookup::default()
}

fn derive_registered_domain(hostname: &str) -> Option<String> {
    let trimmed = hostname.trim().trim_end_matches('.').to_ascii_lowercase();
    if trimmed.is_empty() {
        return None;
    }
    let labels: Vec<&str> = trimmed
        .split('.')
        .filter(|label| !label.is_empty())
        .collect();
    if labels.len() <= 2 {
        Some(trimmed)
    } else {
        Some(labels[labels.len() - 2..].join("."))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    let cfg = config::load()?;
    validate_config(&cfg)?;
    emit_env_alias_warnings(&cfg);
    if check_config_mode_enabled() {
        println!("policy-engine config check passed");
        return Ok(());
    }
    let (db_url, _db_alias) = resolve_policy_db_url(&cfg);
    let admin_db_url = env::var("OD_ADMIN_DATABASE_URL").ok();
    if let (Some(policy_db), Some(admin_db)) = (db_url.as_ref(), admin_db_url.as_ref()) {
        if policy_db != admin_db {
            warn!(
                target = "svc-policy",
                policy_database = %policy_db,
                admin_database = %admin_db,
                "policy/admin databases differ; policy updates may drift from enforcement runtime"
            );
        }
    }
    let (activation_db_url, _activation_alias) = resolve_activation_db_url(&cfg, db_url.clone());
    let taxonomy =
        Arc::new(TaxonomyStore::load_default().context("failed to load canonical taxonomy")?);
    let mut audit_logger = None;

    let (evaluator, classification_pool) = if let Some(db_url) = db_url {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await?;
        let shared_with_admin_db = admin_db_url
            .as_deref()
            .map(|admin_db| admin_db == db_url)
            .unwrap_or_else(|| activation_db_url.as_deref() == Some(db_url.as_str()));
        if shared_with_admin_db {
            warn!(
                target = "svc-policy",
                "policy database is shared with admin database; skipping policy-engine local migrations"
            );
            ensure_policy_tables(&pool).await?;
        } else {
            sqlx::migrate!("./migrations").run(&pool).await?;
        }
        let activation_pool = if activation_db_url.as_deref() == Some(db_url.as_str()) {
            pool.clone()
        } else if let Some(url) = activation_db_url.as_deref() {
            PgPoolOptions::new().max_connections(5).connect(url).await?
        } else {
            pool.clone()
        };
        let (activation_state, activation_refresh_enabled) = match ActivationState::load(
            &activation_pool,
        )
        .await
        {
            Ok(state) => (state, true),
            Err(err) => {
                tracing::warn!(
                    target = "svc-policy",
                    %err,
                    "failed to load taxonomy activation profile; defaulting to fail-closed activation"
                );
                (ActivationState::deny_all(), false)
            }
        };
        let activation = Arc::new(activation_state);
        if activation_refresh_enabled {
            ActivationState::spawn_refresh_task(Arc::clone(&activation), activation_pool.clone());
        }
        let store = match PolicyStore::load_from_db(&pool, Arc::clone(&taxonomy)).await? {
            Some(store) => store,
            None => {
                let doc = PolicyDocument::load_from_file(&cfg.policy_file)?;
                PolicyStore::seed_db_from_document(&pool, &doc, "default", Some("system")).await?;
                PolicyStore::load_from_db(&pool, Arc::clone(&taxonomy))
                    .await?
                    .expect("seeded policy must exist")
            }
        };
        audit_logger = Some(PolicyAuditLogger::new(pool.clone()));
        (
            PolicyEvaluator::from_database(
                store,
                pool.clone(),
                activation_pool.clone(),
                Some(cfg.policy_file.clone()),
                activation,
            ),
            Some(activation_pool.clone()),
        )
    } else {
        let activation = Arc::new(ActivationState::allow_all());
        let store = PolicyStore::load_from_file(&cfg.policy_file, Arc::clone(&taxonomy))?;
        (
            PolicyEvaluator::from_file(store, cfg.policy_file.clone(), activation),
            None,
        )
    };
    let auth_settings = AuthSettings::from_env(cfg.auth.clone());
    let admin_auth = Arc::new(AdminAuth::from_config(auth_settings).await?);
    let state = AppState {
        evaluator: Arc::new(evaluator),
        audit_logger,
        classification_pool,
    };

    let loaded_policy_id = state
        .evaluator
        .policy_id()
        .map(|id| id.to_string())
        .unwrap_or_else(|| "<none>".to_string());
    info!(
        target = "svc-policy",
        policy_id = loaded_policy_id,
        policy_version = %state.evaluator.version(),
        "policy runtime initialized"
    );

    let auth_layer = {
        let auth = admin_auth.clone();
        middleware::from_fn(move |req, next| {
            let auth = auth.clone();
            async move { enforce_admin(auth, req, next).await }
        })
    };

    let deprecation_layer = middleware::from_fn(deprecated_policy_admin_middleware);

    let admin_routes = Router::new()
        .route("/api/v1/policies", get(list_policies).post(create_policy))
        .route("/api/v1/policies/reload", post(reload_policies))
        .route("/api/v1/policies/simulate", post(simulate_policy))
        .route("/api/v1/policies/:id", put(update_policy))
        .with_state(state.clone())
        .layer(deprecation_layer)
        .layer(auth_layer);

    let app = Router::new()
        .route("/api/v1/decision", post(handle_decision))
        .route("/health/ready", get(health))
        .with_state(state.clone())
        .merge(admin_routes);

    let addr: SocketAddr = format!("{}:{}", cfg.api_host, cfg.api_port).parse()?;
    let listener = TcpListener::bind(addr).await?;
    info!(target = "svc-policy", %addr, "policy engine listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn deprecated_policy_admin_middleware(
    req: axum::http::Request<axum::body::Body>,
    next: middleware::Next,
) -> Response {
    warn!(
        target = "svc-policy",
        method = %req.method(),
        path = %req.uri().path(),
        "policy-engine policy admin endpoint is deprecated; use admin-api /api/v1/policies"
    );
    let mut response = next.run(req).await;
    response
        .headers_mut()
        .insert("deprecation", HeaderValue::from_static("true"));
    response.headers_mut().insert(
        "warning",
        HeaderValue::from_static("299 - \"Deprecated API: use admin-api /api/v1/policies\""),
    );
    response.headers_mut().insert(
        "sunset",
        HeaderValue::from_static("Wed, 31 Dec 2026 23:59:59 GMT"),
    );
    response
}

async fn handle_decision(
    State(state): State<AppState>,
    Json(mut payload): Json<DecisionRequest>,
) -> Result<Json<common_types::PolicyDecision>, (StatusCode, Json<ErrorResponse>)> {
    if payload.normalized_key.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error_code: "VALIDATION_ERROR",
                message: "normalized_key required".into(),
            }),
        ));
    }

    if let Some(decision) = state.resolve_override_decision(&payload).await {
        return Ok(Json(decision));
    }

    state.hydrate_request_hints(&mut payload).await;
    let mut simulation = state.evaluator.simulate(&payload);
    simulation.decision.decision_source = Some(decision_source_for_simulation(&state, &simulation));
    let decision = simulation.decision;
    Ok(Json(decision))
}

fn decision_source_for_simulation(
    state: &AppState,
    simulation: &store::SimulationResult,
) -> String {
    let taxonomy_disabled = simulation
        .decision
        .verdict
        .as_ref()
        .map(|verdict| {
            !state
                .evaluator
                .is_verdict_enabled(&verdict.primary_category, &verdict.subcategory)
        })
        .unwrap_or(false);
    if taxonomy_disabled {
        "taxonomy_disabled".to_string()
    } else if simulation.matched_rule_id.is_some() {
        "policy_rule".to_string()
    } else {
        "default".to_string()
    }
}

async fn list_policies(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
) -> Result<Json<PolicyListResponse>, StatusCode> {
    require_roles(&user, ROLE_POLICY_VIEWER_ROLES)?;
    let rules = state.evaluator.rules();
    let version = state.evaluator.version();
    let policy_id = state.evaluator.policy_id();
    Ok(Json(PolicyListResponse::from_store(
        version, policy_id, rules,
    )))
}

async fn reload_policies(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
) -> Result<Json<PolicyListResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_roles(&user, ROLE_POLICY_EDITOR_ROLES)
        .map_err(|status| (status, Json(ErrorResponse::forbidden())))?;
    state.evaluator.reload().await.map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error_code: "RELOAD_FAILED",
                message: err.to_string(),
            }),
        )
    })?;
    let rules = state.evaluator.rules();
    let version = state.evaluator.version();
    let policy_id = state.evaluator.policy_id();
    state
        .log_event(PolicyAuditEvent {
            action: "policy.reload".into(),
            actor: Some(user.actor.clone()),
            policy_id,
            version: Some(version.clone()),
            status: None,
            notes: Some("manual reload".into()),
            diff: None,
        })
        .await;
    Ok(Json(PolicyListResponse::from_store(
        version, policy_id, rules,
    )))
}

async fn create_policy(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Json(mut payload): Json<PolicyCreateRequest>,
) -> Result<Json<PolicyListResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_roles(&user, ROLE_POLICY_EDITOR_ROLES)
        .map_err(|status| (status, Json(ErrorResponse::forbidden())))?;
    if payload.created_by.is_none() {
        payload.created_by = Some(user.actor.clone());
    }
    let audit_diff = serde_json::to_value(&payload).ok();
    let created_by = payload.created_by.clone();
    let version = payload.version.clone();
    let policy_id = state
        .evaluator
        .create_policy(payload)
        .await
        .map_err(|err| {
            let status = if err.to_string().contains("database backend not configured") {
                StatusCode::NOT_IMPLEMENTED
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (
                status,
                Json(ErrorResponse {
                    error_code: "POLICY_CREATE_FAILED",
                    message: err.to_string(),
                }),
            )
        })?;
    state
        .log_event(PolicyAuditEvent {
            action: "policy.create".into(),
            actor: created_by,
            policy_id: Some(policy_id),
            version: Some(version),
            status: Some("active".into()),
            notes: None,
            diff: audit_diff,
        })
        .await;
    let rules = state.evaluator.rules();
    let version = state.evaluator.version();
    let policy_id = state.evaluator.policy_id();
    Ok(Json(PolicyListResponse::from_store(
        version, policy_id, rules,
    )))
}

async fn simulate_policy(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Json(mut payload): Json<SimulatePolicyRequest>,
) -> Result<Json<SimulationResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_roles(&user, ROLE_POLICY_VIEWER_ROLES)
        .map_err(|status| (status, Json(ErrorResponse::forbidden())))?;
    if payload.request.normalized_key.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error_code: "VALIDATION_ERROR",
                message: "normalized_key required".into(),
            }),
        ));
    }

    if payload.mode == SimulationMode::Runtime {
        if let Some(decision) = state.resolve_override_decision(&payload.request).await {
            return Ok(Json(SimulationResponse {
                decision,
                matched_rule_id: None,
                policy_version: state.evaluator.version(),
                mode: payload.mode.as_str().to_string(),
            }));
        }
        state.hydrate_request_hints(&mut payload.request).await;
    }

    let mut result = state.evaluator.simulate(&payload.request);
    result.decision.decision_source = Some(decision_source_for_simulation(&state, &result));

    Ok(Json(SimulationResponse {
        decision: result.decision,
        matched_rule_id: result.matched_rule_id,
        policy_version: state.evaluator.version(),
        mode: payload.mode.as_str().to_string(),
    }))
}

async fn update_policy(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(policy_id): Path<String>,
    Json(payload): Json<PolicyUpdateRequest>,
) -> Result<Json<PolicyListResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_roles(&user, ROLE_POLICY_EDITOR_ROLES)
        .map_err(|status| (status, Json(ErrorResponse::forbidden())))?;

    let target_id = if policy_id == "current" {
        state.evaluator.policy_id().ok_or((
            StatusCode::NOT_IMPLEMENTED,
            Json(ErrorResponse {
                error_code: "POLICY_DB_DISABLED",
                message: "policy database backend required for updates".into(),
            }),
        ))?
    } else {
        Uuid::parse_str(&policy_id).map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error_code: "VALIDATION_ERROR",
                    message: "policy_id must be a UUID or 'current'".into(),
                }),
            )
        })?
    };

    let audit_diff = serde_json::to_value(&payload).ok();
    let status_override = payload.status.clone();
    let notes = payload.notes.clone();
    let version_override = payload.version.clone();

    state
        .evaluator
        .update_policy(target_id, payload, &user.actor)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error_code: "POLICY_UPDATE_FAILED",
                    message: err.to_string(),
                }),
            )
        })?;

    let rules = state.evaluator.rules();
    let current_version = state.evaluator.version();
    let logged_version = version_override.unwrap_or(current_version.clone());
    let policy_id = state.evaluator.policy_id();
    state
        .log_event(PolicyAuditEvent {
            action: "policy.update".into(),
            actor: Some(user.actor.clone()),
            policy_id: Some(target_id),
            version: Some(logged_version),
            status: status_override,
            notes,
            diff: audit_diff,
        })
        .await;
    Ok(Json(PolicyListResponse::from_store(
        current_version,
        policy_id,
        rules,
    )))
}

async fn health() -> &'static str {
    "OK"
}

async fn ensure_policy_tables(pool: &PgPool) -> Result<()> {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS policies (
               id UUID PRIMARY KEY,
               name TEXT NOT NULL,
               version TEXT NOT NULL,
               status TEXT NOT NULL DEFAULT 'active',
               created_by TEXT,
               created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
           )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS policy_rules (
               id UUID PRIMARY KEY,
               policy_id UUID NOT NULL REFERENCES policies(id) ON DELETE CASCADE,
               priority INTEGER NOT NULL,
               action TEXT NOT NULL,
               description TEXT,
               conditions JSONB NOT NULL,
               created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
           )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS policy_rules_policy_priority_idx ON policy_rules (policy_id, priority)",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS policy_versions (
               id UUID PRIMARY KEY,
               policy_id UUID NOT NULL REFERENCES policies(id) ON DELETE CASCADE,
               version TEXT NOT NULL,
               status TEXT NOT NULL DEFAULT 'draft',
               created_by TEXT,
               created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
               notes TEXT,
               rules JSONB NOT NULL,
               deployed_at TIMESTAMPTZ
           )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS policy_versions_policy_idx ON policy_versions (policy_id, created_at DESC)",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS policy_audit_events (
               id UUID PRIMARY KEY,
               policy_id UUID REFERENCES policies(id) ON DELETE SET NULL,
               action TEXT NOT NULL,
               actor TEXT,
               version TEXT,
               status TEXT,
               notes TEXT,
               diff JSONB,
               created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
           )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS policy_audit_events_policy_idx ON policy_audit_events (policy_id, created_at DESC)",
    )
    .execute(pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::Router;
    use common_types::PolicyAction;
    use std::{collections::HashMap, fs};
    use tower::ServiceExt;
    use uuid::Uuid;

    fn base_policy_cfg() -> config::PolicyConfig {
        config::PolicyConfig {
            api_host: "0.0.0.0".into(),
            api_port: 19010,
            policy_file: "config/policies.json".into(),
            database_url: Some(
                "postgres://svc_policy:prod-db-password-123@db.example:5432/defender_policy".into(),
            ),
            activation_database_url: Some(
                "postgres://svc_policy:prod-db-password-123@db.example:5432/defender_admin".into(),
            ),
            admin_token: None,
            auth: None,
        }
    }

    fn in_memory_app(doc: PolicyDocument) -> Router {
        let tmp = std::env::temp_dir().join(format!("policy-app-test-{}.json", Uuid::new_v4()));
        fs::write(&tmp, serde_json::to_string(&doc).unwrap()).unwrap();
        let taxonomy = Arc::new(TaxonomyStore::load_default().unwrap());
        let activation = Arc::new(ActivationState::allow_all());
        let store =
            PolicyStore::load_from_file(tmp.to_str().unwrap(), Arc::clone(&taxonomy)).unwrap();
        let evaluator = PolicyEvaluator::from_file(store, tmp.to_string_lossy().into(), activation);
        let state = AppState {
            evaluator: Arc::new(evaluator),
            audit_logger: None,
            classification_pool: None,
        };
        Router::new()
            .route("/api/v1/decision", post(handle_decision))
            .route("/api/v1/policies", get(list_policies))
            .with_state(state)
    }

    #[tokio::test]
    async fn decision_blocked_when_category_matches() {
        let doc = PolicyDocument {
            version: "test".into(),
            rules: vec![policy_dsl::PolicyRule {
                id: "block-social".into(),
                description: Some("Block social".into()),
                priority: 10,
                action: PolicyAction::Block,
                conditions: policy_dsl::Conditions {
                    categories: Some(vec!["Social".into()]),
                    ..Default::default()
                },
            }],
        };
        let app = in_memory_app(doc);
        let payload = serde_json::json!({
            "normalized_key": "domain:example.com",
            "entity_level": "domain",
            "source_ip": "10.0.0.1",
            "category_hint": "Social"
        });

        let response = app
            .oneshot(
                Request::post("/api/v1/decision")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn decision_validation_error() {
        let doc = PolicyDocument {
            version: "test".into(),
            rules: vec![],
        };
        let app = in_memory_app(doc);
        let payload = serde_json::json!({
            "normalized_key": "",
            "entity_level": "domain",
            "source_ip": "10.0.0.1"
        });

        let response = app
            .oneshot(
                Request::post("/api/v1/decision")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn deprecation_middleware_adds_headers() {
        async fn ok() -> &'static str {
            "ok"
        }

        let app = Router::new()
            .route("/api/v1/policies", get(ok))
            .layer(middleware::from_fn(deprecated_policy_admin_middleware));

        let response = app
            .oneshot(
                Request::get("/api/v1/policies")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("deprecation")
                .and_then(|value| value.to_str().ok()),
            Some("true")
        );
        assert!(response.headers().get("warning").is_some());
        assert!(response.headers().get("sunset").is_some());
    }

    #[test]
    fn unknown_toggle_controls_decision() {
        let taxonomy = Arc::new(TaxonomyStore::load_default().unwrap());
        let doc = PolicyDocument {
            version: "test".into(),
            rules: vec![policy_dsl::PolicyRule {
                id: "allow-all".into(),
                description: None,
                priority: 1,
                action: PolicyAction::Allow,
                conditions: policy_dsl::Conditions::default(),
            }],
        };
        let store = PolicyStore::from_document(doc.clone(), Arc::clone(&taxonomy)).unwrap();
        let evaluator = PolicyEvaluator::from_file(
            store,
            "test".into(),
            Arc::new(ActivationState::allow_all()),
        );
        let request = DecisionRequest {
            normalized_key: "domain:unknown.test".into(),
            entity_level: "domain".into(),
            source_ip: "192.0.2.10".into(),
            user_id: None,
            group_ids: None,
            category_hint: Some("Unknown / Unclassified".into()),
            subcategory_hint: None,
            risk_hint: None,
            confidence_hint: None,
        };
        let decision = evaluator.evaluate(&request);
        assert_eq!(decision.action, PolicyAction::Allow);

        let mut category_states = HashMap::new();
        category_states.insert("unknown-unclassified".into(), false);
        let activation = Arc::new(ActivationState::from_maps(
            category_states,
            HashMap::new(),
            false,
        ));
        let store_block = PolicyStore::from_document(doc, taxonomy).unwrap();
        let evaluator = PolicyEvaluator::from_file(store_block, "test".into(), activation);
        let decision = evaluator.evaluate(&request);
        assert_eq!(decision.action, PolicyAction::Block);
    }

    #[test]
    fn applies_persisted_hints_when_missing() {
        let mut request = DecisionRequest {
            normalized_key: "subdomain:www.instagram.com".into(),
            entity_level: "subdomain".into(),
            source_ip: "127.0.0.1".into(),
            user_id: None,
            group_ids: None,
            category_hint: None,
            subcategory_hint: None,
            risk_hint: None,
            confidence_hint: None,
        };
        apply_persisted_hints(
            &mut request,
            PersistedClassificationHints {
                category_hint: Some("social-media".into()),
                subcategory_hint: Some("photo-sharing".into()),
                risk_hint: Some("low".into()),
                confidence_hint: Some(0.9),
            },
        );
        assert_eq!(request.category_hint.as_deref(), Some("social-media"));
        assert_eq!(request.subcategory_hint.as_deref(), Some("photo-sharing"));
        assert_eq!(request.risk_hint.as_deref(), Some("low"));
        assert_eq!(request.confidence_hint, Some(0.9));
    }

    #[test]
    fn does_not_override_existing_request_hints() {
        let mut request = DecisionRequest {
            normalized_key: "subdomain:www.instagram.com".into(),
            entity_level: "subdomain".into(),
            source_ip: "127.0.0.1".into(),
            user_id: None,
            group_ids: None,
            category_hint: Some("news-media".into()),
            subcategory_hint: Some("general-news".into()),
            risk_hint: Some("medium".into()),
            confidence_hint: Some(0.7),
        };
        apply_persisted_hints(
            &mut request,
            PersistedClassificationHints {
                category_hint: Some("social-media".into()),
                subcategory_hint: Some("photo-sharing".into()),
                risk_hint: Some("low".into()),
                confidence_hint: Some(0.9),
            },
        );
        assert_eq!(request.category_hint.as_deref(), Some("news-media"));
        assert_eq!(request.subcategory_hint.as_deref(), Some("general-news"));
        assert_eq!(request.risk_hint.as_deref(), Some("medium"));
        assert_eq!(request.confidence_hint, Some(0.7));
    }

    #[test]
    fn extracts_host_from_domain_and_subdomain_keys() {
        assert_eq!(
            host_from_normalized_key("domain:mozilla.org"),
            Some("mozilla.org".to_string())
        );
        assert_eq!(
            host_from_normalized_key("subdomain:www.mozilla.org"),
            Some("www.mozilla.org".to_string())
        );
        assert_eq!(host_from_normalized_key("url:https://mozilla.org"), None);
    }

    #[test]
    fn domain_scope_matching_honors_subdomain_boundary() {
        assert!(domain_scope_matches_host("mozilla.org", "mozilla.org"));
        assert!(domain_scope_matches_host("mozilla.org", "www.mozilla.org"));
        assert!(domain_scope_matches_host(
            "*.mozilla.org",
            "www.mozilla.org"
        ));
        assert!(!domain_scope_matches_host("mozilla.org", "evilmozilla.org"));
        assert!(!domain_scope_matches_host(
            "mozilla.org",
            "mozilla.org.evil"
        ));
    }

    #[test]
    fn more_specific_domain_scope_has_higher_priority() {
        assert!(
            domain_scope_specificity("support.mozilla.org")
                > domain_scope_specificity("mozilla.org")
        );
    }

    #[test]
    fn block_action_wins_tie_break() {
        assert!(override_action_rank("block") > override_action_rank("allow"));
        assert_eq!(override_action_rank("unknown"), 0);
    }

    #[test]
    fn detects_conflicting_actions_after_normalization() {
        assert_eq!(normalize_override_action(" BLOCK "), "block");
        assert_ne!(
            normalize_override_action("allow"),
            normalize_override_action("block")
        );
    }

    #[test]
    fn related_lookup_for_subdomain_prefers_registered_domain() {
        let related = related_classification_lookup("subdomain:www.youtube.com");
        assert_eq!(
            related.ancestor_domain_key.as_deref(),
            Some("domain:youtube.com")
        );
        assert!(related.descendant_like_pattern.is_none());
    }

    #[test]
    fn related_lookup_for_domain_includes_descendant_pattern() {
        let related = related_classification_lookup("domain:youtube.com");
        assert!(related.ancestor_domain_key.is_none());
        assert_eq!(
            related.descendant_like_pattern.as_deref(),
            Some("subdomain:%.youtube.com")
        );
    }

    #[test]
    fn derive_registered_domain_handles_multi_label_subdomain() {
        assert_eq!(
            derive_registered_domain("api.eu.example.com").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            derive_registered_domain("example.com").as_deref(),
            Some("example.com")
        );
    }

    #[test]
    fn config_rejects_default_database_credentials_when_dev_mode_disabled() {
        std::env::remove_var("OD_ALLOW_INSECURE_DEV_SECRETS");
        let mut cfg = base_policy_cfg();
        cfg.database_url = Some("postgres://defender:defender@postgres:5432/defender_admin".into());
        let err = validate_config(&cfg).expect_err("default DB credentials must fail");
        assert!(format!("{err:#}").contains("OD_POLICY_DATABASE_URL"));
    }

    #[test]
    fn config_accepts_strong_database_credentials_when_dev_mode_disabled() {
        std::env::remove_var("OD_ALLOW_INSECURE_DEV_SECRETS");
        let cfg = base_policy_cfg();
        assert!(validate_config(&cfg).is_ok());
    }

    #[test]
    fn config_allows_default_database_credentials_in_explicit_dev_mode() {
        std::env::set_var("OD_ALLOW_INSECURE_DEV_SECRETS", "true");
        let mut cfg = base_policy_cfg();
        cfg.database_url = Some("postgres://defender:defender@postgres:5432/defender_admin".into());
        assert!(validate_config(&cfg).is_ok());
        std::env::remove_var("OD_ALLOW_INSECURE_DEV_SECRETS");
    }
}
