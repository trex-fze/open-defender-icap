mod config;
mod evaluator;
mod models;
mod store;

use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use evaluator::PolicyEvaluator;
use models::{DecisionRequest, ErrorResponse, PolicyCreateRequest, PolicyListResponse};
use policy_dsl::PolicyDocument;
use sqlx::postgres::PgPoolOptions;
use std::{net::SocketAddr, sync::Arc};
use store::PolicyStore;
use tokio::net::TcpListener;
use tracing::{info, Level};

#[derive(Clone)]
struct AppState {
    evaluator: Arc<PolicyEvaluator>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    let cfg = config::load()?;
    let evaluator = if let Some(db_url) = cfg.database_url.clone() {
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
    let state = AppState {
        evaluator: Arc::new(evaluator),
    };
    let app = Router::new()
        .route("/api/v1/decision", post(handle_decision))
        .route("/api/v1/policies", get(list_policies).post(create_policy))
        .route("/api/v1/policies/reload", post(reload_policies))
        .route("/health/ready", get(health))
        .with_state(state);

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

async fn list_policies(State(state): State<AppState>) -> Json<PolicyListResponse> {
    let rules = state.evaluator.rules();
    let version = state.evaluator.version();
    Json(PolicyListResponse::from_rules(version, rules))
}

async fn reload_policies(
    State(state): State<AppState>,
) -> Result<Json<PolicyListResponse>, (StatusCode, Json<ErrorResponse>)> {
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
    Ok(Json(PolicyListResponse::from_rules(version, rules)))
}

async fn create_policy(
    State(state): State<AppState>,
    Json(payload): Json<PolicyCreateRequest>,
) -> Result<Json<PolicyListResponse>, (StatusCode, Json<ErrorResponse>)> {
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
    Ok(Json(PolicyListResponse::from_rules(version, rules)))
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
