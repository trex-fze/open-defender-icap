use crate::{
    auth::{require_roles, UserContext, ROLE_POLICY_EDIT, ROLE_POLICY_VIEW},
    pagination::{cursor_limit, decode_cursor, encode_cursor, CursorPaged},
    ApiError, AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{DateTime, Utc};
use common_types::{normalizer::canonical_classification_key, PolicyAction, PolicyDecision};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use tracing::{error, warn};

#[derive(Debug, Deserialize)]
pub struct ClassificationListQuery {
    pub state: Option<String>,
    pub q: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ClassificationCursor {
    updated_at: DateTime<Utc>,
    normalized_key: String,
}

#[derive(Debug, Serialize)]
pub struct ClassificationListRecord {
    pub normalized_key: String,
    pub state: String,
    pub primary_category: Option<String>,
    pub subcategory: Option<String>,
    pub risk_level: Option<String>,
    pub recommended_action: Option<PolicyAction>,
    pub effective_action: Option<PolicyAction>,
    pub effective_decision_source: Option<String>,
    pub confidence: Option<f32>,
    pub status: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateClassificationRequest {
    pub primary_category: String,
    pub subcategory: String,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ClassificationMutationRecord {
    pub normalized_key: String,
    pub primary_category: String,
    pub subcategory: String,
    pub risk_level: String,
    pub recommended_action: PolicyAction,
    pub confidence: f32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct PolicyDecisionRequestPayload {
    normalized_key: String,
    entity_level: String,
    source_ip: String,
    user_id: Option<String>,
    group_ids: Option<Vec<String>>,
    category_hint: Option<String>,
    subcategory_hint: Option<String>,
    risk_hint: Option<String>,
    confidence_hint: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClassificationExchangeBundle {
    pub schema_version: String,
    pub exported_at: DateTime<Utc>,
    pub taxonomy_version: String,
    pub entries: Vec<ClassificationExchangeEntry>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClassificationExchangeEntry {
    pub normalized_key: String,
    pub primary_category: String,
    pub subcategory: String,
    pub risk_level: Option<String>,
    pub recommended_action: Option<String>,
    pub confidence: Option<f32>,
    pub status: Option<String>,
    pub flags: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct ClassificationExportQuery {
    pub q: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ClassificationImportRequest {
    pub bundle: ClassificationExchangeBundle,
    #[serde(default)]
    pub mode: ClassificationImportMode,
    #[serde(default = "default_recompute_policy_fields")]
    pub recompute_policy_fields: bool,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Deserialize, Clone, Copy, Default)]
#[serde(rename_all = "lowercase")]
pub enum ClassificationImportMode {
    #[default]
    Merge,
    Replace,
}

#[derive(Debug, Serialize)]
pub struct ClassificationImportResponse {
    pub mode: String,
    pub recompute_policy_fields: bool,
    pub dry_run: bool,
    pub total_entries: usize,
    pub imported: usize,
    pub updated: usize,
    pub skipped: usize,
    pub replaced_deleted: usize,
    pub invalid: usize,
    pub invalid_rows_filename: Option<String>,
    pub invalid_rows_jsonl: Option<String>,
    pub invalid_rows_truncated: bool,
}

#[derive(Debug, Deserialize)]
pub struct ClassificationFlushRequest {
    pub scope: ClassificationFlushScope,
    pub keys: Option<Vec<String>>,
    pub prefix: Option<String>,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClassificationFlushScope {
    All,
    Keys,
    Prefix,
}

#[derive(Debug, Serialize)]
pub struct ClassificationFlushResponse {
    pub scope: String,
    pub dry_run: bool,
    pub matched: usize,
    pub deleted: usize,
    pub invalid_keys: Vec<String>,
}

#[derive(Debug, Serialize)]
struct InvalidImportRow {
    index: usize,
    normalized_key: Option<String>,
    error_code: String,
    error_message: String,
    raw_entry: Value,
}

#[derive(Debug, Clone)]
struct PreparedImportEntry {
    normalized_key: String,
    primary_category: String,
    subcategory: String,
    risk_level: String,
    recommended_action: PolicyAction,
    confidence: f32,
    flags: Value,
}

#[derive(Debug, Clone)]
struct ExistingClassificationRow {
    normalized_key: String,
    primary_category: String,
    subcategory: String,
    risk_level: String,
    recommended_action: String,
    confidence: f64,
}

pub async fn list(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Query(query): Query<ClassificationListQuery>,
) -> Result<Json<CursorPaged<ClassificationListRecord>>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_POLICY_VIEW)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;

    let limit = cursor_limit(query.limit);
    let state_filter = query
        .state
        .as_deref()
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(|| "all".to_string());
    let q = query.q.unwrap_or_default();
    let q_like = format!("%{}%", q.trim().to_ascii_lowercase());

    let cursor = query
        .cursor
        .as_deref()
        .map(decode_cursor::<ClassificationCursor>)
        .transpose()
        .map_err(|message| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError::new("INVALID_CURSOR", message)),
            )
        })?;

    let cursor_updated_at = cursor.as_ref().map(|c| c.updated_at);
    let cursor_key = cursor
        .as_ref()
        .map(|c| c.normalized_key.clone())
        .unwrap_or_default();

    let rows = sqlx::query(
        r#"WITH combined AS (
               SELECT normalized_key,
                      'classified'::text AS state,
                      primary_category,
                      subcategory,
                      risk_level,
                      recommended_action,
                      confidence::float8 AS confidence,
                      status,
                      updated_at
               FROM classifications
               WHERE status = 'active'
                 AND LOWER(normalized_key) LIKE $1
                 AND ($2 = 'all' OR $2 = 'classified')
               UNION ALL
               SELECT cr.normalized_key,
                      'unclassified'::text AS state,
                      NULL::text AS primary_category,
                      NULL::text AS subcategory,
                      NULL::text AS risk_level,
                      NULL::text AS recommended_action,
                      NULL::float8 AS confidence,
                      cr.status,
                      cr.updated_at
               FROM classification_requests cr
               LEFT JOIN classifications c
                 ON c.normalized_key = cr.normalized_key
                AND c.status = 'active'
               WHERE c.normalized_key IS NULL
                 AND LOWER(cr.normalized_key) LIKE $1
                 AND ($2 = 'all' OR $2 = 'unclassified')
           )
           SELECT normalized_key,
                  state,
                  primary_category,
                  subcategory,
                  risk_level,
                  recommended_action,
                  confidence,
                  status,
                  updated_at
           FROM combined
           WHERE ($3::timestamptz IS NULL OR (updated_at, normalized_key) < ($3, $4))
           ORDER BY updated_at DESC, normalized_key DESC
           LIMIT $5"#,
    )
    .bind(&q_like)
    .bind(&state_filter)
    .bind(cursor_updated_at)
    .bind(&cursor_key)
    .bind((limit + 1) as i64)
    .fetch_all(state.pool())
    .await
    .map_err(db_error)?;

    let mut out: Vec<ClassificationListRecord> = rows
        .into_iter()
        .map(|row| ClassificationListRecord {
            normalized_key: row.get("normalized_key"),
            state: row.get("state"),
            primary_category: row.get("primary_category"),
            subcategory: row.get("subcategory"),
            risk_level: row.get("risk_level"),
            recommended_action: row
                .try_get::<Option<String>, _>("recommended_action")
                .ok()
                .flatten()
                .as_deref()
                .and_then(parse_policy_action),
            effective_action: None,
            effective_decision_source: None,
            confidence: row
                .try_get::<Option<f64>, _>("confidence")
                .ok()
                .flatten()
                .map(|v| v as f32),
            status: row.get("status"),
            updated_at: row.get("updated_at"),
        })
        .collect();

    let has_more = out.len() > limit as usize;
    if has_more {
        out.truncate(limit as usize);
    }
    enrich_effective_decisions(&state, &mut out).await;
    let next_cursor = if has_more {
        out.last().and_then(|last| {
            encode_cursor(&ClassificationCursor {
                updated_at: last.updated_at,
                normalized_key: last.normalized_key.clone(),
            })
            .ok()
        })
    } else {
        None
    };
    Ok(Json(CursorPaged::new(out, limit, has_more, next_cursor)))
}

pub async fn export_bundle(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Query(query): Query<ClassificationExportQuery>,
) -> Result<Json<ClassificationExchangeBundle>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_POLICY_VIEW)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;

    let q = query.q.unwrap_or_default();
    let q_like = format!("%{}%", q.trim().to_ascii_lowercase());

    let rows = sqlx::query(
        r#"SELECT normalized_key,
                  primary_category,
                  subcategory,
                  risk_level,
                  recommended_action,
                  confidence::float8 AS confidence,
                  status,
                  flags
           FROM classifications
           WHERE normalized_key LIKE 'domain:%'
             AND LOWER(normalized_key) LIKE $1
           ORDER BY normalized_key ASC"#,
    )
    .bind(&q_like)
    .fetch_all(state.pool())
    .await
    .map_err(db_error)?;

    let entries = rows
        .into_iter()
        .map(|row| ClassificationExchangeEntry {
            normalized_key: row.get("normalized_key"),
            primary_category: row.get("primary_category"),
            subcategory: row.get("subcategory"),
            risk_level: row.get("risk_level"),
            recommended_action: row.get("recommended_action"),
            confidence: row
                .try_get::<Option<f64>, _>("confidence")
                .ok()
                .flatten()
                .map(|v| v as f32),
            status: row.get("status"),
            flags: row.try_get("flags").ok(),
        })
        .collect();

    let bundle = ClassificationExchangeBundle {
        schema_version: "od-classification-bundle.v1".to_string(),
        exported_at: Utc::now(),
        taxonomy_version: state.taxonomy_store().taxonomy().version.clone(),
        entries,
    };

    Ok(Json(bundle))
}

pub async fn import_bundle(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Json(payload): Json<ClassificationImportRequest>,
) -> Result<Json<ClassificationImportResponse>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_POLICY_EDIT)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;

    let mut invalid_rows: Vec<InvalidImportRow> = Vec::new();
    let mut prepared_by_key: HashMap<String, PreparedImportEntry> = HashMap::new();
    let taxonomy_store = state.taxonomy_store();

    for (index, entry) in payload.bundle.entries.iter().enumerate() {
        let raw_entry = serde_json::to_value(entry).unwrap_or(Value::Null);
        let canonical = match canonical_classification_key(&entry.normalized_key) {
            Some(value) => value,
            None => {
                invalid_rows.push(InvalidImportRow {
                    index,
                    normalized_key: Some(entry.normalized_key.clone()),
                    error_code: "invalid_normalized_key".to_string(),
                    error_message: "normalized_key must be canonicalizable to domain:*".to_string(),
                    raw_entry,
                });
                continue;
            }
        };

        let sub_input = if entry.subcategory.trim().is_empty() {
            None
        } else {
            Some(entry.subcategory.as_str())
        };
        let validated = taxonomy_store.validate_labels(&entry.primary_category, sub_input);
        if let Some(reason) = validated.fallback_reason {
            invalid_rows.push(InvalidImportRow {
                index,
                normalized_key: Some(canonical),
                error_code: "invalid_taxonomy".to_string(),
                error_message: format!(
                    "category/subcategory rejected by canonical taxonomy: {}",
                    reason.as_str()
                ),
                raw_entry,
            });
            continue;
        }

        let mut flags = entry.flags.clone().unwrap_or_else(|| json!({}));
        if !flags.is_object() {
            flags = json!({});
        }
        if let Some(obj) = flags.as_object_mut() {
            obj.insert(
                "source".into(),
                Value::String(if payload.recompute_policy_fields {
                    "import-recompute".to_string()
                } else {
                    "import-as-is".to_string()
                }),
            );
            obj.insert("actor".into(), Value::String(user.actor.clone()));
        }

        let (risk_level, recommended_action, confidence) = if payload.recompute_policy_fields {
            match evaluate_decision_for_import(
                &state,
                &canonical,
                &validated.category.id,
                &validated.subcategory.id,
            )
            .await
            {
                Ok(values) => values,
                Err(err) => {
                    invalid_rows.push(InvalidImportRow {
                        index,
                        normalized_key: Some(canonical),
                        error_code: "policy_decision_failed".to_string(),
                        error_message: err,
                        raw_entry,
                    });
                    continue;
                }
            }
        } else {
            let action_raw = match entry.recommended_action.as_deref() {
                Some(value) => value,
                None => {
                    invalid_rows.push(InvalidImportRow {
                        index,
                        normalized_key: Some(canonical),
                        error_code: "missing_recommended_action".to_string(),
                        error_message: "recommended_action is required when recompute is disabled"
                            .to_string(),
                        raw_entry,
                    });
                    continue;
                }
            };
            let action = match parse_policy_action(action_raw) {
                Some(value) => value,
                None => {
                    invalid_rows.push(InvalidImportRow {
                        index,
                        normalized_key: Some(canonical),
                        error_code: "invalid_recommended_action".to_string(),
                        error_message: format!("unsupported recommended_action: {}", action_raw),
                        raw_entry,
                    });
                    continue;
                }
            };
            let risk_level = match entry.risk_level.as_deref() {
                Some(value) if !value.trim().is_empty() => {
                    let normalized = value.trim().to_ascii_lowercase();
                    if is_valid_risk_level(&normalized) {
                        normalized
                    } else {
                        invalid_rows.push(InvalidImportRow {
                            index,
                            normalized_key: Some(canonical),
                            error_code: "invalid_risk_level".to_string(),
                            error_message: format!(
                                "risk_level must be one of low|medium|high|critical, got {}",
                                value
                            ),
                            raw_entry,
                        });
                        continue;
                    }
                }
                _ => {
                    invalid_rows.push(InvalidImportRow {
                        index,
                        normalized_key: Some(canonical),
                        error_code: "missing_risk_level".to_string(),
                        error_message: "risk_level is required when recompute is disabled"
                            .to_string(),
                        raw_entry,
                    });
                    continue;
                }
            };
            let confidence = match entry.confidence {
                Some(value) if (0.0..=1.0).contains(&value) => value,
                _ => {
                    invalid_rows.push(InvalidImportRow {
                        index,
                        normalized_key: Some(canonical),
                        error_code: "invalid_confidence".to_string(),
                        error_message:
                            "confidence must be set between 0.0 and 1.0 when recompute is disabled"
                                .to_string(),
                        raw_entry,
                    });
                    continue;
                }
            };
            (risk_level, action, confidence)
        };

        prepared_by_key.insert(
            canonical.clone(),
            PreparedImportEntry {
                normalized_key: canonical,
                primary_category: validated.category.id.clone(),
                subcategory: validated.subcategory.id.clone(),
                risk_level,
                recommended_action,
                confidence,
                flags,
            },
        );
    }

    let prepared: Vec<PreparedImportEntry> = prepared_by_key.into_values().collect();
    let prepared_keys: HashSet<String> = prepared
        .iter()
        .map(|entry| entry.normalized_key.clone())
        .collect();

    let existing_rows = list_existing_domain_rows(state.pool())
        .await
        .map_err(db_error)?;
    let existing_map: HashMap<String, ExistingClassificationRow> = existing_rows
        .into_iter()
        .map(|row| (row.normalized_key.clone(), row))
        .collect();

    let to_delete_replace: Vec<String> =
        if matches!(payload.mode, ClassificationImportMode::Replace) {
            list_existing_domain_keys_union(state.pool())
                .await
                .map_err(db_error)?
                .into_iter()
                .filter(|key| !prepared_keys.contains(key))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

    let mut imported = 0usize;
    let mut updated = 0usize;
    let mut skipped = 0usize;

    if !payload.dry_run {
        for entry in &prepared {
            if let Some(existing) = existing_map.get(&entry.normalized_key) {
                if is_same_classification(existing, entry) {
                    skipped += 1;
                    continue;
                }
                updated += 1;
            } else {
                imported += 1;
            }
            upsert_import_entry(
                state.pool(),
                entry,
                &payload.bundle.taxonomy_version,
                &user.actor,
            )
            .await
            .map_err(db_error)?;

            let _ = sqlx::query("DELETE FROM classification_requests WHERE normalized_key = $1")
                .bind(&entry.normalized_key)
                .execute(state.pool())
                .await;
        }

        if !to_delete_replace.is_empty() {
            delete_keys_and_related(state.pool(), &to_delete_replace)
                .await
                .map_err(db_error)?;
        }

        if !prepared.is_empty() || !to_delete_replace.is_empty() {
            state.invalidate_policy_cache().await;
        }
    } else {
        for entry in &prepared {
            if let Some(existing) = existing_map.get(&entry.normalized_key) {
                if is_same_classification(existing, entry) {
                    skipped += 1;
                } else {
                    updated += 1;
                }
            } else {
                imported += 1;
            }
        }
    }

    let invalid_rows_filename = if invalid_rows.is_empty() {
        None
    } else {
        Some(format!(
            "classification-import-invalid-{}.jsonl",
            Utc::now().format("%Y%m%d%H%M%S")
        ))
    };
    let (invalid_rows_jsonl, invalid_rows_truncated) = if invalid_rows.is_empty() {
        (None, false)
    } else {
        let (jsonl, truncated) = render_invalid_rows_jsonl_limited(&invalid_rows, 1_000_000);
        (Some(jsonl), truncated)
    };

    state
        .log_policy_event(
            "classifications.import",
            Some(user.actor.clone()),
            None,
            json!({
                "mode": mode_label(payload.mode),
                "recompute_policy_fields": payload.recompute_policy_fields,
                "dry_run": payload.dry_run,
                "imported": imported,
                "updated": updated,
                "replaced_deleted": to_delete_replace.len(),
                "invalid": invalid_rows.len(),
            }),
        )
        .await;

    Ok(Json(ClassificationImportResponse {
        mode: mode_label(payload.mode).to_string(),
        recompute_policy_fields: payload.recompute_policy_fields,
        dry_run: payload.dry_run,
        total_entries: payload.bundle.entries.len(),
        imported,
        updated,
        skipped,
        replaced_deleted: to_delete_replace.len(),
        invalid: invalid_rows.len(),
        invalid_rows_filename,
        invalid_rows_jsonl,
        invalid_rows_truncated,
    }))
}

