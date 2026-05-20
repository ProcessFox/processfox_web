//! Workspace-Dateien (Phase 5, **lokales Volume** statt S3). Upload →
//! Dateisystem unter `STORAGE_DIR/workspaces/<wid>/<datei>`, Liste, Löschen,
//! Download über kurzlebig signierten Link, Text-Read/Write mit
//! mtime-Konkurrenz, Office-Vorschau. Pfade gehen immer durch
//! `crate::sandbox` (kein Traversal).

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::perm::require_member;
use crate::sandbox::{ensure_in_workspace, sanitize_filename, workspace_key};
use crate::{preview, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/workspaces/{wid}/files", get(list_files).post(upload_file))
        .route("/files/{id}", axum::routing::delete(delete_file))
        .route("/files/{id}/download-url", get(download_url))
        // Token-gesichert (kein Auth-Header) — taugt als <img>/PDF-Quelle.
        .route("/files/{id}/raw", get(raw_file))
        .route("/files/{id}/text", get(read_text).put(write_text))
        .route("/files/{id}/preview/{kind}", get(preview_file))
        .layer(DefaultBodyLimit::max(MAX_BYTES + 1024 * 1024))
}

const MAX_BYTES: usize = 50 * 1024 * 1024;
const ALLOWED: [&str; 11] = [
    "md", "txt", "pdf", "docx", "xlsx", "csv", "png", "jpg", "jpeg", "webp", "pptx",
];
const LINK_TTL_SECS: u64 = 900;

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

fn io_err<E: std::fmt::Display>(e: E) -> ApiError {
    ApiError::Internal(anyhow::anyhow!("Storage: {e}"))
}

async fn file_row(state: &AppState, id: Uuid) -> ApiResult<FileRow> {
    sqlx::query_as::<_, FileRow>(&format!("SELECT {COLS} FROM workspace_files WHERE id = $1"))
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .ok_or(ApiError::NotFound)
}

/// Versions-Token = mtime in Millisekunden (Optimistic Concurrency).
fn mtime_version(path: &std::path::Path) -> String {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis().to_string())
        .unwrap_or_default()
}

// --- Signierter Download-Link --------------------------------------------

#[derive(Serialize, Deserialize)]
struct LinkClaims {
    /// File-ID.
    fid: String,
    exp: usize,
}

fn sign_link(secret: &str, file_id: Uuid) -> ApiResult<String> {
    let exp = (SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        + LINK_TTL_SECS) as usize;
    encode(
        &Header::new(Algorithm::HS256),
        &LinkClaims {
            fid: file_id.to_string(),
            exp,
        },
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| ApiError::Internal(anyhow::anyhow!("Link-Signatur: {e}")))
}

fn verify_link(secret: &str, token: &str, file_id: Uuid) -> ApiResult<()> {
    let data = decode::<LinkClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )
    .map_err(|_| ApiError::Unauthorized)?;
    if data.claims.fid != file_id.to_string() {
        return Err(ApiError::Unauthorized);
    }
    Ok(())
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
    require_member(&state, &user, wid).await?;

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
    let path = state.storage.path(&key);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io_err)?;
    }
    std::fs::write(&path, &bytes).map_err(io_err)?;

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
    require_member(&state, &user, row.workspace_id).await?;
    ensure_in_workspace(row.workspace_id, &row.s3_key)?;
    let _ = std::fs::remove_file(state.storage.path(&row.s3_key));
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
    let token = sign_link(&state.config.jwt_secret, row.id)?;
    Ok(Json(json!({
        "url": format!("/api/v1/files/{}/raw?token={token}", row.id)
    })))
}

#[derive(Deserialize)]
struct RawQuery {
    token: String,
}

/// Liefert die Datei-Bytes; autorisiert über den signierten Token (kein
/// Auth-Header — funktioniert als `<img src>` / PDF-Quelle).
async fn raw_file(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<RawQuery>,
) -> ApiResult<Response> {
    verify_link(&state.config.jwt_secret, &q.token, id)?;
    let row = file_row(&state, id).await?;
    ensure_in_workspace(row.workspace_id, &row.s3_key)?;
    let bytes = std::fs::read(state.storage.path(&row.s3_key)).map_err(io_err)?;
    Response::builder()
        .header(header::CONTENT_TYPE, &row.content_type)
        .header(
            header::CONTENT_DISPOSITION,
            format!("inline; filename=\"{}\"", row.filename),
        )
        .body(Body::from(bytes))
        .map_err(io_err)
}

async fn read_text(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let row = file_row(&state, id).await?;
    require_member(&state, &user, row.workspace_id).await?;
    ensure_in_workspace(row.workspace_id, &row.s3_key)?;
    let path = state.storage.path(&row.s3_key);
    let bytes = std::fs::read(&path).map_err(io_err)?;
    Ok(Json(json!({
        "content": String::from_utf8_lossy(&bytes),
        "version": mtime_version(&path),
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
    require_member(&state, &user, row.workspace_id).await?;
    ensure_in_workspace(row.workspace_id, &row.s3_key)?;
    let path = state.storage.path(&row.s3_key);

    // Optimistic Concurrency: aktuelle mtime muss zur erwarteten passen.
    if !body.expected_version.is_empty() && body.expected_version != mtime_version(&path) {
        return Err(ApiError::VersionConflict);
    }
    std::fs::write(&path, body.content.as_bytes()).map_err(io_err)?;
    broadcast_fs_changed(&state, row.workspace_id);
    Ok(Json(json!({ "version": mtime_version(&path) })))
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
    let bytes = std::fs::read(state.storage.path(&row.s3_key)).map_err(io_err)?;

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

/// `fs-changed` an alle Mitglieder des Workspaces (Phase 6a, WS-Hub).
fn broadcast_fs_changed(state: &AppState, workspace_id: Uuid) {
    state
        .ws
        .publish(Some(workspace_id), "fs-changed", serde_json::Value::Null);
}
