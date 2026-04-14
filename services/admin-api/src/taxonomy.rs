use std::collections::HashMap;

use axum::{
    extract::{OriginalUri, State},
    http::{Method, StatusCode, Uri},
    Extension, Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, Row};
use tracing::error;
use uuid::Uuid;

use crate::{
    auth::{require_roles, UserContext, ROLE_POLICY_VIEW, ROLE_TAXONOMY_EDIT},
    metrics::record_taxonomy_activation_change,
    ApiError, AppState,
};
use ::taxonomy::{CanonicalCategory, CanonicalSubcategory, CanonicalTaxonomy};

const CATEGORY_SENTINEL_ID: &str = "__CATEGORY__";
const SYSTEM_ACTOR: &str = "system";

pub async fn get_taxonomy(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
) -> Result<Json<TaxonomyResponse>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_POLICY_VIEW)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;

    let taxonomy = state.canonical_taxonomy();
    assert_no_reserved_ids(&taxonomy)?;
    let profile = ensure_activation_profile(state.pool(), &taxonomy)
        .await
        .map_err(db_error)?;

    let categories = taxonomy
        .categories
        .iter()
        .map(|category| map_category_response(category, &profile))
        .collect();

    Ok(Json(TaxonomyResponse {
        version: profile.version.clone(),
        updated_at: profile.updated_at,
        updated_by: profile.updated_by.clone(),
        categories,
    }))
}

pub async fn update_taxonomy_activation(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Json(payload): Json<ActivationUpdateRequest>,
) -> Result<Json<ActivationSaveResponse>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_TAXONOMY_EDIT)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;

    let taxonomy = state.canonical_taxonomy();
    assert_no_reserved_ids(&taxonomy)?;
    let validated = validate_activation_payload(payload, &taxonomy)?;
    let profile = ensure_activation_profile(state.pool(), &taxonomy)
        .await
        .map_err(db_error)?;
    let actor = user.actor.clone();

    let updated =
        persist_activation_profile(state.pool(), profile.id, &validated, &taxonomy, &actor)
            .await
            .map_err(db_error)?;

    state
        .log_policy_event(
            "taxonomy.activation.update",
            Some(actor),
            Some(updated.id.to_string()),
            &validated,
        )
        .await;
    state.trigger_policy_reload().await.map_err(|err| {
        error!(
            target = "svc-admin",
            %err,
            "failed to trigger immediate policy-engine reload after taxonomy update"
        );
        (
            StatusCode::BAD_GATEWAY,
            Json(ApiError::new(
                "POLICY_RELOAD_FAILED",
                "taxonomy update persisted but policy-engine reload failed",
            )),
        )
    })?;
    state.invalidate_policy_cache().await;
    record_taxonomy_activation_change();

    Ok(Json(ActivationSaveResponse {
        version: updated.version.clone(),
        updated_at: updated.updated_at,
        updated_by: updated.updated_by.clone(),
    }))
}

pub async fn block_category_mutation(
    method: Method,
    OriginalUri(uri): OriginalUri,
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    handle_mutation_request(method, uri, user, state, "category").await
}

pub async fn block_subcategory_mutation(
    method: Method,
    OriginalUri(uri): OriginalUri,
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    handle_mutation_request(method, uri, user, state, "subcategory").await
}

#[derive(Debug, Serialize)]
pub(crate) struct TaxonomyResponse {
    version: String,
    updated_at: DateTime<Utc>,
    updated_by: Option<String>,
    categories: Vec<TaxonomyCategoryResponse>,
}

#[derive(Debug, Serialize)]
struct TaxonomyCategoryResponse {
    id: String,
    name: String,
    enabled: bool,
    effective_enabled: bool,
    locked: bool,
    subcategories: Vec<TaxonomySubcategoryResponse>,
}

