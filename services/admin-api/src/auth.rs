use crate::iam::{
    EffectiveAccess, IamError, IamService, LocalAuthenticatedUser, ServiceAccountPrincipal,
};
use crate::{ApiError, AppState};
use axum::{
    body::Body,
    extract::State,
    http::{header, Request, StatusCode},
    middleware::Next,
    response::Response,
    Json,
};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::Sha256;
use std::{collections::HashSet, env, sync::Arc};
use tracing::error;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

const ROLE_POLICY_ADMIN: &str = "policy-admin";
const ROLE_POLICY_EDITOR: &str = "policy-editor";
const ROLE_POLICY_VIEWER: &str = "policy-viewer";
const ROLE_AUDITOR: &str = "auditor";

#[derive(Clone)]
pub struct AdminAuth {
    static_token: Option<String>,
    fallback_roles: Vec<String>,
    mode: AuthMode,
    allow_claim_fallback: bool,
    local_jwt: LocalJwtIssuer,
    max_failed_attempts: i32,
    lockout_seconds: i64,
    jwt: Option<JwtValidator>,
    iam: Arc<IamService>,
}

#[derive(Clone, Debug, Deserialize, Default)]
#[serde(default)]
pub struct AuthSettings {
    #[serde(default)]
    pub mode: AuthMode,
    pub static_roles: Option<Vec<String>>,
    pub oidc_issuer: Option<String>,
    pub oidc_audience: Option<String>,
    pub oidc_hs256_secret: Option<String>,
    #[serde(default = "default_allow_claim_fallback")]
    pub allow_claim_fallback: bool,
    pub local_jwt_secret: Option<String>,
    #[serde(default = "default_local_jwt_ttl_seconds")]
    pub local_jwt_ttl_seconds: i64,
    #[serde(default = "default_max_failed_attempts")]
    pub max_failed_attempts: i32,
    #[serde(default = "default_lockout_seconds")]
    pub lockout_seconds: i64,
}

#[derive(Clone, Debug, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    #[default]
    Local,
    Hybrid,
    Oidc,
}

impl AuthSettings {
    pub fn merge_env(mut self) -> Self {
        if let Ok(mode) = env::var("OD_AUTH_MODE") {
            self.mode = match mode.to_ascii_lowercase().as_str() {
                "oidc" => AuthMode::Oidc,
                "hybrid" => AuthMode::Hybrid,
                _ => AuthMode::Local,
            };
        }
        if let Ok(secret) = env::var("OD_OIDC_HS256_SECRET") {
            self.oidc_hs256_secret = Some(secret);
        }
        if let Ok(issuer) = env::var("OD_OIDC_ISSUER") {
            self.oidc_issuer = Some(issuer);
        }
        if let Ok(aud) = env::var("OD_OIDC_AUDIENCE") {
            self.oidc_audience = Some(aud);
        }
        if let Ok(secret) = env::var("OD_LOCAL_AUTH_JWT_SECRET") {
            self.local_jwt_secret = Some(secret);
        }
        if let Ok(value) = env::var("OD_LOCAL_AUTH_TTL_SECONDS") {
            if let Ok(parsed) = value.parse::<i64>() {
                self.local_jwt_ttl_seconds = parsed.max(300);
            }
        }
        if let Ok(value) = env::var("OD_LOCAL_AUTH_MAX_FAILED_ATTEMPTS") {
            if let Ok(parsed) = value.parse::<i32>() {
                self.max_failed_attempts = parsed.max(1);
            }
        }
        if let Ok(value) = env::var("OD_LOCAL_AUTH_LOCKOUT_SECONDS") {
            if let Ok(parsed) = value.parse::<i64>() {
                self.lockout_seconds = parsed.max(30);
            }
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

const fn default_local_jwt_ttl_seconds() -> i64 {
    60 * 60
}

const fn default_max_failed_attempts() -> i32 {
    5
}

const fn default_lockout_seconds() -> i64 {
    15 * 60
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
            mode: merged.mode,
            allow_claim_fallback: merged.allow_claim_fallback,
            local_jwt: LocalJwtIssuer::new(
                merged
                    .local_jwt_secret
                    .unwrap_or_else(|| "od-local-dev-secret-change-me".to_string()),
                merged.local_jwt_ttl_seconds,
            ),
            max_failed_attempts: merged.max_failed_attempts,
            lockout_seconds: merged.lockout_seconds,
            jwt,
            iam,
        })
    }