pub async fn flush(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Json(payload): Json<ClassificationFlushRequest>,
) -> Result<Json<ClassificationFlushResponse>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_POLICY_EDIT)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;

    let mut invalid_keys: Vec<String> = Vec::new();
    let keys = match payload.scope {
        ClassificationFlushScope::All => list_existing_domain_keys(state.pool()).await,
        ClassificationFlushScope::Prefix => {
            let raw_prefix = payload.prefix.clone().unwrap_or_default();
            let trimmed = raw_prefix.trim().to_ascii_lowercase();
            let prefix = if trimmed.starts_with("domain:") {
                trimmed
            } else {
                format!("domain:{}", trimmed)
            };
            if prefix == "domain:" {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ApiError::new("INVALID_PREFIX", "prefix cannot be empty")),
                ));
            }
            list_existing_domain_keys_with_prefix(state.pool(), &prefix).await
        }
        ClassificationFlushScope::Keys => {
            let provided = payload.keys.clone().unwrap_or_default();
            if provided.is_empty() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ApiError::new("INVALID_KEYS", "keys must not be empty")),
                ));
            }
            let mut canonical = Vec::new();
            for value in provided {
                if let Some(key) = canonical_classification_key(&value) {
                    canonical.push(key);
                } else {
                    invalid_keys.push(value);
                }
            }
            if canonical.is_empty() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ApiError::new(
                        "INVALID_KEYS",
                        "no valid domain/subdomain keys provided",
                    )),
                ));
            }
            canonical.sort();
            canonical.dedup();
            Ok(canonical)
        }
    }
    .map_err(db_error)?;

    let matched = keys.len();
    let mut deleted = 0usize;
    if !payload.dry_run && !keys.is_empty() {
        deleted = delete_keys_and_related(state.pool(), &keys)
            .await
            .map_err(db_error)?;
        state.invalidate_policy_cache().await;
    }

    state
        .log_policy_event(
            "classifications.flush",
            Some(user.actor.clone()),
            None,
            json!({
                "scope": flush_scope_label(&payload.scope),
                "dry_run": payload.dry_run,
                "matched": matched,
                "deleted": deleted,
            }),
        )
        .await;

    Ok(Json(ClassificationFlushResponse {
        scope: flush_scope_label(&payload.scope).to_string(),
        dry_run: payload.dry_run,
        matched,
        deleted,
        invalid_keys,
    }))
}

