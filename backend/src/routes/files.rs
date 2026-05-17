//! Workspace-Dateien (PLAN.md Phase 5). Upload → S3, Liste, Löschen,
//! Presigned-Download (15 min), Text-Read/Write mit ETag-Konkurrenz,
//! Office-Vorschau. Pfade gehen immer durch `crate::sandbox`.

use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::primitives::ByteStream;
use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::perm::{require_editor, require_member};
use crate::sandbox::{ensure_in_workspace, sanitize_filename, workspace_key};
use crate::{preview, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/workspaces/{wid}/files", get(list_files).post(upload_file))
        .route("/files/{id}", axum::routing::delete(delete_file))
        .route("/files/{id}/download-url", get(download_url))
        .route("/files/{id}/text", get(read_text).put(write_text))
        .route("/files/{id}/preview/{kind}", get(preview_file))
        // 50-MB-Uploads zulassen (Default wäre 2 MB).
        .layer(DefaultBodyLimit::max(MAX_BYTES + 1024 * 1024))
}

const MAX_BYTES: usize = 50 * 1024 * 1024;
const ALLOWED: [&str; 11] = [
    "md", "txt", "pdf", "docx", "xlsx", "csv", "png", "jpg", "jpeg", "webp", "pptx",
];

#[derive(sqlx::FromRow)]
struct FileRow {
    id: Uuid,
    workspace_id: Uuid,
    filename: String,
    s3_key: String,
    size_bytes: i64,
    content_type: String,
    uploaded_by: Uuid,
    uploaded_at: OffsetDateTime,
}

impl FileRow {
    fn to_api(&self) -> Value {
        json!({
            "id": self.id.to_string(),
            "workspaceId": self.workspace_id.to_string(),
            "filename": self.filename,
            "sizeBytes": self.size_bytes,
            "contentType": self.content_type,
            "uploadedBy": self.uploaded_by.to_string(),
            "uploadedAt": self.uploaded_at.format(&Rfc3339).unwrap_or_default(),
        })
    }
}

const COLS: &str = "id, workspace_id, filename, s3_key, size_bytes, \
    content_type, uploaded_by, uploaded_at";

fn ext_of(name: &str) -> String {
    name.rsplit('.').next().unwrap_or("").to_lowercase()
}

fn s3_err<E: std::fmt::Display>(e: E) -> ApiError {
    ApiError::Internal(anyhow::anyhow!("S3: {e}"))
}

async fn file_row(state: &AppState, id: Uuid) -> ApiResult<FileRow> {
    sqlx::query_as::<_, FileRow>(&format!("SELECT {COLS} FROM workspace_files WHERE id = $1"))
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .ok_or(ApiError::NotFound)
}

/// Lädt Objekt-Bytes + ETag (Version) aus S3.
async fn s3_get(state: &AppState, key: &str) -> ApiResult<(Vec<u8>, String)> {
    let obj = state
        .storage
        .client
        .get_object()
        .bucket(&state.storage.bucket)
        .key(key)
        .send()
        .await
        .map_err(s3_err)?;
    let etag = obj.e_tag().unwrap_or_default().to_string();
    let data = obj.body.collect().await.map_err(s3_err)?.into_bytes();
    Ok((data.to_vec(), etag))
}

// --- Handlers -------------------------------------------------------------

async fn list_files(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<Uuid>,
) -> ApiResult<Json<Vec<Value>>> {
    require_member(&state, &user, wid).await?;
    let rows: Vec<FileRow> = sqlx::query_as(&format!(
        "SELECT {COLS} FROM workspace_files \
         WHERE workspace_id = $1 ORDER BY uploaded_at DESC"
    ))
    .bind(wid)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    Ok(Json(rows.iter().map(FileRow::to_api).collect()))
}

async fn upload_file(
    State(state): State<AppState>,
    user: AuthUser,
    Path(wid): Path<Uuid>,
    mut multipart: Multipart,
) -> ApiResult<(StatusCode, Json<Value>)> {
    require_editor(&state, &user, wid).await?;

    // Feld vollständig innerhalb der Schleife konsumieren — ein `Field`
    // leiht `multipart` mutably und darf nicht über Iterationen gehalten
    // werden.
    let mut found: Option<(String, String, axum::body::Bytes)> = None;
    while let Some(f) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(format!("Multipart-Fehler: {e}")))?
    {
        if f.name() == Some("file") {
            let raw_name = f.file_name().unwrap_or("upload").to_string();
            let ct = f
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string();
            let bytes = f
                .bytes()
                .await
                .map_err(|e| ApiError::BadRequest(format!("Upload-Fehler: {e}")))?;
            found = Some((raw_name, ct, bytes));
            break;
        }
    }
    let (raw_name, content_type, bytes) =
        found.ok_or_else(|| ApiError::BadRequest("Kein 'file'-Feld im Upload.".into()))?;

    let filename = sanitize_filename(&raw_name)?;
    if !ALLOWED.contains(&ext_of(&filename).as_str()) {
        return Err(ApiError::BadRequest("Dateityp nicht erlaubt.".into()));
    }
    if bytes.len() > MAX_BYTES {
        return Err(ApiError::BadRequest("Datei größer als 50 MB.".into()));
    }

    let key = workspace_key(wid, &filename);
    ensure_in_workspace(wid, &key)?;
    state
        .storage
        .client
        .put_object()
        .bucket(&state.storage.bucket)
        .key(&key)
        .content_type(&content_type)
        .body(ByteStream::from(bytes.to_vec()))
        .send()
        .await
        .map_err(s3_err)?;

    // Gleicher Dateiname → ersetzen (PLAN-Lücke #6: überschreiben).
    let row: FileRow = sqlx::query_as(&format!(
        "INSERT INTO workspace_files \
         (workspace_id, filename, s3_key, size_bytes, content_type, uploaded_by) \
         VALUES ($1,$2,$3,$4,$5,$6) \
         ON CONFLICT (workspace_id, filename) DO UPDATE SET \
           s3_key = EXCLUDED.s3_key, size_bytes = EXCLUDED.size_bytes, \
           content_type = EXCLUDED.content_type, \
           uploaded_by = EXCLUDED.uploaded_by, uploaded_at = now() \
         RETURNING {COLS}"
    ))
    .bind(wid)
    .bind(&filename)
    .bind(&key)
    .bind(bytes.len() as i64)
    .bind(&content_type)
    .bind(user.user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?
    .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("insert failed")))?;

    broadcast_fs_changed(&state, wid);
    Ok((StatusCode::CREATED, Json(row.to_api())))
}

