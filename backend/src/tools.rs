//! Skill-/Tool-Registry (Phase 6b-1). Read-only mit dem Backend gebündelt
//! (CLAUDE.md §3 Regel 7). Erste Skill: `files` (Workspace-Dateien lesen +
//! per HITL anhängen). docx/xlsx/Template/Delegation folgen in 6b-2+.

use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::sandbox::{ensure_in_workspace, sanitize_filename, workspace_key};
use crate::AppState;

/// Tool-Definition (providerneutral; `schema` = JSON-Schema der Parameter).
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: Value,
}

pub const WRITE_TOOL: &str = "append_to_file";
pub const WRITE_XLSX_TOOL: &str = "write_xlsx";
pub const ASK_TOOL: &str = "ask_user";

fn all_tools() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "list_files",
            description: "Listet alle Dateien im Workspace.",
            schema: json!({ "type": "object", "properties": {} }),
        },
        ToolSpec {
            name: "read_file",
            description: "Liest den Textinhalt einer Workspace-Datei.",
            schema: json!({
                "type": "object",
                "properties": { "filename": { "type": "string" } },
                "required": ["filename"]
            }),
        },
        ToolSpec {
            name: WRITE_TOOL,
            description: "Hängt Text an eine Workspace-Datei an (legt sie \
                bei Bedarf an). Erfordert Nutzer-Freigabe.",
            schema: json!({
                "type": "object",
                "properties": {
                    "filename": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["filename", "content"]
            }),
        },
        ToolSpec {
            name: WRITE_XLSX_TOOL,
            description: "Schreibt eine Excel-Datei (.xlsx) mit den \
                angegebenen Zeilen (überschreibt eine bestehende). \
                Erfordert Nutzer-Freigabe.",
            schema: json!({
                "type": "object",
                "properties": {
                    "filename": { "type": "string" },
                    "sheet": { "type": "string" },
                    "rows": {
                        "type": "array",
                        "items": {
                            "type": "array",
                            "items": { "type": "string" }
                        }
                    }
                },
                "required": ["filename", "rows"]
            }),
        },
        ToolSpec {
            name: ASK_TOOL,
            description: "Stellt dem Nutzer eine Rückfrage und wartet auf \
                dessen Freitext-Antwort, bevor weitergearbeitet wird.",
            schema: json!({
                "type": "object",
                "properties": { "question": { "type": "string" } },
                "required": ["question"]
            }),
        },
    ]
}

/// Tools, die der Agent gemäß seiner aktivierten Skills nutzen darf.
pub fn available_tools(skills: &[String]) -> Vec<ToolSpec> {
    if skills.iter().any(|s| s == "files") {
        all_tools()
    } else {
        Vec::new()
    }
}

pub fn is_write_tool(name: &str) -> bool {
    name == WRITE_TOOL || name == WRITE_XLSX_TOOL
}

pub fn is_ask_tool(name: &str) -> bool {
    name == ASK_TOOL
}

/// `GET /skills`-Payload (Frontend-`Skill`-Vertrag).
pub fn skills_json() -> Value {
    json!([{
        "name": "files",
        "title": "Dateien",
        "description": "Workspace-Dateien lesen und (mit Freigabe) ergänzen.",
        "icon": "Folder",
        "tools": ["list_files", "read_file", WRITE_TOOL, WRITE_XLSX_TOOL, ASK_TOOL],
        "hitl": { "default": true },
        "language": "de",
        "body": "",
        "acceptsAttachments": []
    }])
}

// --- Ausführung -----------------------------------------------------------

async fn list_files(state: &AppState, wid: Uuid) -> ApiResult<String> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT filename FROM workspace_files \
         WHERE workspace_id = $1 ORDER BY filename",
    )
    .bind(wid)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    if rows.is_empty() {
        return Ok("(keine Dateien)".into());
    }
    Ok(rows.into_iter().map(|r| r.0).collect::<Vec<_>>().join("\n"))
}