async fn enrich_effective_decisions(state: &AppState, out: &mut [ClassificationListRecord]) {
    for record in out.iter_mut() {
        if record.state != "classified" {
            continue;
        }

        let Some((entity_level, _)) = parse_normalized_key(&record.normalized_key) else {
            continue;
        };

        let payload = PolicyDecisionRequestPayload {
            normalized_key: record.normalized_key.clone(),
            entity_level: entity_level.to_string(),
            source_ip: "127.0.0.1".to_string(),
            user_id: None,
            group_ids: None,
            category_hint: record.primary_category.clone(),
            subcategory_hint: record.subcategory.clone(),
            risk_hint: record.risk_level.clone(),
            confidence_hint: record.confidence,
        };

        match state
            .evaluate_policy_decision::<_, PolicyDecision>(&payload)
            .await
        {
            Ok(decision) => {
                record.effective_action = Some(decision.action);
                record.effective_decision_source = decision.decision_source;
            }
            Err(err) => {
                warn!(
                    target = "svc-admin",
                    %err,
                    normalized_key = %record.normalized_key,
                    "failed to compute effective decision for classifications list"
                );
            }
        }
    }
}

pub async fn update(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(normalized_key): Path<String>,
    Json(payload): Json<UpdateClassificationRequest>,
) -> Result<Json<ClassificationMutationRecord>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_POLICY_EDIT)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;

    let taxonomy_store = state.taxonomy_store();
    let sub_input = if payload.subcategory.trim().is_empty() {
        None
    } else {
        Some(payload.subcategory.as_str())
    };
    let validated = taxonomy_store.validate_labels(&payload.primary_category, sub_input);
    if let Some(reason) = validated.fallback_reason {
        warn!(
            target = "svc-admin",
            reason = reason.as_str(),
            actor = %user.actor,
            original_category = %payload.primary_category,
            original_subcategory = %payload.subcategory,
            canonical_category = %validated.category.id,
            canonical_subcategory = %validated.subcategory.id,
            "classification update normalized to canonical taxonomy"
        );
    }

    let (entity_level, hostname) = parse_normalized_key(&normalized_key).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                "INVALID_NORMALIZED_KEY",
                "normalized_key must start with domain: or subdomain:",
            )),
        )
    })?;

    let decision_payload = PolicyDecisionRequestPayload {
        normalized_key: normalized_key.clone(),
        entity_level: entity_level.to_string(),
        source_ip: "127.0.0.1".to_string(),
        user_id: None,
        group_ids: None,
        category_hint: Some(validated.category.id.clone()),
        subcategory_hint: Some(validated.subcategory.id.clone()),
        risk_hint: None,
        confidence_hint: None,
    };

    let decision = state
        .evaluate_policy_decision::<_, PolicyDecision>(&decision_payload)
        .await
        .map_err(policy_error)?;

    let risk_level = decision
        .verdict
        .as_ref()
        .map(|v| v.risk_level.clone())
        .unwrap_or_else(|| "medium".to_string());
    let confidence = decision
        .verdict
        .as_ref()
        .map(|v| v.confidence)
        .unwrap_or(0.9);

    let taxonomy_version = taxonomy_store.taxonomy().version.clone();
    let flags = json!({
        "source": "classification-edit",
        "actor": user.actor,
        "reason": payload.reason,
        "hostname": hostname,
    });

    let row = sqlx::query(
        r#"INSERT INTO classifications (
               id, normalized_key, taxonomy_version, model_version, primary_category,
               subcategory, risk_level, recommended_action, confidence, sfw, flags,
               ttl_seconds, status, next_refresh_at
            ) VALUES ($1, $2, $3, 'manual', $4, $5, $6, $7, $8, false, $9, 3600, 'active', NOW() + INTERVAL '4 hours')
            ON CONFLICT (normalized_key)
            DO UPDATE SET
               primary_category = EXCLUDED.primary_category,
               subcategory = EXCLUDED.subcategory,
               risk_level = EXCLUDED.risk_level,
               recommended_action = EXCLUDED.recommended_action,
               confidence = EXCLUDED.confidence,
               flags = EXCLUDED.flags,
               updated_at = NOW(),
               ttl_seconds = EXCLUDED.ttl_seconds,
               next_refresh_at = NOW() + INTERVAL '4 hours'
            RETURNING id, primary_category, subcategory, risk_level, recommended_action, confidence::float8 AS confidence, updated_at"#,
    )
    .bind(uuid::Uuid::new_v4())
    .bind(&normalized_key)
    .bind(&taxonomy_version)
    .bind(&validated.category.id)
    .bind(&validated.subcategory.id)
    .bind(&risk_level)
    .bind(decision.action.to_string())
    .bind(confidence as f64)
    .bind(flags)
    .fetch_one(state.pool())
    .await
    .map_err(db_error)?;

    let classification_id: uuid::Uuid = row.get("id");
    let next_version: i64 = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT MAX(version) FROM classification_versions WHERE classification_id = $1",
    )
    .bind(classification_id)
    .fetch_one(state.pool())
    .await
    .map_err(db_error)?
    .unwrap_or(0) as i64
        + 1;

    sqlx::query(
        "INSERT INTO classification_versions (id, classification_id, version, changed_by, reason, payload)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(classification_id)
    .bind(next_version)
    .bind(Some(user.actor.clone()))
    .bind(payload.reason.clone())
    .bind(json!({
        "normalized_key": normalized_key,
        "category": validated.category.id,
        "subcategory": validated.subcategory.id,
        "source": "classification-edit",
    }))
    .execute(state.pool())
    .await
    .map_err(db_error)?;

    let _ = sqlx::query("DELETE FROM classification_requests WHERE normalized_key = $1")
        .bind(&normalized_key)
        .execute(state.pool())
        .await;

    state.invalidate_cache_key(&normalized_key).await;

    Ok(Json(ClassificationMutationRecord {
        normalized_key,
        primary_category: row.get("primary_category"),
        subcategory: row.get("subcategory"),
        risk_level: row.get("risk_level"),
        recommended_action: parse_policy_action(
            row.get::<String, _>("recommended_action").as_str(),
        )
        .unwrap_or(PolicyAction::Monitor),
        confidence: row.get::<f64, _>("confidence") as f32,
        updated_at: row.get("updated_at"),
    }))
}

