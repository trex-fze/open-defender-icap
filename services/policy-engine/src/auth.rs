use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use reqwest::{Client, StatusCode as ReqStatus};
use serde::Deserialize;
use std::{collections::HashSet, env, sync::Arc};
use tracing::error;

const ROLE_POLICY_ADMIN: &str = "policy-admin";
const ROLE_POLICY_EDITOR: &str = "policy-editor";
const ROLE_POLICY_VIEWER: &str = "policy-viewer";

#[derive(Clone)]
pub struct AdminAuth {
    resolver_url: String,
    client: Client,
}

#[derive(Clone, Debug, Deserialize, Default)]
#[serde(default)]
pub struct AuthSettings {
    pub resolver_url: Option<String>,
}

impl AuthSettings {
    pub fn from_env(value: Option<Self>) -> Self {
        let mut settings = value.unwrap_or_default();
        if let Ok(url) = env::var("OD_IAM_RESOLVER_URL") {
            settings.resolver_url = Some(url);
        }
        settings
    }
}

impl AdminAuth {
    pub async fn from_config(settings: AuthSettings) -> anyhow::Result<Self> {
        let resolver_url = settings
            .resolver_url
            .unwrap_or_else(|| "http://localhost:19000/api/v1/iam/whoami".to_string());
        let client = Client::builder().build()?;
        Ok(Self {
            resolver_url,
            client,
        })
    }

    pub async fn authenticate(
        &self,
        bearer: Option<String>,
        admin: Option<String>,
    ) -> Result<UserContext, StatusCode> {
        let has_auth = bearer.is_some() || admin.is_some();

        if !has_auth {
            return Err(StatusCode::UNAUTHORIZED);
        }

        let mut builder = self.client.get(&self.resolver_url);
        if let Some(authz) = bearer {
            builder = builder.header("Authorization", authz);
        }
        if let Some(token) = admin {
            builder = builder.header("X-Admin-Token", token);
        }

        let response = builder.send().await.map_err(|err| {
            error!(target = "svc-policy", %err, "failed to contact IAM resolver");
            StatusCode::BAD_GATEWAY
        })?;

        match response.status() {
            ReqStatus::OK => {
                let identity = response.json::<ResolverIdentity>().await.map_err(|err| {
                    error!(target = "svc-policy", %err, "failed to parse IAM resolver response");
                    StatusCode::BAD_GATEWAY
                })?;
                Ok(UserContext::from_identity(identity))
            }
            ReqStatus::UNAUTHORIZED => Err(StatusCode::UNAUTHORIZED),
            ReqStatus::FORBIDDEN => Err(StatusCode::FORBIDDEN),
            status => {
                let body = response.text().await.unwrap_or_default();
                error!(target = "svc-policy", %status, body, "IAM resolver returned error");
                Err(StatusCode::BAD_GATEWAY)
            }
        }
    }
}

pub async fn enforce_admin(
    auth: Arc<AdminAuth>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let bearer = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|hv| hv.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(|value| value.to_string());
    let admin = req
        .headers()
        .get("X-Admin-Token")
        .and_then(|hv| hv.to_str().ok())
        .map(|value| value.to_string());
    let user = auth.authenticate(bearer, admin).await?;
    req.extensions_mut().insert(user);
    Ok(next.run(req).await)
}

#[derive(Clone, Debug)]
pub struct UserContext {
    pub actor: String,
    roles: HashSet<String>,
}

impl UserContext {
    fn from_identity(identity: ResolverIdentity) -> Self {
        Self {
            actor: identity.actor,
            roles: identity.roles.into_iter().collect(),
        }
    }

    pub fn has_role(&self, role: &str) -> bool {
        self.roles.contains(role)
    }
}

#[derive(Deserialize)]
struct ResolverIdentity {
    actor: String,
    roles: Vec<String>,
    #[allow(dead_code)]
    permissions: Vec<String>,
}

pub fn require_roles(ctx: &UserContext, roles: &[&str]) -> Result<(), StatusCode> {
    if roles.iter().any(|role| ctx.has_role(role)) {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

pub const ROLE_POLICY_VIEWER_ROLES: &[&str] =
    &[ROLE_POLICY_ADMIN, ROLE_POLICY_EDITOR, ROLE_POLICY_VIEWER];
pub const ROLE_POLICY_EDITOR_ROLES: &[&str] = &[ROLE_POLICY_ADMIN, ROLE_POLICY_EDITOR];

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn authenticate_requires_credentials() {
        let auth = AdminAuth {
            resolver_url: "http://localhost:19000/api/v1/iam/whoami".to_string(),
            client: Client::builder().build().expect("client"),
        };

        let status = auth
            .authenticate(None, None)
            .await
            .expect_err("missing credentials must be rejected");
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}