fn read_file(state: &AppState, wid: Uuid, filename: &str) -> ApiResult<String> {
    let fname = sanitize_filename(filename)?;
    let key = workspace_key(wid, &fname);
    ensure_in_workspace(wid, &key)?;
    match std::fs::read(state.storage.path(&key)) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(s) => Ok(s),
            Err(_) => Ok("(Datei ist nicht als Text lesbar)".into()),
        },
        Err(_) => Ok("(Datei nicht gefunden)".into()),
    }
}

/// Read-Tools ohne Seiteneffekt (kein HITL nötig).
pub async fn execute_read_tool(
    state: &AppState,
    wid: Uuid,
    name: &str,
    input: &Value,
) -> ApiResult<String> {
    match name {
        "list_files" => list_files(state, wid).await,
        "read_file" => {
            let fname = input
                .get("filename")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ApiError::BadRequest("filename fehlt".into()))?;
            read_file(state, wid, fname)
        }
        other => Err(ApiError::BadRequest(format!("Unbekanntes Tool: {other}"))),
    }
}

/// Letzte ~400 Zeichen einer bestehenden Datei (für die HITL-Vorschau).
pub fn append_preview(
    state: &AppState,
    wid: Uuid,
    filename: &str,
    content: &str,
) -> ApiResult<Value> {
    let fname = sanitize_filename(filename)?;
    let key = workspace_key(wid, &fname);
    ensure_in_workspace(wid, &key)?;
    let existing = std::fs::read_to_string(state.storage.path(&key)).ok();
    let creates = existing.is_none();
    let tail = existing.map(|s| {
        let start = s.len().saturating_sub(400);
        s[start..].to_string()
    });
    Ok(json!({
        "kind": "appendToFile",
        "path": fname,
        "content": content,
        "createsFile": creates,
        "existingTail": tail,
    }))
}

/// Führt das Write-Tool **nach** HITL-Freigabe aus: hängt an, aktualisiert
/// `workspace_files` und gibt eine Tool-Result-Meldung zurück.
pub async fn do_append(
    state: &AppState,
    wid: Uuid,
    uploaded_by: Uuid,
    filename: &str,
    content: &str,
) -> ApiResult<String> {
    let fname = sanitize_filename(filename)?;
    let key = workspace_key(wid, &fname);
    ensure_in_workspace(wid, &key)?;
    let path = state.storage.path(&key);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ApiError::Internal(anyhow::anyhow!("{e}")))?;
    }
    let mut prev = std::fs::read_to_string(&path).unwrap_or_default();
    prev.push_str(content);
    std::fs::write(&path, prev.as_bytes())
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("{e}")))?;
    let size = prev.len() as i64;

    sqlx::query(
        "INSERT INTO workspace_files \
         (workspace_id, filename, s3_key, size_bytes, content_type, uploaded_by) \
         VALUES ($1,$2,$3,$4,'text/plain',$5) \
         ON CONFLICT (workspace_id, filename) DO UPDATE SET \
           size_bytes = EXCLUDED.size_bytes, uploaded_by = EXCLUDED.uploaded_by, \
           uploaded_at = now()",
    )
    .bind(wid)
    .bind(&fname)
    .bind(&key)
    .bind(size)
    .bind(uploaded_by)
    .execute(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;

    state
        .ws
        .publish(Some(wid), "fs-changed", serde_json::Value::Null);
    Ok(format!("An '{fname}' angehängt ({} Bytes gesamt).", size))
}

// --- xlsx schreiben (Phase 6b-2b) -----------------------------------------