pub async fn delete(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(normalized_key): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_POLICY_EDIT)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;

    sqlx::query("DELETE FROM classifications WHERE normalized_key = $1")
        .bind(&normalized_key)
        .execute(state.pool())
        .await
        .map_err(db_error)?;
    let _ = sqlx::query("DELETE FROM classification_requests WHERE normalized_key = $1")
        .bind(&normalized_key)
        .execute(state.pool())
        .await;
    let _ = sqlx::query("DELETE FROM page_contents WHERE normalized_key = $1")
        .bind(&normalized_key)
        .execute(state.pool())
        .await;

    state.invalidate_cache_key(&normalized_key).await;
    Ok(StatusCode::NO_CONTENT)
}

fn parse_policy_action(value: &str) -> Option<PolicyAction> {
    match value {
        "Allow" => Some(PolicyAction::Allow),
        "Block" => Some(PolicyAction::Block),
        "Warn" => Some(PolicyAction::Warn),
        "Monitor" => Some(PolicyAction::Monitor),
        "Review" => Some(PolicyAction::Review),
        "RequireApproval" => Some(PolicyAction::RequireApproval),
        "ContentPending" => Some(PolicyAction::ContentPending),
        _ => None,
    }
}

fn default_recompute_policy_fields() -> bool {
    true
}

