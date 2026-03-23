use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use tracing::error;
use uuid::Uuid;

use crate::{
    auth::{require_roles, UserContext, ROLE_POLICY_VIEW, ROLE_TAXONOMY_EDIT},
    ApiError, AppState,
};

#[derive(Debug, Serialize)]
pub struct TaxonomyCategory {
    pub id: Uuid,
    pub name: String,
    pub default_action: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct TaxonomySubcategory {
    pub id: Uuid,
    pub category_id: Uuid,
    pub name: String,
    pub default_action: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CategoryPayload {
    pub name: String,
    pub default_action: String,
}

#[derive(Debug, Deserialize)]
pub struct SubcategoryPayload {
    pub category_id: Uuid,
    pub name: String,
    pub default_action: String,
}

#[derive(Debug, Deserialize)]
pub struct SubcategoryQuery {
    pub category_id: Option<Uuid>,
}

pub async fn list_categories(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
) -> Result<Json<Vec<TaxonomyCategory>>, StatusCode> {
    require_roles(&user, ROLE_POLICY_VIEW)?;
    let rows = sqlx::query(
        "SELECT id, name, default_action, created_at FROM taxonomy_categories ORDER BY name",
    )
    .fetch_all(state.pool())
    .await
    .map_err(map_db_error)?;

    let data = rows
        .into_iter()
        .map(|row| TaxonomyCategory {
            id: row.get("id"),
            name: row.get("name"),
            default_action: row.get("default_action"),
            created_at: row.get("created_at"),
        })
        .collect();
    Ok(Json(data))
}

pub async fn create_category(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Json(payload): Json<CategoryPayload>,
) -> Result<Json<TaxonomyCategory>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_TAXONOMY_EDIT)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;
    let name = payload.name.trim();
    if name.is_empty() {
        return Err(crate::validation_error("name is required"));
    }
    let action = normalize_action(&payload.default_action)?;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO taxonomy_categories (id, name, default_action) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(name)
        .bind(&action)
        .execute(state.pool())
        .await
        .map_err(map_db_error_api)?;
    let record = fetch_category(state.pool(), id).await?;
    Ok(Json(record))
}

pub async fn update_category(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<CategoryPayload>,
) -> Result<Json<TaxonomyCategory>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_TAXONOMY_EDIT)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;
    let name = payload.name.trim();
    if name.is_empty() {
        return Err(crate::validation_error("name is required"));
    }
    let action = normalize_action(&payload.default_action)?;
    let result =
        sqlx::query("UPDATE taxonomy_categories SET name = $1, default_action = $2 WHERE id = $3")
            .bind(name)
            .bind(&action)
            .bind(id)
            .execute(state.pool())
            .await
            .map_err(map_db_error_api)?;
    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::new("NOT_FOUND", "category not found")),
        ));
    }
    let record = fetch_category(state.pool(), id).await?;
    Ok(Json(record))
}

pub async fn delete_category(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_TAXONOMY_EDIT)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;
    let result = sqlx::query("DELETE FROM taxonomy_categories WHERE id = $1")
        .bind(id)
        .execute(state.pool())
        .await
        .map_err(map_db_error_api)?;
    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::new("NOT_FOUND", "category not found")),
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_subcategories(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Query(params): Query<SubcategoryQuery>,
) -> Result<Json<Vec<TaxonomySubcategory>>, StatusCode> {
    require_roles(&user, ROLE_POLICY_VIEW)?;
    let rows = if let Some(category_id) = params.category_id {
        sqlx::query(
            "SELECT id, category_id, name, default_action, created_at FROM taxonomy_subcategories WHERE category_id = $1 ORDER BY name",
        )
        .bind(category_id)
        .fetch_all(state.pool())
        .await
        .map_err(map_db_error)?
    } else {
        sqlx::query(
            "SELECT id, category_id, name, default_action, created_at FROM taxonomy_subcategories ORDER BY name",
        )
        .fetch_all(state.pool())
        .await
        .map_err(map_db_error)?
    };

    let data = rows
        .into_iter()
        .map(|row| TaxonomySubcategory {
            id: row.get("id"),
            category_id: row.get("category_id"),
            name: row.get("name"),
            default_action: row.get("default_action"),
            created_at: row.get("created_at"),
        })
        .collect();
    Ok(Json(data))
}