#[derive(Debug, Serialize)]
struct TaxonomySubcategoryResponse {
    id: String,
    name: String,
    enabled: bool,
    effective_enabled: bool,
    locked: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct ActivationSaveResponse {
    version: String,
    updated_at: DateTime<Utc>,
    updated_by: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ActivationUpdateRequest {
    version: String,
    categories: Vec<ActivationCategoryInput>,
}

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct ActivationCategoryInput {
    id: String,
    enabled: bool,
    #[serde(default)]
    subcategories: Vec<ActivationSubcategoryInput>,
}

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct ActivationSubcategoryInput {
    id: String,
    enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
struct ValidatedActivationPayload {
    version: String,
    categories: Vec<ValidatedActivationCategory>,
}

#[derive(Debug, Clone, Serialize)]
struct ValidatedActivationCategory {
    id: String,
    enabled: bool,
    subcategories: Vec<ValidatedActivationSubcategory>,
}

#[derive(Debug, Clone, Serialize)]
struct ValidatedActivationSubcategory {
    id: String,
    enabled: bool,
}

#[derive(Debug, Clone)]
struct ActivationProfile {
    id: Uuid,
    version: String,
    updated_by: Option<String>,
    updated_at: DateTime<Utc>,
    category_states: HashMap<String, bool>,
    subcategory_states: HashMap<String, HashMap<String, bool>>,
}

impl ActivationProfile {
    fn category_enabled(&self, category_id: &str) -> bool {
        self.category_states
            .get(category_id)
            .copied()
            .unwrap_or(true)
    }

    fn subcategory_enabled(&self, category_id: &str, subcategory_id: &str) -> Option<bool> {
        self.subcategory_states
            .get(category_id)
            .and_then(|subs| subs.get(subcategory_id))
            .copied()
    }
}

fn map_category_response(
    category: &CanonicalCategory,
    profile: &ActivationProfile,
) -> TaxonomyCategoryResponse {
    let locked = category.always_enabled.unwrap_or(false);
    let stored_enabled = profile.category_enabled(&category.id);
    let enabled = if locked { true } else { stored_enabled };

    let subcategories = category
        .subcategories
        .iter()
        .map(|sub| map_subcategory_response(sub, &category.id, enabled, locked, profile))
        .collect();

    TaxonomyCategoryResponse {
        id: category.id.clone(),
        name: category.name.clone(),
        enabled,
        effective_enabled: enabled,
        locked,
        subcategories,
    }
}

fn map_subcategory_response(
    sub: &CanonicalSubcategory,
    category_id: &str,
    parent_enabled: bool,
    parent_locked: bool,
    profile: &ActivationProfile,
) -> TaxonomySubcategoryResponse {
    let locked = parent_locked || sub.always_enabled.unwrap_or(false);
    let stored = profile
        .subcategory_enabled(category_id, &sub.id)
        .unwrap_or(parent_enabled);
    let enabled = if locked { true } else { stored };
    let effective_enabled = enabled;

    TaxonomySubcategoryResponse {
        id: sub.id.clone(),
        name: sub.name.clone(),
        enabled,
        effective_enabled,
        locked,
    }
}

fn validate_activation_payload(
    payload: ActivationUpdateRequest,
    taxonomy: &CanonicalTaxonomy,
) -> Result<ValidatedActivationPayload, (StatusCode, Json<ApiError>)> {
    let requested_version = payload.version.trim();
    if requested_version != taxonomy.version {
        return Err((
            StatusCode::CONFLICT,
            Json(ApiError::new(
                "VERSION_CONFLICT",
                format!(
                    "taxonomy version mismatch: expected {}, got {}",
                    taxonomy.version, requested_version
                ),
            )),
        ));
    }

    let mut by_category: HashMap<String, ActivationCategoryInput> = HashMap::new();
    for category in payload.categories {
        if by_category.insert(category.id.clone(), category).is_some() {
            return Err(crate::validation_error("duplicate category id in payload"));
        }
    }

    let mut validated_categories = Vec::new();
    for canonical in &taxonomy.categories {
        let Some(input) = by_category.remove(&canonical.id) else {
            return Err(crate::validation_error(&format!(
                "missing category {} in payload",
                canonical.id
            )));
        };
        if canonical.always_enabled.unwrap_or(false) && !input.enabled {
            return Err(crate::validation_error(&format!(
                "category {} cannot be disabled",
                canonical.id
            )));
        }

        let validated_subs = validate_subcategories(canonical, &input)?;
        validated_categories.push(ValidatedActivationCategory {
            id: canonical.id.clone(),
            enabled: input.enabled,
            subcategories: validated_subs,
        });
    }

    if !by_category.is_empty() {
        let extras: Vec<String> = by_category.keys().cloned().collect();
        return Err(crate::validation_error(&format!(
            "unknown categories in payload: {}",
            extras.join(", ")
        )));
    }

    Ok(ValidatedActivationPayload {
        version: taxonomy.version.clone(),
        categories: validated_categories,
    })
}

fn validate_subcategories(
    canonical: &CanonicalCategory,
    input: &ActivationCategoryInput,
) -> Result<Vec<ValidatedActivationSubcategory>, (StatusCode, Json<ApiError>)> {
    let mut by_sub: HashMap<String, ActivationSubcategoryInput> = HashMap::new();
    for sub in &input.subcategories {
        if by_sub.insert(sub.id.clone(), sub.clone()).is_some() {
            return Err(crate::validation_error(&format!(
                "duplicate subcategory {} in category {}",
                sub.id, canonical.id
            )));
        }
    }

    let mut validated = Vec::new();
    for canonical_sub in &canonical.subcategories {
        let Some(sub_input) = by_sub.remove(&canonical_sub.id) else {
            return Err(crate::validation_error(&format!(
                "missing subcategory {} in category {}",
                canonical_sub.id, canonical.id
            )));
        };
        if canonical_sub.always_enabled.unwrap_or(false) && !sub_input.enabled {
            return Err(crate::validation_error(&format!(
                "subcategory {} cannot be disabled",
                canonical_sub.id
            )));
        }
        validated.push(ValidatedActivationSubcategory {
            id: canonical_sub.id.clone(),
            enabled: sub_input.enabled,
        });
    }

    if !by_sub.is_empty() {
        let extras: Vec<String> = by_sub.keys().cloned().collect();
        return Err(crate::validation_error(&format!(
            "unknown subcategories in category {}: {}",
            canonical.id,
            extras.join(", ")
        )));
    }

    Ok(validated)
}

async fn ensure_activation_profile(
    pool: &PgPool,
    taxonomy: &CanonicalTaxonomy,
) -> Result<ActivationProfile, sqlx::Error> {
    if let Some(profile) = load_activation_profile(pool).await? {
        synchronize_profile(pool, profile, taxonomy).await
    } else {
        seed_default_profile(pool, taxonomy).await
    }
}

async fn synchronize_profile(
    pool: &PgPool,
    mut profile: ActivationProfile,
    taxonomy: &CanonicalTaxonomy,
) -> Result<ActivationProfile, sqlx::Error> {
    let mut inserts = Vec::new();
    for category in &taxonomy.categories {
        if !profile.category_states.contains_key(&category.id) {
            profile.category_states.insert(category.id.clone(), true);
            inserts.push((category.id.clone(), CATEGORY_SENTINEL_ID.to_string(), true));
        }

        let subs = profile
            .subcategory_states
            .entry(category.id.clone())
            .or_default();
        for sub in &category.subcategories {
            if !subs.contains_key(&sub.id) {
                subs.insert(sub.id.clone(), true);
                inserts.push((category.id.clone(), sub.id.clone(), true));
            }
        }
    }

    let version_outdated = profile.version != taxonomy.version;
    if inserts.is_empty() && !version_outdated {
        return Ok(profile);
    }

    let mut tx = pool.begin().await?;
    for (category_id, subcategory_id, enabled) in inserts {
        sqlx::query(
            "INSERT INTO taxonomy_activation_entries (profile_id, category_id, subcategory_id, enabled) VALUES ($1, $2, $3, $4)",
        )
        .bind(profile.id)
        .bind(&category_id)
        .bind(&subcategory_id)
        .bind(enabled)
        .execute(&mut *tx)
        .await?;
    }

    if version_outdated {
        sqlx::query("UPDATE taxonomy_activation_profiles SET version = $1 WHERE id = $2")
            .bind(&taxonomy.version)
            .bind(profile.id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;

    load_activation_profile(pool)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

async fn persist_activation_profile(
    pool: &PgPool,
    profile_id: Uuid,
    payload: &ValidatedActivationPayload,
    taxonomy: &CanonicalTaxonomy,
    actor: &str,
) -> Result<ActivationProfile, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let actor_value = Some(actor.to_string());
    sqlx::query(
        "UPDATE taxonomy_activation_profiles SET version = $1, updated_by = $2, updated_at = NOW() WHERE id = $3",
    )
    .bind(&taxonomy.version)
    .bind(actor_value)
    .bind(profile_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM taxonomy_activation_entries WHERE profile_id = $1")
        .bind(profile_id)
        .execute(&mut *tx)
        .await?;

    for category in &payload.categories {
        sqlx::query(
            "INSERT INTO taxonomy_activation_entries (profile_id, category_id, subcategory_id, enabled) VALUES ($1, $2, $3, $4)",
        )
        .bind(profile_id)
        .bind(&category.id)
        .bind(CATEGORY_SENTINEL_ID)
        .bind(category.enabled)
        .execute(&mut *tx)
        .await?;
        for sub in &category.subcategories {
            sqlx::query(
                "INSERT INTO taxonomy_activation_entries (profile_id, category_id, subcategory_id, enabled) VALUES ($1, $2, $3, $4)",
            )
            .bind(profile_id)
            .bind(&category.id)
            .bind(&sub.id)
            .bind(sub.enabled)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;

    load_activation_profile(pool)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

async fn seed_default_profile(
    pool: &PgPool,
    taxonomy: &CanonicalTaxonomy,
) -> Result<ActivationProfile, sqlx::Error> {
    let profile_id = Uuid::new_v4();
    let mut tx = pool.begin().await?;
    let actor_value = Some(SYSTEM_ACTOR.to_string());
    sqlx::query(
        "INSERT INTO taxonomy_activation_profiles (id, version, updated_by) VALUES ($1, $2, $3)",
    )
    .bind(profile_id)
    .bind(&taxonomy.version)
    .bind(actor_value)
    .execute(&mut *tx)
    .await?;

    for category in &taxonomy.categories {
        sqlx::query(
            "INSERT INTO taxonomy_activation_entries (profile_id, category_id, subcategory_id, enabled) VALUES ($1, $2, $3, $4)",
        )
        .bind(profile_id)
        .bind(&category.id)
        .bind(CATEGORY_SENTINEL_ID)
        .bind(true)
        .execute(&mut *tx)
        .await?;
        for sub in &category.subcategories {
            sqlx::query(
                "INSERT INTO taxonomy_activation_entries (profile_id, category_id, subcategory_id, enabled) VALUES ($1, $2, $3, $4)",
            )
            .bind(profile_id)
            .bind(&category.id)
            .bind(&sub.id)
            .bind(true)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;

    load_activation_profile(pool)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

async fn load_activation_profile(pool: &PgPool) -> Result<Option<ActivationProfile>, sqlx::Error> {
    let maybe_row = sqlx::query(
        "SELECT id, version, updated_by, updated_at FROM taxonomy_activation_profiles ORDER BY updated_at DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    let Some(row) = maybe_row else {
        return Ok(None);
    };

    let profile_id: Uuid = row.get("id");
    let entries = sqlx::query(
        "SELECT category_id, subcategory_id, enabled FROM taxonomy_activation_entries WHERE profile_id = $1",
    )
    .bind(profile_id)
    .fetch_all(pool)
    .await?;

    let mut category_states = HashMap::new();
    let mut subcategory_states: HashMap<String, HashMap<String, bool>> = HashMap::new();
    for entry in entries {
        let category_id: String = entry.get("category_id");
        let subcategory_id: String = entry.get("subcategory_id");
        let enabled: bool = entry.get("enabled");
        if subcategory_id == CATEGORY_SENTINEL_ID {
            category_states.insert(category_id, enabled);
        } else {
            subcategory_states
                .entry(category_id.clone())
                .or_default()
                .insert(subcategory_id, enabled);
        }
    }

    Ok(Some(ActivationProfile {
        id: profile_id,
        version: row.get("version"),
        updated_by: row.get("updated_by"),
        updated_at: row.get("updated_at"),
        category_states,
        subcategory_states,
    }))
}

fn assert_no_reserved_ids(
    taxonomy: &CanonicalTaxonomy,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    if taxonomy.categories.iter().any(|cat| {
        cat.subcategories
            .iter()
            .any(|sub| sub.id == CATEGORY_SENTINEL_ID)
    }) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(
                "SERVER_ERROR",
                format!(
                    "canonical taxonomy includes reserved id {}",
                    CATEGORY_SENTINEL_ID
                ),
            )),
        ));
    }
    Ok(())
}

fn db_error(err: sqlx::Error) -> (StatusCode, Json<ApiError>) {
    error!(target = "svc-admin", %err, "taxonomy activation query failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError::new("DB_ERROR", err.to_string())),
    )
}

#[cfg(test)]
mod support {
    use super::*;
    use common_types::normalizer::CanonicalizationPolicy;
    use crate::{
        audit::AuditLogger,
        auth::{AdminAuth, AuthSettings},
        iam::IamService,
        metrics::ReviewMetrics,
    };
    use ::taxonomy::TaxonomyStore;
    use anyhow::Result;
    use sqlx::postgres::PgPoolOptions;
    use std::sync::Arc;

    pub async fn build_test_state(db_url: &str, mutation_enabled: bool) -> Result<AppState> {
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(db_url)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        let iam = Arc::new(IamService::new(pool.clone()));
        let admin_auth = Arc::new(
            AdminAuth::from_config(
                Some("test-token".into()),
                AuthSettings::default(),
                iam.clone(),
            )
            .await
            .expect("admin auth"),
        );
        let audit_logger = AuditLogger::new(pool.clone(), None);
        let metrics = ReviewMetrics::new(4 * 60 * 60);
        let canonical_taxonomy =
            Arc::new(CanonicalTaxonomy::load_from_env().expect("canonical taxonomy should load"));
        let taxonomy_store = Arc::new(TaxonomyStore::new(canonical_taxonomy.clone()));

        Ok(AppState {
            pool,
            admin_auth,
            cache_invalidator: None,
            audit_logger,
            metrics,
            reporting_client: None,
            iam,
            canonical_taxonomy,
            taxonomy_store,
            taxonomy_mutation_enabled: mutation_enabled,
            policy_engine_url: "http://policy-engine:19010".to_string(),
            policy_engine_admin_token: Some("test-token".to_string()),
            llm_providers_url: "http://llm-worker:19015/providers".to_string(),
            prometheus_url: Some("http://prometheus:9090".to_string()),
            http_client: reqwest::Client::new(),
            classification_job_publisher: None,
            canonicalization_policy: Arc::new(CanonicalizationPolicy::default()),
        })
    }
}

async fn handle_mutation_request(
    method: Method,
    uri: Uri,
    user: UserContext,
    state: AppState,
    target: &'static str,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let mutation_enabled = state.taxonomy_mutation_enabled();
    state
        .log_policy_event(
            if mutation_enabled {
                "taxonomy.mutation.maintenance"
            } else {
                "taxonomy.mutation.blocked"
            },
            Some(user.actor.clone()),
            None,
            json!({
                "method": method.as_str(),
                "path": uri.to_string(),
                "target": target,
                "mutation_enabled": mutation_enabled,
            }),
        )
        .await;

    let (status, error) = if mutation_enabled {
        (
            StatusCode::NOT_IMPLEMENTED,
            ApiError::new(
                "TAXONOMY_MUTATION_UNSUPPORTED",
                "Maintenance mode enabled, but taxonomy structure is governed by config/canonical-taxonomy.json. Update the file and redeploy instead of calling legacy mutation endpoints.",
            ),
        )
    } else {
        (
            StatusCode::LOCKED,
            ApiError::new(
                "TAXONOMY_LOCKED",
                "Taxonomy structure is locked. Set OD_TAXONOMY_MUTATION_ENABLED=true only if you must perform break-glass maintenance.",
            ),
        )
    };

    Err((status, Json(error)))
}

#[cfg(test)]
mod mutation_tests {
    use super::support::build_test_state;
    use super::*;
    use crate::auth::UserContext;
    use anyhow::Result;
    use axum::{extract::State, Extension};
    use std::env;

    #[tokio::test]
    async fn taxonomy_mutation_locked_by_default() -> Result<()> {
        let db_url = match env::var("ADMIN_TEST_DATABASE_URL") {
            Ok(url) => url,
            Err(_) => {
                eprintln!(
                    "skipping taxonomy_mutation_locked_by_default (set ADMIN_TEST_DATABASE_URL to run)"
                );
                return Ok(());
            }
        };

        let state = build_test_state(&db_url, false).await?;
        let result = handle_mutation_request(
            Method::POST,
            "/api/v1/taxonomy/categories".parse().unwrap(),
            UserContext::system(),
            state,
            "category",
        )
        .await;
        let (status, Json(err)) = result.expect_err("expected lock error");
        assert_eq!(status, StatusCode::LOCKED);
        assert_eq!(err.code(), "TAXONOMY_LOCKED");
        Ok(())
    }

    #[tokio::test]
    async fn taxonomy_mutation_maintenance_flag_acknowledged() -> Result<()> {
        let db_url = match env::var("ADMIN_TEST_DATABASE_URL") {
            Ok(url) => url,
            Err(_) => {
                eprintln!(
                    "skipping taxonomy_mutation_maintenance_flag_acknowledged (set ADMIN_TEST_DATABASE_URL to run)"
                );
                return Ok(());
            }
        };

        let state = build_test_state(&db_url, true).await?;
        let result = handle_mutation_request(
            Method::DELETE,
            "/api/v1/taxonomy/categories/legacy".parse().unwrap(),
            UserContext::system(),
            state,
            "category",
        )
        .await;
        let (status, Json(err)) = result.expect_err("expected maintenance response");
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
        assert_eq!(err.code(), "TAXONOMY_MUTATION_UNSUPPORTED");
        Ok(())
    }

    #[tokio::test]
    async fn category_mutation_route_respects_lock() -> Result<()> {
        let db_url = match env::var("ADMIN_TEST_DATABASE_URL") {
            Ok(url) => url,
            Err(_) => {
                eprintln!(
                    "skipping category_mutation_route_respects_lock (set ADMIN_TEST_DATABASE_URL to run)"
                );
                return Ok(());
            }
        };

        let state = build_test_state(&db_url, false).await?;
        let uri: Uri = "/api/v1/taxonomy/categories".parse().unwrap();
        let result = block_category_mutation(
            Method::POST,
            OriginalUri(uri),
            Extension(UserContext::system()),
            State(state.clone()),
        )
        .await;
        let (status, Json(err)) = result.expect_err("expected lock");
        assert_eq!(status, StatusCode::LOCKED);
        assert_eq!(err.code(), "TAXONOMY_LOCKED");
        Ok(())
    }

    #[tokio::test]
    async fn category_mutation_route_acknowledges_maintenance() -> Result<()> {
        let db_url = match env::var("ADMIN_TEST_DATABASE_URL") {
            Ok(url) => url,
            Err(_) => {
                eprintln!(
                    "skipping category_mutation_route_acknowledges_maintenance (set ADMIN_TEST_DATABASE_URL to run)"
                );
                return Ok(());
            }
        };

        let state = build_test_state(&db_url, true).await?;
        let uri: Uri = "/api/v1/taxonomy/categories/legacy".parse().unwrap();
        let result = block_category_mutation(
            Method::DELETE,
            OriginalUri(uri),
            Extension(UserContext::system()),
            State(state.clone()),
        )
        .await;
        let (status, Json(err)) = result.expect_err("expected maintenance");
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
        assert_eq!(err.code(), "TAXONOMY_MUTATION_UNSUPPORTED");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::support::build_test_state;
    use super::*;
    use anyhow::Result;
    use axum::{extract::State, Extension};
    use sqlx::postgres::PgPoolOptions;
    use std::env;

    fn build_taxonomy() -> CanonicalTaxonomy {
        CanonicalTaxonomy {
            version: "test-version".into(),
            source: None,
            categories: vec![CanonicalCategory {
                id: "cat-a".into(),
                name: "Category A".into(),
                always_enabled: None,
                subcategories: vec![
                    CanonicalSubcategory {
                        id: "sub-a".into(),
                        name: "Sub A".into(),
                        always_enabled: None,
                    },
                    CanonicalSubcategory {
                        id: "sub-b".into(),
                        name: "Sub B".into(),
                        always_enabled: Some(true),
                    },
                ],
            }],
        }
    }

    #[test]
    fn rejects_version_mismatch() {
        let taxonomy = build_taxonomy();
        let payload = ActivationUpdateRequest {
            version: "another".into(),
            categories: vec![],
        };
        let err = validate_activation_payload(payload, &taxonomy).unwrap_err();
        assert_eq!(err.0, StatusCode::CONFLICT);
    }

    #[test]
    fn rejects_missing_category() {
        let taxonomy = build_taxonomy();
        let payload = ActivationUpdateRequest {
            version: taxonomy.version.clone(),
            categories: vec![],
        };
        let err = validate_activation_payload(payload, &taxonomy).unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn rejects_locked_subcategory_changes() {
        let taxonomy = build_taxonomy();
        let payload = ActivationUpdateRequest {
            version: taxonomy.version.clone(),
            categories: vec![ActivationCategoryInput {
                id: "cat-a".into(),
                enabled: true,
                subcategories: vec![
                    ActivationSubcategoryInput {
                        id: "sub-a".into(),
                        enabled: true,
                    },
                    ActivationSubcategoryInput {
                        id: "sub-b".into(),
                        enabled: false,
                    },
                ],
            }],
        };
        let err = validate_activation_payload(payload, &taxonomy).unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn activation_profile_save_and_reload() -> Result<()> {
        let db_url = match env::var("ADMIN_TEST_DATABASE_URL") {
            Ok(url) => url,
            Err(_) => {
                eprintln!(
                    "skipping activation_profile_save_and_reload (set ADMIN_TEST_DATABASE_URL to run)"
                );
                return Ok(());
            }
        };

        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&db_url)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        sqlx::query("TRUNCATE taxonomy_activation_entries CASCADE")
            .execute(&pool)
            .await?;
        sqlx::query("TRUNCATE taxonomy_activation_profiles CASCADE")
            .execute(&pool)
            .await?;

        let taxonomy = CanonicalTaxonomy::load_from_env().expect("canonical taxonomy");
        let profile = ensure_activation_profile(&pool, &taxonomy).await.unwrap();
        assert!(profile.category_states.contains_key("unknown-unclassified"));

        let request = ActivationUpdateRequest {
            version: taxonomy.version.clone(),
            categories: taxonomy
                .categories
                .iter()
                .map(|cat| ActivationCategoryInput {
                    id: cat.id.clone(),
                    enabled: cat.id != "unknown-unclassified",
                    subcategories: cat
                        .subcategories
                        .iter()
                        .map(|sub| ActivationSubcategoryInput {
                            id: sub.id.clone(),
                            enabled: cat.id != "unknown-unclassified",
                        })
                        .collect(),
                })
                .collect(),
        };
        let validated = validate_activation_payload(request, &taxonomy).unwrap();
        persist_activation_profile(&pool, profile.id, &validated, &taxonomy, "tester")
            .await
            .unwrap();

        let reloaded = ensure_activation_profile(&pool, &taxonomy).await.unwrap();
        assert_eq!(
            reloaded
                .category_states
                .get("unknown-unclassified")
                .copied(),
            Some(false)
        );

        Ok(())
    }

    #[tokio::test]
    async fn taxonomy_activation_round_trip() -> Result<()> {
        let db_url = match env::var("ADMIN_TEST_DATABASE_URL") {
            Ok(url) => url,
            Err(_) => {
                eprintln!(
                    "skipping taxonomy_activation_round_trip (set ADMIN_TEST_DATABASE_URL to run)"
                );
                return Ok(());
            }
        };

        let state = build_test_state(&db_url, false).await?;
        sqlx::query("TRUNCATE taxonomy_activation_entries CASCADE")
            .execute(state.pool())
            .await?;
        sqlx::query("TRUNCATE taxonomy_activation_profiles CASCADE")
            .execute(state.pool())
            .await?;

        let Json(initial) = get_taxonomy(Extension(UserContext::system()), State(state.clone()))
            .await
            .expect("taxonomy fetch");
        assert!(!initial.categories.is_empty(), "expected taxonomy data");

        let mut payload = ActivationUpdateRequest {
            version: initial.version.clone(),
            categories: initial
                .categories
                .iter()
                .map(|category| ActivationCategoryInput {
                    id: category.id.clone(),
                    enabled: category.enabled,
                    subcategories: category
                        .subcategories
                        .iter()
                        .map(|sub| ActivationSubcategoryInput {
                            id: sub.id.clone(),
                            enabled: sub.enabled,
                        })
                        .collect(),
                })
                .collect(),
        };

        let unknown = payload
            .categories
            .iter_mut()
            .find(|cat| cat.id == "unknown-unclassified")
            .expect("unknown category present");
        unknown.enabled = false;
        for sub in &mut unknown.subcategories {
            sub.enabled = false;
        }

        let editor =
            UserContext::from_fallback("taxonomy-editor".into(), vec!["policy-admin".into()]);
        let _ = update_taxonomy_activation(Extension(editor), State(state.clone()), Json(payload))
            .await
            .expect("activation update");

        let Json(updated) = get_taxonomy(Extension(UserContext::system()), State(state.clone()))
            .await
            .expect("taxonomy refetch");
        let unknown_response = updated
            .categories
            .iter()
            .find(|cat| cat.id == "unknown-unclassified")
            .expect("unknown response");
        assert!(
            !unknown_response.enabled,
            "unknown category should reflect updated state"
        );
        assert!(unknown_response
            .subcategories
            .iter()
            .all(|sub| !sub.enabled));

        Ok(())
    }
}