fn mode_label(mode: ClassificationImportMode) -> &'static str {
    match mode {
        ClassificationImportMode::Merge => "merge",
        ClassificationImportMode::Replace => "replace",
    }
}

fn flush_scope_label(scope: &ClassificationFlushScope) -> &'static str {
    match scope {
        ClassificationFlushScope::All => "all",
        ClassificationFlushScope::Keys => "keys",
        ClassificationFlushScope::Prefix => "prefix",
    }
}

async fn evaluate_decision_for_import(
    state: &AppState,
    normalized_key: &str,
    primary_category: &str,
    subcategory: &str,
) -> Result<(String, PolicyAction, f32), String> {
    let (entity_level, _) = parse_normalized_key(normalized_key)
        .ok_or_else(|| "normalized_key must start with domain: or subdomain:".to_string())?;

    let payload = PolicyDecisionRequestPayload {
        normalized_key: normalized_key.to_string(),
        entity_level: entity_level.to_string(),
        source_ip: "127.0.0.1".to_string(),
        user_id: None,
        group_ids: None,
        category_hint: Some(primary_category.to_string()),
        subcategory_hint: Some(subcategory.to_string()),
        risk_hint: None,
        confidence_hint: None,
    };

    let decision = state
        .evaluate_policy_decision::<_, PolicyDecision>(&payload)
        .await
        .map_err(|err| format!("failed to evaluate policy decision: {}", err))?;

    let risk_level = decision
        .verdict
        .as_ref()
        .map(|v| v.risk_level.clone())
        .unwrap_or_else(|| "medium".to_string());
    let confidence = decision
        .verdict
        .as_ref()
        .map(|v| v.confidence)
        .unwrap_or(0.9);

    Ok((risk_level, decision.action, confidence))
}