pub async fn create_subcategory(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Json(payload): Json<SubcategoryPayload>,
) -> Result<Json<TaxonomySubcategory>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_TAXONOMY_EDIT)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;
    let name = payload.name.trim();
    if name.is_empty() {
        return Err(crate::validation_error("name is required"));
    }
    let action = normalize_action(&payload.default_action)?;
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO taxonomy_subcategories (id, category_id, name, default_action) VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(payload.category_id)
    .bind(name)
    .bind(&action)
    .execute(state.pool())
    .await
    .map_err(map_db_error_api)?;
    let record = fetch_subcategory(state.pool(), id).await?;
    Ok(Json(record))
}

pub async fn update_subcategory(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<SubcategoryPayload>,
) -> Result<Json<TaxonomySubcategory>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_TAXONOMY_EDIT)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;
    let name = payload.name.trim();
    if name.is_empty() {
        return Err(crate::validation_error("name is required"));
    }
    let action = normalize_action(&payload.default_action)?;
    let result = sqlx::query(
        "UPDATE taxonomy_subcategories SET category_id = $1, name = $2, default_action = $3 WHERE id = $4",
    )
    .bind(payload.category_id)
    .bind(name)
    .bind(&action)
    .bind(id)
    .execute(state.pool())
    .await
    .map_err(map_db_error_api)?;
    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::new("NOT_FOUND", "subcategory not found")),
        ));
    }
    let record = fetch_subcategory(state.pool(), id).await?;
    Ok(Json(record))
}

pub async fn delete_subcategory(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_TAXONOMY_EDIT)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;
    let result = sqlx::query("DELETE FROM taxonomy_subcategories WHERE id = $1")
        .bind(id)
        .execute(state.pool())
        .await
        .map_err(map_db_error_api)?;
    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::new("NOT_FOUND", "subcategory not found")),
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn fetch_category(
    pool: &PgPool,
    id: Uuid,
) -> Result<TaxonomyCategory, (StatusCode, Json<ApiError>)> {
    let row = sqlx::query(
        "SELECT id, name, default_action, created_at FROM taxonomy_categories WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(map_db_error_api)?
    .ok_or((
        StatusCode::NOT_FOUND,
        Json(ApiError::new("NOT_FOUND", "category not found")),
    ))?;
    Ok(TaxonomyCategory {
        id: row.get("id"),
        name: row.get("name"),
        default_action: row.get("default_action"),
        created_at: row.get("created_at"),
    })
}

async fn fetch_subcategory(
    pool: &PgPool,
    id: Uuid,
) -> Result<TaxonomySubcategory, (StatusCode, Json<ApiError>)> {
    let row = sqlx::query(
        "SELECT id, category_id, name, default_action, created_at FROM taxonomy_subcategories WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(map_db_error_api)?
    .ok_or((
        StatusCode::NOT_FOUND,
        Json(ApiError::new("NOT_FOUND", "subcategory not found")),
    ))?;
    Ok(TaxonomySubcategory {
        id: row.get("id"),
        category_id: row.get("category_id"),
        name: row.get("name"),
        default_action: row.get("default_action"),
        created_at: row.get("created_at"),
    })
}

fn normalize_action(value: &str) -> Result<String, (StatusCode, Json<ApiError>)> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(crate::validation_error("default_action is required"));
    }
    Ok(normalized
        .chars()
        .enumerate()
        .map(|(idx, ch)| {
            if idx == 0 {
                ch.to_ascii_uppercase()
            } else {
                ch.to_ascii_lowercase()
            }
        })
        .collect())
}

fn map_db_error(err: sqlx::Error) -> StatusCode {
    error!(target = "svc-admin", %err, "taxonomy query failed");
    StatusCode::INTERNAL_SERVER_ERROR
}

fn map_db_error_api(err: sqlx::Error) -> (StatusCode, Json<ApiError>) {
    error!(target = "svc-admin", %err, "taxonomy mutation failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError::new("DB_ERROR", err.to_string())),
    )
}
