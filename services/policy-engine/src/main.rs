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
    http::StatusCode,
    middleware,
    routing::{get, post, put},
    Extension, Json, Router,
};
use evaluator::PolicyEvaluator;
use models::{
    DecisionRequest, ErrorResponse, PolicyCreateRequest, PolicyListResponse, PolicyUpdateRequest,
    SimulationResponse,
};
use policy_dsl::PolicyDocument;
use sqlx::postgres::PgPoolOptions;
use std::{env, net::SocketAddr, sync::Arc};
use store::PolicyStore;
use taxonomy::{ActivationState, TaxonomyStore};
use tokio::net::TcpListener;
use tracing::{error, info, Level};
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    evaluator: Arc<PolicyEvaluator>,
    audit_logger: Option<PolicyAuditLogger>,
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
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    let cfg = config::load()?;
    let db_url = cfg
        .database_url
        .clone()
        .or_else(|| env::var("OD_POLICY_DATABASE_URL").ok())
        .or_else(|| env::var("DATABASE_URL").ok());
    let activation_db_url = cfg
        .activation_database_url
        .clone()
        .or_else(|| env::var("OD_TAXONOMY_DATABASE_URL").ok())
        .or_else(|| env::var("OD_ADMIN_DATABASE_URL").ok())
        .or_else(|| db_url.clone());
    let taxonomy =
        Arc::new(TaxonomyStore::load_default().context("failed to load canonical taxonomy")?);
    let mut audit_logger = None;

    let evaluator = if let Some(db_url) = db_url {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        let activation_pool = if activation_db_url.as_deref() == Some(db_url.as_str()) {
            pool.clone()
        } else if let Some(url) = activation_db_url.as_deref() {
            PgPoolOptions::new().max_connections(5).connect(url).await?
        } else {
            pool.clone()
        };
        let (activation_state, activation_refresh_enabled) =
            match ActivationState::load(&activation_pool).await {
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
        PolicyEvaluator::from_database(store, pool, Some(cfg.policy_file.clone()), activation)
    } else {
        let activation = Arc::new(ActivationState::allow_all());
        let store = PolicyStore::load_from_file(&cfg.policy_file, Arc::clone(&taxonomy))?;
        PolicyEvaluator::from_file(store, cfg.policy_file.clone(), activation)
    };
    let auth_settings = AuthSettings::from_env(cfg.auth.clone());
    let admin_auth = Arc::new(AdminAuth::from_config(auth_settings).await?);
    let state = AppState {
        evaluator: Arc::new(evaluator),
        audit_logger,
    };

    let auth_layer = {
        let auth = admin_auth.clone();
        middleware::from_fn(move |req, next| {
            let auth = auth.clone();
            async move { enforce_admin(auth, req, next).await }
        })
    };

    let admin_routes = Router::new()
        .route("/api/v1/policies", get(list_policies).post(create_policy))
        .route("/api/v1/policies/reload", post(reload_policies))
        .route("/api/v1/policies/simulate", post(simulate_policy))
        .route("/api/v1/policies/:id", put(update_policy))
        .with_state(state.clone())
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

async fn handle_decision(
    State(state): State<AppState>,
    Json(payload): Json<DecisionRequest>,
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

    let decision = state.evaluator.evaluate(&payload);
    Ok(Json(decision))
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
    Json(payload): Json<DecisionRequest>,
) -> Result<Json<SimulationResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_roles(&user, ROLE_POLICY_VIEWER_ROLES)
        .map_err(|status| (status, Json(ErrorResponse::forbidden())))?;
    if payload.normalized_key.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error_code: "VALIDATION_ERROR",
                message: "normalized_key required".into(),
            }),
        ));
    }

    let result = state.evaluator.simulate(&payload);
    Ok(Json(SimulationResponse {
        decision: result.decision,
        matched_rule_id: result.matched_rule_id,
        policy_version: state.evaluator.version(),
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
}