async fn list_existing_domain_keys(pool: &sqlx::PgPool) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT normalized_key FROM classifications WHERE normalized_key LIKE 'domain:%'",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("normalized_key"))
        .collect())
}

async fn list_existing_domain_rows(
    pool: &sqlx::PgPool,
) -> Result<Vec<ExistingClassificationRow>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT normalized_key,
                  primary_category,
                  subcategory,
                  risk_level,
                  recommended_action,
                  confidence::float8 AS confidence
           FROM classifications
           WHERE normalized_key LIKE 'domain:%'"#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| ExistingClassificationRow {
            normalized_key: row.get("normalized_key"),
            primary_category: row.get("primary_category"),
            subcategory: row.get("subcategory"),
            risk_level: row.get("risk_level"),
            recommended_action: row.get("recommended_action"),
            confidence: row.get("confidence"),
        })
        .collect())
}

async fn list_existing_domain_keys_union(pool: &sqlx::PgPool) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT normalized_key FROM classifications WHERE normalized_key LIKE 'domain:%'
           UNION
           SELECT normalized_key FROM classification_requests WHERE normalized_key LIKE 'domain:%'
           UNION
           SELECT normalized_key FROM page_contents WHERE normalized_key LIKE 'domain:%'"#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("normalized_key"))
        .collect())
}

