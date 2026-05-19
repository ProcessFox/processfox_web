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
pub const WRITE_DOCX_TOOL: &str = "write_docx";
pub const WRITE_DOCX_TPL_TOOL: &str = "write_docx_from_template";
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
            name: WRITE_DOCX_TOOL,
            description: "Schreibt eine Word-Datei (.docx) aus den \
                angegebenen Absätzen (überschreibt eine bestehende). \
                Erfordert Nutzer-Freigabe.",
            schema: json!({
                "type": "object",
                "properties": {
                    "filename": { "type": "string" },
                    "paragraphs": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
                },
                "required": ["filename", "paragraphs"]
            }),
        },
        ToolSpec {
            name: WRITE_DOCX_TPL_TOOL,
            description: "Füllt eine vorhandene Word-Vorlage (.docx im \
                Workspace) aus: ersetzt {{Platzhalter}} durch Werte und \
                schreibt das Ergebnis als neue .docx. Erfordert Freigabe.",
            schema: json!({
                "type": "object",
                "properties": {
                    "templateFilename": { "type": "string" },
                    "outputFilename": { "type": "string" },
                    "replacements": {
                        "type": "object",
                        "additionalProperties": { "type": "string" }
                    }
                },
                "required": ["templateFilename", "outputFilename",
                             "replacements"]
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
    name == WRITE_TOOL
        || name == WRITE_XLSX_TOOL
        || name == WRITE_DOCX_TOOL
        || name == WRITE_DOCX_TPL_TOOL
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
        "tools": ["list_files", "read_file", WRITE_TOOL, WRITE_XLSX_TOOL,
                  WRITE_DOCX_TOOL, WRITE_DOCX_TPL_TOOL, ASK_TOOL],
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

// --- docx schreiben (Phase 6b-2c) -----------------------------------------

fn paras_from_input(input: &Value) -> Vec<String> {
    input
        .get("paragraphs")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .map(|c| match c {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Minimales, gültiges .docx (OOXML-Zip ohne extra Dependency).
fn build_docx(paragraphs: &[String]) -> ApiResult<Vec<u8>> {
    use std::io::Write;
    let body: String = paragraphs
        .iter()
        .map(|p| {
            format!(
                "<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
                xml_escape(p)
            )
        })
        .collect();
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:body>{body}<w:sectPr/></w:body></w:document>"
    );
    const CONTENT_TYPES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
<Default Extension=\"xml\" ContentType=\"application/xml\"/>\
<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
</Types>";
    const RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
</Relationships>";

    let mut zw = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts: zip::write::SimpleFileOptions = Default::default();
    let mut put = |name: &str, data: &str| -> ApiResult<()> {
        zw.start_file(name, opts)
            .map_err(|e| ApiError::Internal(anyhow::anyhow!("{e}")))?;
        zw.write_all(data.as_bytes())
            .map_err(|e| ApiError::Internal(anyhow::anyhow!("{e}")))?;
        Ok(())
    };
    put("[Content_Types].xml", CONTENT_TYPES)?;
    put("_rels/.rels", RELS)?;
    put("word/document.xml", &document)?;
    let cursor = zw
        .finish()
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("{e}")))?;
    Ok(cursor.into_inner())
}

fn docx_preview(
    state: &AppState,
    wid: Uuid,
    filename: &str,
    paragraphs: &[String],
) -> ApiResult<Value> {
    let fname = sanitize_filename(filename)?;
    let key = workspace_key(wid, &fname);
    ensure_in_workspace(wid, &key)?;
    let creates = !state.storage.path(&key).exists();
    let mut preview_text = paragraphs.join("\n");
    if preview_text.len() > 800 {
        preview_text.truncate(800);
        preview_text.push('…');
    }
    Ok(json!({
        "kind": "writeDocx",
        "path": fname,
        "blockCount": paragraphs.len(),
        "previewText": preview_text,
        "createsFile": creates,
    }))
}

async fn do_write_docx(
    state: &AppState,
    wid: Uuid,
    uploaded_by: Uuid,
    filename: &str,
    paragraphs: &[String],
) -> ApiResult<String> {
    let fname = sanitize_filename(filename)?;
    if !fname.to_lowercase().ends_with(".docx") {
        return Err(ApiError::BadRequest(
            "Dateiname muss auf .docx enden.".into(),
        ));
    }
    let key = workspace_key(wid, &fname);
    ensure_in_workspace(wid, &key)?;
    let bytes = build_docx(paragraphs)?;

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
           'application/vnd.openxmlformats-officedocument.wordprocessingml.document',\
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
        "'{fname}' geschrieben ({} Absätze).",
        paragraphs.len()
    ))
}

// --- docx aus Vorlage (Phase 6b-2d) ---------------------------------------

fn reps_from_input(input: &Value) -> Vec<(String, String)> {
    input
        .get("replacements")
        .and_then(|r| r.as_object())
        .map(|m| {
            m.iter()
                .map(|(k, v)| {
                    let val = match v {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    (k.clone(), val)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `{{...}}`-Platzhalter aus dem document.xml-Text sammeln (heuristisch:
/// keine `<`/Zeilenumbrüche im Token → ignoriert run-übergreifende).
fn scan_placeholders(xml: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let bytes = xml.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if &xml[i..i + 2] == "{{" {
            if let Some(end) = xml[i + 2..].find("}}") {
                let inner = &xml[i + 2..i + 2 + end];
                if !inner.contains('<') && !inner.contains('\n') {
                    let k = inner.trim().to_string();
                    if !k.is_empty() && !out.contains(&k) {
                        out.push(k);
                    }
                }
                i += 2 + end + 2;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn read_template_doc(
    state: &AppState,
    wid: Uuid,
    template_filename: &str,
) -> ApiResult<(Vec<u8>, String)> {
    let tname = sanitize_filename(template_filename)?;
    if !tname.to_lowercase().ends_with(".docx") {
        return Err(ApiError::BadRequest("Vorlage muss eine .docx sein.".into()));
    }
    let key = workspace_key(wid, &tname);
    ensure_in_workspace(wid, &key)?;
    let bytes = std::fs::read(state.storage.path(&key))
        .map_err(|_| ApiError::BadRequest(format!("Vorlage '{tname}' nicht gefunden.")))?;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes))
        .map_err(|e| ApiError::BadRequest(format!("Keine gültige .docx: {e}")))?;
    let doc = {
        use std::io::Read;
        let mut f = zip
            .by_name("word/document.xml")
            .map_err(|_| ApiError::BadRequest("Vorlage ohne word/document.xml.".into()))?;
        let mut s = String::new();
        f.read_to_string(&mut s)
            .map_err(|e| ApiError::Internal(anyhow::anyhow!("{e}")))?;
        s
    };
    Ok((bytes, doc))
}

fn fill_template(template_bytes: &[u8], reps: &[(String, String)]) -> ApiResult<Vec<u8>> {
    use std::io::{Read, Write};
    let mut zin = zip::ZipArchive::new(std::io::Cursor::new(template_bytes))
        .map_err(|e| ApiError::BadRequest(format!("Keine gültige .docx: {e}")))?;
    let mut zw = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts: zip::write::SimpleFileOptions = Default::default();
    for i in 0..zin.len() {
        let mut f = zin
            .by_index(i)
            .map_err(|e| ApiError::Internal(anyhow::anyhow!("{e}")))?;
        if f.is_dir() {
            continue;
        }
        let name = f.name().to_string();
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)
            .map_err(|e| ApiError::Internal(anyhow::anyhow!("{e}")))?;
        let data = if name == "word/document.xml" {
            let mut s = String::from_utf8_lossy(&buf).into_owned();
            for (k, v) in reps {
                s = s.replace(&format!("{{{{{k}}}}}"), &xml_escape(v));
            }
            s.into_bytes()
        } else {
            buf
        };
        zw.start_file(name, opts)
            .map_err(|e| ApiError::Internal(anyhow::anyhow!("{e}")))?;
        zw.write_all(&data)
            .map_err(|e| ApiError::Internal(anyhow::anyhow!("{e}")))?;
    }
    let cur = zw
        .finish()
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("{e}")))?;
    Ok(cur.into_inner())
}

fn tpl_preview(state: &AppState, wid: Uuid, input: &Value) -> ApiResult<Value> {
    let template = str_in(input, "templateFilename");
    let output = sanitize_filename(&str_in(input, "outputFilename"))?;
    let reps = reps_from_input(input);
    let (_, doc) = read_template_doc(state, wid, &template)?;
    let placeholders = scan_placeholders(&doc);
    let out_key = workspace_key(wid, &output);
    ensure_in_workspace(wid, &out_key)?;
    Ok(json!({
        "kind": "writeDocxFromTemplate",
        "templatePath": sanitize_filename(&template)?,
        "outputPath": output,
        "replacements": reps.iter()
            .map(|(k, v)| json!({ "key": k, "value": v }))
            .collect::<Vec<_>>(),
        "templatePlaceholders": placeholders,
        "createsFile": !state.storage.path(&out_key).exists(),
    }))
}

async fn do_write_docx_from_template(
    state: &AppState,
    wid: Uuid,
    uploaded_by: Uuid,
    input: &Value,
) -> ApiResult<String> {
    let template = str_in(input, "templateFilename");
    let output = sanitize_filename(&str_in(input, "outputFilename"))?;
    if !output.to_lowercase().ends_with(".docx") {
        return Err(ApiError::BadRequest(
            "Ausgabedatei muss auf .docx enden.".into(),
        ));
    }
    let reps = reps_from_input(input);
    let (tpl_bytes, _) = read_template_doc(state, wid, &template)?;
    let bytes = fill_template(&tpl_bytes, &reps)?;

    let key = workspace_key(wid, &output);
    ensure_in_workspace(wid, &key)?;
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
           'application/vnd.openxmlformats-officedocument.wordprocessingml.document',\
           $5) \
         ON CONFLICT (workspace_id, filename) DO UPDATE SET \
           size_bytes = EXCLUDED.size_bytes, uploaded_by = EXCLUDED.uploaded_by, \
           uploaded_at = now()",
    )
    .bind(wid)
    .bind(&output)
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
        "'{output}' aus Vorlage '{}' erzeugt ({} Ersetzungen).",
        sanitize_filename(&template)?,
        reps.len()
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
        WRITE_DOCX_TOOL => docx_preview(
            state,
            wid,
            &str_in(input, "filename"),
            &paras_from_input(input),
        ),
        WRITE_DOCX_TPL_TOOL => tpl_preview(state, wid, input),
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
        WRITE_DOCX_TOOL => {
            do_write_docx(
                state,
                wid,
                uploaded_by,
                &str_in(input, "filename"),
                &paras_from_input(input),
            )
            .await
        }
        WRITE_DOCX_TPL_TOOL => do_write_docx_from_template(state, wid, uploaded_by, input).await,
        other => Err(ApiError::BadRequest(format!("Kein Write-Tool: {other}"))),
    }
}
