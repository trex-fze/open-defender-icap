use std::collections::HashMap;

use axum::{extract::State, http::StatusCode, Extension, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
    record_taxonomy_activation_change();

    Ok(Json(ActivationSaveResponse {
        version: updated.version.clone(),
        updated_at: updated.updated_at,
        updated_by: updated.updated_by.clone(),
    }))
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
mod tests {
    use super::*;

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
}
