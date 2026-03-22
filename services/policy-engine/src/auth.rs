use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use serde::Deserialize;
use std::{collections::HashSet, sync::Arc};

const ROLE_POLICY_ADMIN: &str = "policy-admin";
const ROLE_POLICY_EDITOR: &str = "policy-editor";
const ROLE_POLICY_VIEWER: &str = "policy-viewer";

#[derive(Clone)]
pub struct AdminAuth {
    static_token: Option<String>,
    static_roles: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Default)]
#[serde(default)]
pub struct AuthSettings {
    pub static_roles: Option<Vec<String>>,
}

impl AuthSettings {
    pub fn from_env(value: Option<Self>) -> Self {
        let mut settings = value.unwrap_or_default();
        if settings
            .static_roles
            .as_ref()
            .map_or(true, |r| r.is_empty())
        {
            settings.static_roles = Some(default_roles());
        }
        settings
    }
}

fn default_roles() -> Vec<String> {
    vec![
        ROLE_POLICY_ADMIN.into(),
        ROLE_POLICY_EDITOR.into(),
        ROLE_POLICY_VIEWER.into(),
    ]
}

impl AdminAuth {
    pub async fn from_config(
        token: Option<String>,
        settings: AuthSettings,
    ) -> anyhow::Result<Self> {
        let merged = settings;
        Ok(Self {
            static_token: token,
            static_roles: merged.static_roles.unwrap_or_else(default_roles),
        })
    }

    pub fn authenticate(&self, req: &Request<Body>) -> Result<UserContext, StatusCode> {
        if let Some(expected) = self.static_token.as_deref() {
            if let Some(provided) = req
                .headers()
                .get("X-Admin-Token")
                .and_then(|v| v.to_str().ok())
            {
                if provided == expected {
                    return Ok(UserContext::from_static(&self.static_roles));
                }
            }
            return Err(StatusCode::UNAUTHORIZED);
        }
        Ok(UserContext::system())
    }
}

pub async fn enforce_admin(
    auth: Arc<AdminAuth>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let user = auth.authenticate(&req)?;
    req.extensions_mut().insert(user);
    Ok(next.run(req).await)
}

#[derive(Clone, Debug)]
pub struct UserContext {
    pub actor: String,
    roles: HashSet<String>,
}

impl UserContext {
    fn system() -> Self {
        Self {
            actor: "system".into(),
            roles: default_roles().into_iter().collect(),
        }
    }

    fn from_static(roles: &[String]) -> Self {
        Self {
            actor: "static-admin".into(),
            roles: roles.iter().cloned().collect(),
        }
    }

    pub fn has_role(&self, role: &str) -> bool {
        self.roles.contains(role)
    }
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
