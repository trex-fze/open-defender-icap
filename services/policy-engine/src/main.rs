mod config;
mod evaluator;
mod models;

use anyhow::Result;
use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use evaluator::PolicyEvaluator;
use models::{DecisionRequest, ErrorResponse};
use std::{net::SocketAddr, sync::Arc};
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
    let state = AppState {
        evaluator: Arc::new(PolicyEvaluator::default()),
    };
    let app = Router::new()
        .route("/api/v1/decision", post(handle_decision))
        .route("/health/ready", axum::routing::get(health))
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

async fn health() -> &'static str {
    "OK"
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn test_app() -> Router {
        let state = AppState {
            evaluator: Arc::new(PolicyEvaluator::default()),
        };
        Router::new()
            .route("/api/v1/decision", post(handle_decision))
            .with_state(state)
    }

    #[tokio::test]
    async fn decision_allows_normal_key() {
        let app = test_app();
        let payload = serde_json::json!({
            "normalized_key": "domain:example.com",
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