    pub async fn authenticate(
        &self,
        bearer: Option<String>,
        admin: Option<String>,
    ) -> Result<UserContext, StatusCode> {
        if let Some(token) = bearer {
            if matches!(self.mode, AuthMode::Local | AuthMode::Hybrid) {
                if let Ok(local_claims) = self.local_jwt.validate(&token) {
                    return self
                        .resolve_local_subject(local_claims)
                        .await
                        .map_err(|err| map_auth_error(&err));
                }
            }

            if matches!(self.mode, AuthMode::Oidc | AuthMode::Hybrid) {
                if let Some(jwt) = &self.jwt {
                    let claims = jwt.validate(&token).map_err(|err| map_auth_error(&err))?;
                    return self
                        .resolve_claims(claims)
                        .await
                        .map_err(|err| map_auth_error(&err));
                }
            }

            return Err(StatusCode::UNAUTHORIZED);
        }

        if let Some(provided) = admin {
            if let Some(user_token_principal) = self
                .iam
                .verify_user_token(&provided)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            {
                return Ok(UserContext::from_effective(
                    user_token_principal
                        .username
                        .clone()
                        .unwrap_or_else(|| user_token_principal.email.clone()),
                    PrincipalType::User,
                    Some(user_token_principal.id),
                    EffectiveAccess {
                        roles: user_token_principal.roles,
                        permissions: user_token_principal.permissions,
                    },
                ));
            }
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

        Err(StatusCode::UNAUTHORIZED)
    }

    async fn resolve_local_subject(
        &self,
        claims: LocalTokenClaims,
    ) -> Result<UserContext, AuthError> {
        let user = self
            .iam
            .get_user(claims.sub)
            .await
            .map_err(|_| AuthError::NotProvisioned)?;
        if user.status != "active" {
            return Err(AuthError::Disabled);
        }
        let access = self.iam.effective_permissions_for_user(user.id).await?;
        Ok(UserContext::from_effective(
            claims.username.unwrap_or_else(|| {
                user.username
                    .clone()
                    .or(user.email.clone())
                    .unwrap_or_else(|| user.id.to_string())
            }),
            PrincipalType::User,
            Some(user.id),
            access,
        ))
    }

    async fn login_local(
        &self,
        username: &str,
        password: &str,
    ) -> Result<LocalLoginSuccess, AuthError> {
        if !matches!(self.mode, AuthMode::Local | AuthMode::Hybrid) {
            return Err(AuthError::Internal(
                "local authentication is disabled in current auth mode".into(),
            ));
        }

        let user = self
            .iam
            .authenticate_local_user(
                username,
                password,
                self.max_failed_attempts,
                self.lockout_seconds,
            )
            .await?;
        let access_token = self.local_jwt.issue(&user)?;
        Ok(LocalLoginSuccess {
            access_token,
            expires_in: self.local_jwt.ttl_seconds,
            user,
        })
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

    pub fn mode(&self) -> &AuthMode {
        &self.mode
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

#[derive(Debug, Deserialize)]
pub struct LocalLoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LocalLoginResponse {
    pub access_token: String,
    pub expires_in: i64,
    pub user: LocalLoginUser,
}

#[derive(Debug, Serialize)]
pub struct AuthModeResponse {
    pub mode: String,
}

#[derive(Debug, Serialize)]
pub struct LocalLoginUser {
    pub id: Uuid,
    pub username: Option<String>,
    pub email: String,
    pub display_name: Option<String>,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
    pub must_change_password: bool,
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

pub async fn login_route(
    State(state): State<AppState>,
    Json(payload): Json<LocalLoginRequest>,
) -> Result<Json<LocalLoginResponse>, (StatusCode, Json<ApiError>)> {
    if payload.username.trim().is_empty() || payload.password.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                "VALIDATION_ERROR",
                "username and password are required",
            )),
        ));
    }

    let login = state
        .admin_auth()
        .login_local(payload.username.trim(), payload.password.as_str())
        .await
        .map_err(map_login_error)?;

    state
        .log_iam_event(
            "iam.auth.login.success",
            Some(login.user.email.clone()),
            "user",
            Some(login.user.id.to_string()),
            json!({"username": login.user.username}),
        )
        .await;

    Ok(Json(LocalLoginResponse {
        access_token: login.access_token,
        expires_in: login.expires_in,
        user: LocalLoginUser {
            id: login.user.id,
            username: login.user.username,
            email: login.user.email,
            display_name: login.user.display_name,
            roles: login.user.roles.into_iter().collect(),
            permissions: login.user.permissions.into_iter().collect(),
            must_change_password: login.user.must_change_password,
        },
    }))
}

