use crate::iam::{EffectiveAccess, IamError, IamService, ServiceAccountPrincipal};
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
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::{collections::HashSet, env, sync::Arc};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

const ROLE_POLICY_ADMIN: &str = "policy-admin";
const ROLE_POLICY_EDITOR: &str = "policy-editor";
const ROLE_POLICY_VIEWER: &str = "policy-viewer";
const ROLE_REVIEWER: &str = "review-approver";
const ROLE_AUDITOR: &str = "auditor";

#[derive(Clone)]
pub struct AdminAuth {
    static_token: Option<String>,
    fallback_roles: Vec<String>,
    allow_claim_fallback: bool,
    jwt: Option<JwtValidator>,
    iam: Arc<IamService>,
}

#[derive(Clone, Debug, Deserialize, Default)]
#[serde(default)]
pub struct AuthSettings {
    pub static_roles: Option<Vec<String>>,
    pub oidc_issuer: Option<String>,
    pub oidc_audience: Option<String>,
    pub oidc_hs256_secret: Option<String>,
    #[serde(default = "default_allow_claim_fallback")]
    pub allow_claim_fallback: bool,
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

const fn default_allow_claim_fallback() -> bool {
    true
}

impl AdminAuth {
    pub async fn from_config(
        static_token: Option<String>,
        settings: AuthSettings,
        iam: Arc<IamService>,
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
            fallback_roles: merged
                .static_roles
                .clone()
                .unwrap_or_else(default_static_roles),
            allow_claim_fallback: merged.allow_claim_fallback,
            jwt,
            iam,
        })
    }

    pub async fn authenticate(
        &self,
        bearer: Option<String>,
        admin: Option<String>,
    ) -> Result<UserContext, StatusCode> {
        if let Some(jwt) = &self.jwt {
            if let Some(token) = bearer {
                let claims = jwt.validate(&token).map_err(|err| map_auth_error(&err))?;
                return self
                    .resolve_claims(claims)
                    .await
                    .map_err(|err| map_auth_error(&err));
            }
        }

        if let Some(provided) = admin {
            if let Some(service) = self
                .iam
                .verify_service_token(&provided)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            {
                return Ok(UserContext::from_service_account(service));
            }
            if let Some(expected) = self.static_token.as_deref() {
                if provided == expected {
                    return Ok(UserContext::from_fallback(
                        "static-admin".into(),
                        self.fallback_roles.clone(),
                    ));
                }
            }
            return Err(StatusCode::UNAUTHORIZED);
        }

        if self.static_token.is_some() {
            return Err(StatusCode::UNAUTHORIZED);
        }

        Ok(UserContext::system())
    }

    async fn resolve_claims(&self, claims: Claims) -> Result<UserContext, AuthError> {
        let actor = claims.actor();
        if let Some(user) = self.iam.find_user_by_subject(&claims.sub).await? {
            if user.status != "active" {
                return Err(AuthError::Disabled);
            }
            let access = self.iam.effective_permissions_for_user(user.id).await?;
            return Ok(UserContext::from_effective(
                actor.clone(),
                PrincipalType::User,
                Some(user.id),
                access,
            ));
        }

        if let Some(email) = &claims.email {
            if let Some(user) = self.iam.find_user_by_email(email).await? {
                if user.status != "active" {
                    return Err(AuthError::Disabled);
                }
                let access = self.iam.effective_permissions_for_user(user.id).await?;
                return Ok(UserContext::from_effective(
                    actor.clone(),
                    PrincipalType::User,
                    Some(user.id),
                    access,
                ));
            }
        }

        if self.allow_claim_fallback {
            let roles = claims.compute_roles();
            return Ok(UserContext::from_fallback(actor, roles));
        }

        Err(AuthError::NotProvisioned)
    }
}

pub async fn enforce_admin(
    ctx: Arc<AdminAuth>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let bearer = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|hv| hv.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(|token| token.to_string());
    let admin = req
        .headers()
        .get("X-Admin-Token")
        .and_then(|hv| hv.to_str().ok())
        .map(|value| value.to_string());
    let user = ctx.authenticate(bearer, admin).await?;
    req.extensions_mut().insert(user);
    Ok(next.run(req).await)
}

