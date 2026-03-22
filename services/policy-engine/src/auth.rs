use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};

#[derive(Clone, Default)]
pub struct AdminAuth {
    pub token: Option<String>,
}

pub async fn enforce_admin(
    State(ctx): State<AdminAuth>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if let Some(expected) = ctx.token.as_deref() {
        if let Some(provided) = req
            .headers()
            .get("X-Admin-Token")
            .and_then(|v| v.to_str().ok())
        {
            if provided == expected {
                return Ok(next.run(req).await);
            }
        }
        Err(StatusCode::UNAUTHORIZED)
    } else {
        Ok(next.run(req).await)
    }
}