pub async fn auth_mode_route(
    State(state): State<AppState>,
) -> Result<Json<AuthModeResponse>, (StatusCode, Json<ApiError>)> {
    let mode = match state.admin_auth().mode() {
        AuthMode::Local => "local",
        AuthMode::Hybrid => "hybrid",
        AuthMode::Oidc => "oidc",
    };
    Ok(Json(AuthModeResponse {
        mode: mode.to_string(),
    }))
}

pub async fn change_password_route(
    State(state): State<AppState>,
    axum::Extension(user): axum::Extension<UserContext>,
    Json(payload): Json<ChangePasswordRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let Some(user_id) = user.principal_id else {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ApiError::new(
                "FORBIDDEN",
                "password change is only supported for authenticated user principals",
            )),
        ));
    };

    if payload.current_password.trim().is_empty() || payload.new_password.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                "VALIDATION_ERROR",
                "current_password and new_password are required",
            )),
        ));
    }

    state
        .iam()
        .change_password(
            user_id,
            payload.current_password.as_str(),
            payload.new_password.as_str(),
        )
        .await
        .map_err(|err| {
            let (status, error) = crate::iam::map_iam_error(err);
            (status, error)
        })?;

    state
        .log_iam_event(
            "iam.auth.password.change",
            Some(user.actor.clone()),
            "user",
            Some(user_id.to_string()),
            json!({}),
        )
        .await;

    Ok(StatusCode::NO_CONTENT)
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

    #[allow(dead_code)]
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

#[derive(Clone)]
struct LocalJwtIssuer {
    secret: Vec<u8>,
    ttl_seconds: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct LocalTokenClaims {
    sub: Uuid,
    username: Option<String>,
    email: String,
    iss: String,
    aud: String,
    exp: i64,
    iat: i64,
}

impl LocalJwtIssuer {
    fn new(secret: String, ttl_seconds: i64) -> Self {
        Self {
            secret: secret.into_bytes(),
            ttl_seconds: ttl_seconds.max(300),
        }
    }

    fn issue(&self, user: &LocalAuthenticatedUser) -> Result<String, AuthError> {
        let header = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&json!({"alg": "HS256", "typ": "JWT"}))
                .map_err(|_| AuthError::InvalidFormat)?,
        );
        let now = Utc::now().timestamp();
        let payload_claims = LocalTokenClaims {
            sub: user.id,
            username: user.username.clone(),
            email: user.email.clone(),
            iss: "od-local".to_string(),
            aud: "od-admin-ui".to_string(),
            exp: now + self.ttl_seconds,
            iat: now,
        };
        let payload = URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload_claims).map_err(|_| AuthError::InvalidFormat)?);
        let mut mac =
            HmacSha256::new_from_slice(&self.secret).map_err(|_| AuthError::InvalidFormat)?;
        let signing_input = format!("{}.{}", header, payload);
        mac.update(signing_input.as_bytes());
        let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        Ok(format!("{}.{}", signing_input, signature))
    }

    fn validate(&self, token: &str) -> Result<LocalTokenClaims, AuthError> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(AuthError::InvalidFormat);
        }
        verify_signature(&self.secret, parts[0], parts[1], parts[2])?;
        let payload = decode_segment(parts[1])?;
        let claims: LocalTokenClaims =
            serde_json::from_slice(&payload).map_err(|_| AuthError::InvalidFormat)?;
        if claims.iss != "od-local" || claims.aud != "od-admin-ui" {
            return Err(AuthError::InvalidAudience);
        }
        if claims.exp <= Utc::now().timestamp() {
            return Err(AuthError::Expired);
        }
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

#[derive(Debug)]
enum AuthError {
    InvalidFormat,
    InvalidSignature,
    InvalidIssuer,
    InvalidAudience,
    Expired,
    InvalidCredentials,
    Locked(String),
    NotProvisioned,
    Disabled,
    Database(String),
    Internal(String),
}

struct LocalLoginSuccess {
    access_token: String,
    expires_in: i64,
    user: LocalAuthenticatedUser,
}

impl From<IamError> for AuthError {
    fn from(err: IamError) -> Self {
        match err {
            IamError::NotFound(_) => AuthError::NotProvisioned,
            IamError::Validation(msg) => AuthError::Internal(msg),
            IamError::Db(e) => AuthError::Database(e.to_string()),
            IamError::Crypto(e) => AuthError::Internal(e),
            IamError::InvalidCredentials => AuthError::InvalidCredentials,
            IamError::Disabled => AuthError::Disabled,
            IamError::Locked(until) => AuthError::Locked(until.to_rfc3339()),
        }
    }
}