#[derive(Clone, Debug, Serialize)]
pub struct UserContext {
    pub actor: String,
    pub principal_type: PrincipalType,
    pub principal_id: Option<Uuid>,
    roles: HashSet<String>,
    permissions: HashSet<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalType {
    System,
    User,
    ServiceAccount,
    Fallback,
}

impl UserContext {
    pub fn system() -> Self {
        let roles: HashSet<String> = default_static_roles().into_iter().collect();
        Self {
            actor: "system".into(),
            principal_type: PrincipalType::System,
            principal_id: None,
            permissions: roles.clone(),
            roles,
        }
    }

    pub fn from_fallback(actor: String, roles: Vec<String>) -> Self {
        let role_set: HashSet<String> = roles.into_iter().collect();
        Self {
            actor,
            principal_type: PrincipalType::Fallback,
            principal_id: None,
            permissions: role_set.clone(),
            roles: role_set,
        }
    }

    pub fn from_effective(
        actor: String,
        principal_type: PrincipalType,
        principal_id: Option<Uuid>,
        access: EffectiveAccess,
    ) -> Self {
        Self {
            actor,
            principal_type,
            principal_id,
            roles: access.roles,
            permissions: access.permissions,
        }
    }

    pub fn from_service_account(principal: ServiceAccountPrincipal) -> Self {
        Self {
            actor: principal.name.clone(),
            principal_type: PrincipalType::ServiceAccount,
            principal_id: Some(principal.id),
            roles: principal.roles,
            permissions: principal.permissions,
        }
    }

    pub fn has_role(&self, role: &str) -> bool {
        self.roles.contains(role)
    }

    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.contains(permission)
    }

    pub fn roles_list(&self) -> Vec<String> {
        self.roles.iter().cloned().collect()
    }

    pub fn permissions_list(&self) -> Vec<String> {
        self.permissions.iter().cloned().collect()
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

    fn validate(&self, token: &str) -> Result<Claims, AuthError> {
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
        Ok(claims)
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

#[derive(Clone, Debug, serde::Deserialize)]
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

    fn actor(&self) -> String {
        self.preferred_username
            .clone()
            .or(self.email.clone())
            .unwrap_or_else(|| self.sub.clone())
    }
}

enum AuthError {
    InvalidFormat,
    InvalidSignature,
    InvalidIssuer,
    InvalidAudience,
    Expired,
    NotProvisioned,
    Disabled,
    Database(String),
    Internal(String),
}

impl From<IamError> for AuthError {
    fn from(err: IamError) -> Self {
        match err {
            IamError::NotFound(_) => AuthError::NotProvisioned,
            IamError::Validation(msg) => AuthError::Internal(msg),
            IamError::Db(e) => AuthError::Database(e.to_string()),
            IamError::Crypto(e) => AuthError::Internal(e),
        }
    }
}

fn map_auth_error(err: &AuthError) -> StatusCode {
    match err {
        AuthError::InvalidFormat
        | AuthError::InvalidSignature
        | AuthError::InvalidIssuer
        | AuthError::InvalidAudience
        | AuthError::Expired => StatusCode::UNAUTHORIZED,
        AuthError::NotProvisioned | AuthError::Disabled => StatusCode::FORBIDDEN,
        AuthError::Database(_) | AuthError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
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
pub const ROLE_POLICY_VIEW: &[&str] = &[ROLE_POLICY_ADMIN, ROLE_POLICY_EDITOR, ROLE_POLICY_VIEWER];
pub const ROLE_POLICY_EDIT: &[&str] = &[ROLE_POLICY_ADMIN, ROLE_POLICY_EDITOR];
pub const ROLE_POLICY_PUBLISH: &[&str] = &[ROLE_POLICY_ADMIN];
pub const ROLE_TAXONOMY_EDIT: &[&str] = &[ROLE_POLICY_ADMIN, ROLE_POLICY_EDITOR];
pub const ROLE_REPORTING_VIEW: &[&str] = &[ROLE_POLICY_ADMIN, ROLE_POLICY_VIEWER, ROLE_AUDITOR];
pub const ROLE_CACHE_ADMIN: &[&str] = &[ROLE_POLICY_ADMIN];
pub const ROLE_AUDIT_VIEW: &[&str] = &[ROLE_POLICY_ADMIN, ROLE_AUDITOR];
pub const ROLE_IAM_VIEW: &[&str] = &[ROLE_POLICY_ADMIN, ROLE_AUDITOR];
pub const ROLE_IAM_ADMIN: &[&str] = &[ROLE_POLICY_ADMIN];
