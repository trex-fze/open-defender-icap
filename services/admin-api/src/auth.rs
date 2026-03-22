use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use serde::Deserialize;
use std::{collections::HashSet, env, sync::Arc};

type HmacSha256 = Hmac<Sha256>;

const ROLE_POLICY_ADMIN: &str = "policy-admin";
const ROLE_POLICY_EDITOR: &str = "policy-editor";
const ROLE_POLICY_VIEWER: &str = "policy-viewer";
const ROLE_REVIEWER: &str = "review-approver";
const ROLE_AUDITOR: &str = "auditor";

#[derive(Clone)]
pub struct AdminAuth {
    static_token: Option<String>,
    static_roles: Vec<String>,
    jwt: Option<JwtValidator>,
}

#[derive(Clone, Debug, Deserialize, Default)]
#[serde(default)]
pub struct AuthSettings {
    pub static_roles: Option<Vec<String>>,
    pub oidc_issuer: Option<String>,
    pub oidc_audience: Option<String>,
    pub oidc_hs256_secret: Option<String>,
}

impl AuthSettings {
    pub fn merge_env(mut self) -> Self {
        if let Ok(secret) = env::var("OD_OIDC_HS256_SECRET") {
            self.oidc_hs256_secret = Some(secret);
        }
        if let Ok(issuer) = env::var("OD_OIDC_ISSUER") {
            self.oidc_issuer = Some(issuer);
        }
        if let Ok(aud) = env::var("OD_OIDC_AUDIENCE") {
            self.oidc_audience = Some(aud);
        }
        if self.static_roles.as_ref().map_or(true, |r| r.is_empty()) {
            self.static_roles = Some(default_static_roles());
        }
        self
    }
}

fn default_static_roles() -> Vec<String> {
    vec![
        ROLE_POLICY_ADMIN.into(),
        ROLE_POLICY_EDITOR.into(),
        ROLE_POLICY_VIEWER.into(),
        ROLE_AUDITOR.into(),
    ]
}

impl AdminAuth {
    pub async fn from_config(
        static_token: Option<String>,
        settings: AuthSettings,
    ) -> anyhow::Result<Self> {
        let merged = settings.merge_env();
        let jwt = if let (Some(secret), Some(issuer), Some(audience)) = (
            merged.oidc_hs256_secret.clone(),
            merged.oidc_issuer.clone(),
            merged.oidc_audience.clone(),
        ) {
            Some(JwtValidator::new(secret, issuer, audience))
        } else {
            None
        };

        Ok(Self {
            static_token,
            static_roles: merged.static_roles.unwrap_or_else(default_static_roles),
            jwt,
        })
    }

    pub fn authenticate(&self, req: &Request<Body>) -> Result<UserContext, StatusCode> {
        if let Some(jwt) = &self.jwt {
            if let Some(token) = bearer_token(req) {
                return jwt.validate(token).map_err(|_| StatusCode::UNAUTHORIZED);
            }
        }

        if let Some(expected) = self.static_token.as_deref() {
            if let Some(header) = req.headers().get("X-Admin-Token") {
                if header == expected {
                    return Ok(UserContext::from_static(&self.static_roles));
                }
            }
            return Err(StatusCode::UNAUTHORIZED);
        }

        Ok(UserContext::system())
    }
}

pub async fn enforce_admin(
    ctx: Arc<AdminAuth>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let user = ctx.authenticate(&req)?;
    req.extensions_mut().insert(user);
    Ok(next.run(req).await)
}