fn cells_from_input(input: &Value) -> Vec<Vec<String>> {
    input
        .get("rows")
        .and_then(|r| r.as_array())
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    row.as_array()
                        .map(|cells| {
                            cells
                                .iter()
                                .map(|c| match c {
                                    Value::String(s) => s.clone(),
                                    other => other.to_string(),
                                })
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default()
}

fn xlsx_preview(
    state: &AppState,
    wid: Uuid,
    filename: &str,
    sheet: &str,
    rows: &[Vec<String>],
) -> ApiResult<Value> {
    let fname = sanitize_filename(filename)?;
    let key = workspace_key(wid, &fname);
    ensure_in_workspace(wid, &key)?;
    let creates = !state.storage.path(&key).exists();
    Ok(json!({
        "kind": "writeXlsx",
        "path": fname,
        "sheet": sheet,
        "rows": rows,
        "createsFile": creates,
    }))
}

async fn do_write_xlsx(
    state: &AppState,
    wid: Uuid,
    uploaded_by: Uuid,
    filename: &str,
    sheet: &str,
    rows: &[Vec<String>],
) -> ApiResult<String> {
    let fname = sanitize_filename(filename)?;
    if !fname.to_lowercase().ends_with(".xlsx") {
        return Err(ApiError::BadRequest(
            "Dateiname muss auf .xlsx enden.".into(),
        ));
    }
    let key = workspace_key(wid, &fname);
    ensure_in_workspace(wid, &key)?;

    let mut wb = rust_xlsxwriter::Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name(sheet)
        .map_err(|e| ApiError::BadRequest(format!("Sheet-Name: {e}")))?;
    for (r, row) in rows.iter().enumerate() {
        for (c, val) in row.iter().enumerate() {
            ws.write_string(r as u32, c as u16, val)
                .map_err(|e| ApiError::Internal(anyhow::anyhow!("{e}")))?;
        }
    }
    let bytes = wb
        .save_to_buffer()
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("{e}")))?;

    let path = state.storage.path(&key);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ApiError::Internal(anyhow::anyhow!("{e}")))?;
    }
    let size = bytes.len() as i64;
    std::fs::write(&path, &bytes).map_err(|e| ApiError::Internal(anyhow::anyhow!("{e}")))?;

    sqlx::query(
        "INSERT INTO workspace_files \
         (workspace_id, filename, s3_key, size_bytes, content_type, uploaded_by) \
         VALUES ($1,$2,$3,$4,\
           'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',\
           $5) \
         ON CONFLICT (workspace_id, filename) DO UPDATE SET \
           size_bytes = EXCLUDED.size_bytes, uploaded_by = EXCLUDED.uploaded_by, \
           uploaded_at = now()",
    )
    .bind(wid)
    .bind(&fname)
    .bind(&key)
    .bind(size)
    .bind(uploaded_by)
    .execute(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;

    state
        .ws
        .publish(Some(wid), "fs-changed", serde_json::Value::Null);
    Ok(format!(
        "'{fname}' geschrieben ({} Zeilen, Sheet '{sheet}').",
        rows.len()
    ))
}

// --- Write-Tool-Dispatcher ------------------------------------------------

fn str_in(input: &Value, key: &str) -> String {
    input
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// HITL-Vorschau für ein Write-Tool (vor der Freigabe).
pub fn write_preview(state: &AppState, wid: Uuid, name: &str, input: &Value) -> ApiResult<Value> {
    match name {
        WRITE_TOOL => append_preview(
            state,
            wid,
            &str_in(input, "filename"),
            &str_in(input, "content"),
        ),
        WRITE_XLSX_TOOL => {
            let sheet = input
                .get("sheet")
                .and_then(|s| s.as_str())
                .unwrap_or("Tabelle1");
            xlsx_preview(
                state,
                wid,
                &str_in(input, "filename"),
                sheet,
                &cells_from_input(input),
            )
        }
        other => Err(ApiError::BadRequest(format!("Kein Write-Tool: {other}"))),
    }
}

/// Führt das Write-Tool **nach** HITL-Freigabe aus.
pub async fn execute_write(
    state: &AppState,
    wid: Uuid,
    uploaded_by: Uuid,
    name: &str,
    input: &Value,
) -> ApiResult<String> {
    match name {
        WRITE_TOOL => {
            do_append(
                state,
                wid,
                uploaded_by,
                &str_in(input, "filename"),
                &str_in(input, "content"),
            )
            .await
        }
        WRITE_XLSX_TOOL => {
            let sheet = input
                .get("sheet")
                .and_then(|s| s.as_str())
                .unwrap_or("Tabelle1")
                .to_string();
            do_write_xlsx(
                state,
                wid,
                uploaded_by,
                &str_in(input, "filename"),
                &sheet,
                &cells_from_input(input),
            )
            .await
        }
        other => Err(ApiError::BadRequest(format!("Kein Write-Tool: {other}"))),
    }
}