async fn list_existing_domain_keys_with_prefix(
    pool: &sqlx::PgPool,
    prefix: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let like = format!("{}%", prefix);
    let rows = sqlx::query(
        "SELECT normalized_key FROM classifications WHERE normalized_key LIKE $1 ORDER BY normalized_key",
    )
    .bind(&like)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("normalized_key"))
        .collect())
}

async fn upsert_import_entry(
    pool: &sqlx::PgPool,
    entry: &PreparedImportEntry,
    taxonomy_version: &str,
    actor: &str,
) -> Result<(), sqlx::Error> {
    let row = sqlx::query(
        r#"INSERT INTO classifications (
               id, normalized_key, taxonomy_version, model_version, primary_category,
               subcategory, risk_level, recommended_action, confidence, sfw, flags,
               ttl_seconds, status, next_refresh_at
            ) VALUES ($1, $2, $3, 'import', $4, $5, $6, $7, $8, false, $9, 3600, 'active', NOW() + INTERVAL '4 hours')
            ON CONFLICT (normalized_key)
            DO UPDATE SET
               taxonomy_version = EXCLUDED.taxonomy_version,
               model_version = EXCLUDED.model_version,
               primary_category = EXCLUDED.primary_category,
               subcategory = EXCLUDED.subcategory,
               risk_level = EXCLUDED.risk_level,
               recommended_action = EXCLUDED.recommended_action,
               confidence = EXCLUDED.confidence,
               flags = EXCLUDED.flags,
               updated_at = NOW(),
               ttl_seconds = EXCLUDED.ttl_seconds,
               status = EXCLUDED.status,
               next_refresh_at = NOW() + INTERVAL '4 hours'
            RETURNING id"#,
    )
    .bind(uuid::Uuid::new_v4())
    .bind(&entry.normalized_key)
    .bind(taxonomy_version)
    .bind(&entry.primary_category)
    .bind(&entry.subcategory)
    .bind(&entry.risk_level)
    .bind(entry.recommended_action.to_string())
    .bind(entry.confidence as f64)
    .bind(&entry.flags)
    .fetch_one(pool)
    .await?;

    let classification_id: uuid::Uuid = row.get("id");
    let next_version: i64 = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT MAX(version) FROM classification_versions WHERE classification_id = $1",
    )
    .bind(classification_id)
    .fetch_one(pool)
    .await?
    .unwrap_or(0) as i64
        + 1;

    sqlx::query(
        "INSERT INTO classification_versions (id, classification_id, version, changed_by, reason, payload)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(classification_id)
    .bind(next_version)
    .bind(Some(actor.to_string()))
    .bind(Some("classification import".to_string()))
    .bind(json!({
        "normalized_key": entry.normalized_key,
        "category": entry.primary_category,
        "subcategory": entry.subcategory,
        "source": "classification-import",
    }))
    .execute(pool)
    .await?;

    Ok(())
}