fn bearer_token(req: &Request<Body>) -> Option<&str> {
    req.headers()
        .get(header::AUTHORIZATION)
        .and_then(|hv| hv.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
}

#[derive(Clone, Debug)]
pub struct UserContext {
    pub actor: String,
    roles: HashSet<String>,
}

impl UserContext {
    pub fn system() -> Self {
        Self {
            actor: "system".into(),
            roles: default_static_roles().into_iter().collect(),
        }
    }

    fn from_static(roles: &[String]) -> Self {
        Self {
            actor: "static-admin".into(),
            roles: roles.iter().cloned().collect(),
        }
    }

    fn from_claims(actor: String, roles: Vec<String>) -> Self {
        Self {
            actor,
            roles: roles.into_iter().collect(),
        }
    }

    pub fn has_role(&self, role: &str) -> bool {
        self.roles.contains(role)
    }
}

#[derive(Clone)]
struct JwtValidator {
    secret: Vec<u8>,
    issuer: String,
    audience: String,
}

impl JwtValidator {
    fn new(secret: String, issuer: String, audience: String) -> Self {
        Self {
            secret: secret.into_bytes(),
            issuer,
            audience,
        }
    }

    fn validate(&self, token: &str) -> Result<UserContext, AuthError> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(AuthError::InvalidFormat);
        }
        let header = decode_segment(parts[0])?;
        let payload = decode_segment(parts[1])?;
        verify_signature(&self.secret, parts[0], parts[1], parts[2])?;

        let header: JwtHeader =
            serde_json::from_slice(&header).map_err(|_| AuthError::InvalidFormat)?;
        if header.alg != "HS256" {
            return Err(AuthError::InvalidFormat);
        }

        let claims: Claims =
            serde_json::from_slice(&payload).map_err(|_| AuthError::InvalidFormat)?;
        claims.validate(&self.issuer, &self.audience)?;
        let actor = claims
            .preferred_username
            .clone()
            .or(claims.email.clone())
            .unwrap_or_else(|| claims.sub.clone());
        let roles = claims.compute_roles();
        Ok(UserContext::from_claims(actor, roles))
    }
}

fn decode_segment(segment: &str) -> Result<Vec<u8>, AuthError> {
    URL_SAFE_NO_PAD
        .decode(segment)
        .map_err(|_| AuthError::InvalidFormat)
}

fn verify_signature(
    secret: &[u8],
    header: &str,
    payload: &str,
    signature: &str,
) -> Result<(), AuthError> {
    let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| AuthError::InvalidFormat)?;
    let signing_input = format!("{}.{}", header, payload);
    mac.update(signing_input.as_bytes());
    let expected = mac.finalize().into_bytes();
    let provided = decode_segment(signature)?;
    if expected.as_slice() == provided.as_slice() {
        Ok(())
    } else {
        Err(AuthError::InvalidSignature)
    }
}

#[derive(serde::Deserialize)]
struct JwtHeader {
    alg: String,
}

#[derive(Debug, serde::Deserialize)]
struct Claims {
    sub: String,
    iss: String,
    #[serde(default)]
    aud: Option<String>,
    #[serde(default)]
    exp: Option<i64>,
    #[serde(default)]
    preferred_username: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    roles: Option<Vec<String>>,
    #[serde(default)]
    scope: Option<String>,
}

impl Claims {
    fn validate(&self, issuer: &str, audience: &str) -> Result<(), AuthError> {
        if self.iss != issuer {
            return Err(AuthError::InvalidIssuer);
        }
        if self.aud.as_deref() != Some(audience) {
            return Err(AuthError::InvalidAudience);
        }
        if let Some(exp) = self.exp {
            if Utc::now().timestamp() > exp {
                return Err(AuthError::Expired);
            }
        }
        Ok(())
    }

    fn compute_roles(&self) -> Vec<String> {
        if let Some(roles) = &self.roles {
            if !roles.is_empty() {
                return roles.clone();
            }
        }
        if let Some(scope) = &self.scope {
            return scope.split_whitespace().map(|s| s.to_string()).collect();
        }
        default_static_roles()
    }
}

enum AuthError {
    InvalidFormat,
    InvalidSignature,
    InvalidIssuer,
    InvalidAudience,
    Expired,
}

pub fn require_roles(ctx: &UserContext, roles: &[&str]) -> Result<(), StatusCode> {
    if roles.iter().any(|role| ctx.has_role(role)) {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

pub const ROLE_OVERRIDES_WRITE: &[&str] = &[ROLE_POLICY_ADMIN, ROLE_POLICY_EDITOR];
pub const ROLE_OVERRIDES_DELETE: &[&str] = &[ROLE_POLICY_ADMIN];
pub const ROLE_OVERRIDES_VIEW: &[&str] =
    &[ROLE_POLICY_ADMIN, ROLE_POLICY_EDITOR, ROLE_POLICY_VIEWER];
pub const ROLE_REVIEW_VIEW: &[&str] = &[ROLE_POLICY_ADMIN, ROLE_POLICY_VIEWER, ROLE_REVIEWER];
pub const ROLE_REVIEW_RESOLVE: &[&str] = &[ROLE_POLICY_ADMIN, ROLE_REVIEWER];