fn map_auth_error(err: &AuthError) -> StatusCode {
    match err {
        AuthError::InvalidFormat
        | AuthError::InvalidSignature
        | AuthError::InvalidIssuer
        | AuthError::InvalidAudience
        | AuthError::Expired
        | AuthError::InvalidCredentials => StatusCode::UNAUTHORIZED,
        AuthError::Locked(_) => StatusCode::TOO_MANY_REQUESTS,
        AuthError::NotProvisioned | AuthError::Disabled => StatusCode::FORBIDDEN,
        AuthError::Database(msg) | AuthError::Internal(msg) => {
            error!(target = "svc-admin", %msg, "auth middleware failure");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

fn map_login_error(err: AuthError) -> (StatusCode, Json<ApiError>) {
    match err {
        AuthError::Disabled => (
            StatusCode::FORBIDDEN,
            Json(ApiError::new("ACCOUNT_DISABLED", "account disabled")),
        ),
        AuthError::NotProvisioned
        | AuthError::InvalidCredentials
        | AuthError::InvalidFormat
        | AuthError::InvalidSignature
        | AuthError::InvalidIssuer
        | AuthError::InvalidAudience
        | AuthError::Expired => (
            StatusCode::UNAUTHORIZED,
            Json(ApiError::new("INVALID_CREDENTIALS", "invalid credentials")),
        ),
        AuthError::Locked(until) => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(ApiError::new(
                "ACCOUNT_LOCKED",
                format!("account locked until {}", until),
            )),
        ),
        AuthError::Database(msg) | AuthError::Internal(msg) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("AUTH_ERROR", msg)),
        ),
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
pub const ROLE_POLICY_VIEW: &[&str] = &[ROLE_POLICY_ADMIN, ROLE_POLICY_EDITOR, ROLE_POLICY_VIEWER];
pub const ROLE_POLICY_EDIT: &[&str] = &[ROLE_POLICY_ADMIN, ROLE_POLICY_EDITOR];
pub const ROLE_POLICY_PUBLISH: &[&str] = &[ROLE_POLICY_ADMIN];
pub const ROLE_TAXONOMY_EDIT: &[&str] = &[ROLE_POLICY_ADMIN, ROLE_POLICY_EDITOR];
pub const ROLE_REPORTING_VIEW: &[&str] = &[ROLE_POLICY_ADMIN, ROLE_POLICY_VIEWER, ROLE_AUDITOR];
pub const ROLE_CACHE_ADMIN: &[&str] = &[ROLE_POLICY_ADMIN];
pub const ROLE_AUDIT_VIEW: &[&str] = &[ROLE_POLICY_ADMIN, ROLE_AUDITOR];
pub const ROLE_IAM_VIEW: &[&str] = &[ROLE_POLICY_ADMIN, ROLE_AUDITOR];
pub const ROLE_IAM_ADMIN: &[&str] = &[ROLE_POLICY_ADMIN];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_roles_accepts_any_matching_role() {
        let ctx = UserContext::from_fallback(
            "alice".into(),
            vec![ROLE_POLICY_VIEWER.into(), ROLE_AUDITOR.into()],
        );
        assert!(require_roles(&ctx, ROLE_REPORTING_VIEW).is_ok());
        assert_eq!(
            require_roles(&ctx, ROLE_IAM_ADMIN),
            Err(StatusCode::FORBIDDEN)
        );
    }

    #[test]
    fn service_account_permissions_round_trip() {
        let mut roles = HashSet::new();
        roles.insert(ROLE_POLICY_ADMIN.into());
        let mut permissions = HashSet::new();
        permissions.insert("iam:manage".into());
        let ctx = UserContext::from_service_account(ServiceAccountPrincipal {
            id: Uuid::new_v4(),
            name: "svc-ci".into(),
            roles,
            permissions,
        });
        assert!(ctx.has_role(ROLE_POLICY_ADMIN));
        assert!(ctx.has_permission("iam:manage"));
    }

    #[test]
    fn local_jwt_round_trip() {
        let mut roles = HashSet::new();
        roles.insert("policy-admin".to_string());
        let mut permissions = HashSet::new();
        permissions.insert("iam:manage".to_string());
        let user = LocalAuthenticatedUser {
            id: Uuid::new_v4(),
            username: Some("admin".into()),
            email: "admin@local".into(),
            display_name: Some("Default Admin".into()),
            roles,
            permissions,
            must_change_password: true,
        };

        let issuer = LocalJwtIssuer::new("test-secret".into(), 3600);
        let token = issuer.issue(&user).expect("issue token");
        let claims = issuer.validate(&token).expect("validate token");
        assert_eq!(claims.sub, user.id);
        assert_eq!(claims.username.as_deref(), Some("admin"));
        assert_eq!(claims.email, "admin@local");
    }
}
