mod auth;
mod config;
mod evaluator;
mod models;
mod store;

use anyhow::Result;
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
use tokio::net::TcpListener;
use tracing::{info, Level};
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    evaluator: Arc<PolicyEvaluator>,
    #[allow(dead_code)]
    auth: Arc<AdminAuth>,
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
    let admin_token = cfg
        .admin_token
        .clone()
        .or_else(|| env::var("OD_POLICY_ADMIN_TOKEN").ok());

    let evaluator = if let Some(db_url) = db_url {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        let store = match PolicyStore::load_from_db(&pool).await? {
            Some(store) => store,
            None => {
                let doc = PolicyDocument::load_from_file(&cfg.policy_file)?;
                PolicyStore::seed_db_from_document(&pool, &doc, "default", Some("system")).await?;
                PolicyStore::load_from_db(&pool)
                    .await?
                    .expect("seeded policy must exist")
            }
        };
        PolicyEvaluator::from_database(store, pool, Some(cfg.policy_file.clone()))
    } else {
        let store = PolicyStore::load_from_file(&cfg.policy_file)?;
        PolicyEvaluator::from_file(store, cfg.policy_file.clone())
    };
    let auth_settings = AuthSettings::from_env(cfg.auth.clone());
    let admin_auth = Arc::new(AdminAuth::from_config(admin_token.clone(), auth_settings).await?);
    let state = AppState {
        evaluator: Arc::new(evaluator),
        auth: admin_auth.clone(),
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
    state
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
    let version = state.evaluator.version();
    let policy_id = state.evaluator.policy_id();
    Ok(Json(PolicyListResponse::from_store(
        version, policy_id, rules,
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
    use tower::ServiceExt;
    use uuid::Uuid;

    fn test_app() -> Router {
        let doc = policy_dsl::PolicyDocument {
            version: "test".into(),
            rules: vec![policy_dsl::PolicyRule {
                id: "block-social".into(),
                description: None,
                priority: 10,
                action: common_types::PolicyAction::Block,
                conditions: policy_dsl::Conditions {
                    categories: Some(vec!["Social Media".into()]),
                    ..Default::default()
                },
            }],
        };
        let tmp = std::env::temp_dir().join(format!("policy-app-test-{}.json", Uuid::new_v4()));
        std::fs::write(&tmp, serde_json::to_string(&doc).unwrap()).unwrap();
        let store = PolicyStore::load_from_file(tmp.to_str().unwrap()).unwrap();
        let evaluator = PolicyEvaluator::from_file(store, tmp.to_str().unwrap().into());
        let state = AppState {
            evaluator: Arc::new(evaluator),
        };
        Router::new()
            .route("/api/v1/decision", post(handle_decision))
            .route("/api/v1/policies", get(list_policies))
            .with_state(state)
    }

    #[tokio::test]
    async fn decision_allows_normal_key() {
        let app = test_app();
        let payload = serde_json::json!({
            "normalized_key": "domain:example.com",
            "entity_level": "domain",
            "source_ip": "10.0.0.1",
            "category_hint": "Social Media"
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
        let app = test_app();
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
}