async fn delete_keys_and_related(
    pool: &sqlx::PgPool,
    keys: &[String],
) -> Result<usize, sqlx::Error> {
    if keys.is_empty() {
        return Ok(0);
    }

    let deleted = sqlx::query("DELETE FROM classifications WHERE normalized_key = ANY($1)")
        .bind(keys)
        .execute(pool)
        .await?
        .rows_affected() as usize;
    let _ = sqlx::query("DELETE FROM classification_requests WHERE normalized_key = ANY($1)")
        .bind(keys)
        .execute(pool)
        .await?;
    let _ = sqlx::query("DELETE FROM page_contents WHERE normalized_key = ANY($1)")
        .bind(keys)
        .execute(pool)
        .await?;

    Ok(deleted)
}

fn render_invalid_rows_jsonl_limited(
    rows: &[InvalidImportRow],
    max_bytes: usize,
) -> (String, bool) {
    let mut out = String::new();
    let mut truncated = false;
    for row in rows {
        let Ok(serialized) = serde_json::to_string(row) else {
            continue;
        };
        let needed = serialized.len() + 1;
        if out.len() + needed > max_bytes {
            truncated = true;
            break;
        }
        out.push_str(&serialized);
        out.push('\n');
    }
    (out, truncated)
}

fn is_same_classification(
    existing: &ExistingClassificationRow,
    incoming: &PreparedImportEntry,
) -> bool {
    existing.primary_category == incoming.primary_category
        && existing.subcategory == incoming.subcategory
        && existing.risk_level == incoming.risk_level
        && existing.recommended_action == incoming.recommended_action.to_string()
        && (existing.confidence - incoming.confidence as f64).abs() < 0.0001
}

fn is_valid_risk_level(value: &str) -> bool {
    matches!(value, "low" | "medium" | "high" | "critical")
}

fn parse_normalized_key(normalized_key: &str) -> Option<(&'static str, String)> {
    if let Some(host) = normalized_key.strip_prefix("domain:") {
        let host = host.trim();
        if host.is_empty() {
            return None;
        }
        return Some(("domain", host.to_string()));
    }
    if let Some(host) = normalized_key.strip_prefix("subdomain:") {
        let host = host.trim();
        if host.is_empty() {
            return None;
        }
        return Some(("subdomain", host.to_string()));
    }
    None
}

fn policy_error(err: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    error!(target = "svc-admin", %err, "policy decision call failed");
    (
        StatusCode::BAD_GATEWAY,
        Json(ApiError::new(
            "POLICY_DECISION_FAILED",
            "failed to compute policy decision for classification update",
        )),
    )
}

fn db_error(err: sqlx::Error) -> (StatusCode, Json<ApiError>) {
    error!(target = "svc-admin", %err, "classification query failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError::new("DB_ERROR", err.to_string())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_policy_actions() {
        assert_eq!(parse_policy_action("Allow"), Some(PolicyAction::Allow));
        assert_eq!(parse_policy_action("Block"), Some(PolicyAction::Block));
        assert_eq!(parse_policy_action("Warn"), Some(PolicyAction::Warn));
        assert_eq!(
            parse_policy_action("ContentPending"),
            Some(PolicyAction::ContentPending)
        );
        assert_eq!(parse_policy_action("nope"), None);
    }

    #[test]
    fn parses_domain_and_subdomain_keys() {
        assert_eq!(
            parse_normalized_key("domain:example.com"),
            Some(("domain", "example.com".to_string()))
        );
        assert_eq!(
            parse_normalized_key("subdomain:www.example.com"),
            Some(("subdomain", "www.example.com".to_string()))
        );
        assert_eq!(parse_normalized_key("url:https://example.com"), None);
    }
}