async fn delete_file(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let row = file_row(&state, id).await?;
    require_editor(&state, &user, row.workspace_id).await?;
    ensure_in_workspace(row.workspace_id, &row.s3_key)?;
    let _ = state
        .storage
        .client
        .delete_object()
        .bucket(&state.storage.bucket)
        .key(&row.s3_key)
        .send()
        .await;
    sqlx::query("DELETE FROM workspace_files WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;
    broadcast_fs_changed(&state, row.workspace_id);
    Ok(StatusCode::NO_CONTENT)
}

async fn download_url(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let row = file_row(&state, id).await?;
    require_member(&state, &user, row.workspace_id).await?;
    ensure_in_workspace(row.workspace_id, &row.s3_key)?;
    let pc = PresigningConfig::expires_in(Duration::from_secs(900)).map_err(s3_err)?;
    let presigned = state
        .storage
        .client
        .get_object()
        .bucket(&state.storage.bucket)
        .key(&row.s3_key)
        .presigned(pc)
        .await
        .map_err(s3_err)?;
    Ok(Json(json!({ "url": presigned.uri().to_string() })))
}

async fn read_text(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let row = file_row(&state, id).await?;
    require_member(&state, &user, row.workspace_id).await?;
    ensure_in_workspace(row.workspace_id, &row.s3_key)?;
    let (bytes, version) = s3_get(&state, &row.s3_key).await?;
    Ok(Json(json!({
        "content": String::from_utf8_lossy(&bytes),
        "version": version,
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WriteBody {
    content: String,
    expected_version: String,
}

async fn write_text(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<WriteBody>,
) -> ApiResult<Json<Value>> {
    let row = file_row(&state, id).await?;
    require_editor(&state, &user, row.workspace_id).await?;
    ensure_in_workspace(row.workspace_id, &row.s3_key)?;

    // Optimistic Concurrency: aktuelle Version muss zur erwarteten passen.
    let (_, current) = s3_get(&state, &row.s3_key).await?;
    if !body.expected_version.is_empty() && body.expected_version != current {
        return Err(ApiError::VersionConflict);
    }

    let put = state
        .storage
        .client
        .put_object()
        .bucket(&state.storage.bucket)
        .key(&row.s3_key)
        .content_type(&row.content_type)
        .body(ByteStream::from(body.content.into_bytes()))
        .send()
        .await
        .map_err(s3_err)?;
    let new_version = put.e_tag().unwrap_or_default().to_string();
    broadcast_fs_changed(&state, row.workspace_id);
    Ok(Json(json!({ "version": new_version })))
}

async fn preview_file(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, kind)): Path<(Uuid, String)>,
    Query(q): Query<PreviewQuery>,
) -> ApiResult<Json<Value>> {
    let row = file_row(&state, id).await?;
    require_member(&state, &user, row.workspace_id).await?;
    ensure_in_workspace(row.workspace_id, &row.s3_key)?;
    let (bytes, _) = s3_get(&state, &row.s3_key).await?;

    let result = match kind.as_str() {
        "docx" => preview::docx_html(&bytes),
        "xlsx" => preview::xlsx_preview(&bytes, q.sheet.as_deref()),
        "pptx" => preview::pptx_preview(&bytes),
        _ => return Err(ApiError::BadRequest("Unbekannter Typ.".into())),
    };
    result
        .map(Json)
        .map_err(|e| ApiError::BadRequest(format!("Vorschau fehlgeschlagen: {e}")))
}

#[derive(Deserialize)]
struct PreviewQuery {
    sheet: Option<String>,
}

/// Best-effort `fs-changed`-Broadcast (WS-Hub kommt in Phase 6 — bis dahin
/// No-op-Platzhalter, damit Aufrufstellen stabil bleiben).
fn broadcast_fs_changed(_state: &AppState, _workspace_id: Uuid) {
    // TODO Phase 6: an Workspace-Mitglieder über den WS-Hub broadcasten.
}
