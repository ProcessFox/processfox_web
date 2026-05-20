//! Skill-/Tool-Registry (Phase 6b-1). Read-only mit dem Backend gebündelt
//! (CLAUDE.md §3 Regel 7). Erste Skill: `files` (Workspace-Dateien lesen +
//! per HITL anhängen). docx/xlsx/Template/Delegation folgen in 6b-2+.

use calamine::{Data, Reader, Xlsx};
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
pub const APPEND_DOCX_TOOL: &str = "append_to_docx";
pub const UPDATE_CELLS_TOOL: &str = "update_cells";
pub const DELEGATE_TOOL: &str = "delegate_into_xlsx_column";
pub const ASK_TOOL: &str = "ask_user";
pub const GREP_TOOL: &str = "grep_in_files";
pub const READ_PDF_TOOL: &str = "read_pdf";
/// Sicherheits-Obergrenze für Bulk-Delegation (eine Inferenz je Zeile).
pub const DELEGATE_MAX_ROWS: usize = 200;

/// Caps für `grep_in_files` (analog Local).
const GREP_MAX_FILES: usize = 300;
const GREP_MAX_FILE_BYTES: i64 = 2 * 1024 * 1024;
const GREP_MAX_HITS: usize = 100;
const GREP_SNIPPET_CHARS: usize = 200;

/// Caps für `read_pdf`. Eingabe: tighter als das Upload-Limit (50 MiB),
/// weil Parsen einer großen PDF einen Worker-Thread sekundenlang belegen
/// kann — Multi-Tenant-Schutz, den Local nicht brauchte. Ausgabe: analog
/// Local (200 KiB Plain-Text).
const READ_PDF_MAX_INPUT_BYTES: i64 = 20 * 1024 * 1024;
const READ_PDF_MAX_OUTPUT_BYTES: usize = 200 * 1024;
/// Whitelist textbasierter Endungen — Office-Formate (pdf/docx/xlsx/pptx)
/// und Bilder werden bewusst ausgeschlossen.
const GREP_SCAN_EXTENSIONS: &[&str] = &[
    "md", "txt", "csv", "json", "yaml", "yml", "toml", "html", "htm", "xml", "rs", "ts", "tsx",
    "js", "jsx", "py", "go", "c", "cpp", "h", "hpp", "sh",
];

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
            name: GREP_TOOL,
            description: "Sucht per Regex in den Textdateien des Workspaces \
                (.md, .txt, .csv, .json, .yaml, .toml, .html, .xml und \
                gängige Source-Endungen; Binär-/Office-Formate werden \
                übersprungen). Liefert bis zu 100 Treffer mit \
                Dateiname:Zeile: Snippet. Standardmäßig case-insensitiv.",
            schema: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regulärer Ausdruck (Rust-`regex`-Syntax)."
                    },
                    "caseSensitive": {
                        "type": "boolean",
                        "description": "Wenn `true`, Groß-/Kleinschreibung \
                            beachten. Default: false."
                    }
                },
                "required": ["pattern"]
            }),
        },
        ToolSpec {
            name: READ_PDF_TOOL,
            description: "Extrahiert den Text aus einer PDF-Datei im \
                Workspace. Liefert Klartext (max. ~200 KB, danach \
                gekürzt). Funktioniert für digitale PDFs; gescannte \
                PDFs ohne OCR-Layer liefern leeren oder unbrauchbaren \
                Text. Eingabe max. 20 MB.",
            schema: json!({
                "type": "object",
                "properties": {
                    "filename": {
                        "type": "string",
                        "description": "Workspace-Datei, muss auf .pdf enden."
                    }
                },
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
            name: APPEND_DOCX_TOOL,
            description: "Hängt Absätze an eine vorhandene Word-Datei \
                (.docx) an (legt sie bei Bedarf neu an). Erfordert \
                Nutzer-Freigabe.",
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
            name: UPDATE_CELLS_TOOL,
            description: "Ändert gezielt einzelne Zellen einer vorhandenen \
                .xlsx (z. B. {\"B2\":\"42\"}). Erfordert Nutzer-Freigabe. \
                Nur das Zielblatt bleibt erhalten; Formeln/Formate gehen \
                verloren.",
            schema: json!({
                "type": "object",
                "properties": {
                    "filename": { "type": "string" },
                    "sheet": { "type": "string" },
                    "changes": {
                        "type": "object",
                        "additionalProperties": { "type": "string" }
                    }
                },
                "required": ["filename", "changes"]
            }),
        },
        ToolSpec {
            name: DELEGATE_TOOL,
            description: "Verarbeitet eine .xlsx Zeile für Zeile mit je \
                einer fokussierten KI-Inferenz und schreibt das Ergebnis \
                in eine Zielspalte. Im promptTemplate referenzieren \
                {{Spaltenüberschrift}} oder {{A}} andere Spalten der Zeile. \
                Erfordert Nutzer-Freigabe.",
            schema: json!({
                "type": "object",
                "properties": {
                    "filename": { "type": "string" },
                    "sheet": { "type": "string" },
                    "promptTemplate": { "type": "string" },
                    "targetColumn": {
                        "type": "string",
                        "description": "Spaltenbuchstabe oder Überschrift; \
                            unbekannt → neue Spalte am Ende."
                    }
                },
                "required": ["filename", "promptTemplate", "targetColumn"]
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
        || name == APPEND_DOCX_TOOL
        || name == UPDATE_CELLS_TOOL
}

pub fn is_ask_tool(name: &str) -> bool {
    name == ASK_TOOL
}

/// Delegation läuft nicht über den Write-Dispatcher, sondern als
/// Sonderzweig in `chat.rs` (Worker-LLM + Fortschrittsevents).
pub fn is_delegate_tool(name: &str) -> bool {
    name == DELEGATE_TOOL
}

/// `GET /skills`-Payload (Frontend-`Skill`-Vertrag).
pub fn skills_json() -> Value {
    json!([{
        "name": "files",
        "title": "Dateien",
        "description": "Workspace-Dateien lesen und (mit Freigabe) ergänzen.",
        "icon": "Folder",
        "tools": ["list_files", "read_file", GREP_TOOL, READ_PDF_TOOL,
                  WRITE_TOOL, WRITE_XLSX_TOOL, WRITE_DOCX_TOOL,
                  WRITE_DOCX_TPL_TOOL, APPEND_DOCX_TOOL, UPDATE_CELLS_TOOL,
                  DELEGATE_TOOL, ASK_TOOL],
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

/// Workspace-weite Regex-Suche (Phase 6b-2h). Read-only Pendant zu
/// `grep_in_files` aus ProcessFox Local. Wir iterieren über die DB-Zeilen
/// (`workspace_files`) — die DB ist die Wahrheit, das Volume nur die Bytes.
async fn grep_in_files(state: &AppState, wid: Uuid, input: &Value) -> ApiResult<String> {
    let pattern = input
        .get("pattern")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::BadRequest("pattern fehlt".into()))?;
    let case_sensitive = input
        .get("caseSensitive")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let regex_src = if case_sensitive {
        pattern.to_string()
    } else {
        format!("(?i){pattern}")
    };
    let re = regex::Regex::new(&regex_src)
        .map_err(|e| ApiError::BadRequest(format!("Ungültiges Regex: {e}")))?;

    // Kandidaten kommen aus der DB, nicht aus `read_dir` — Sichtbarkeits-/
    // Permission-Invarianten leben in `workspace_files`. `ensure_in_workspace`
    // läuft trotzdem als Defense-in-Depth über jeden Storage-Key.
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT filename, s3_key, size_bytes FROM workspace_files \
         WHERE workspace_id = $1 ORDER BY filename",
    )
    .bind(wid)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;

    let mut files_scanned = 0usize;
    let mut hits: Vec<String> = Vec::new();
    let mut hit_cap_reached = false;
    for (filename, s3_key, size_bytes) in &rows {
        if files_scanned >= GREP_MAX_FILES {
            break;
        }
        if hits.len() >= GREP_MAX_HITS {
            hit_cap_reached = true;
            break;
        }
        // Endungs-Whitelist (Office/Bilder sind binär → andere Tools).
        let ext_ok = std::path::Path::new(filename.as_str())
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| GREP_SCAN_EXTENSIONS.contains(&e.to_lowercase().as_str()))
            .unwrap_or(false);
        if !ext_ok {
            continue;
        }
        if *size_bytes > GREP_MAX_FILE_BYTES {
            continue;
        }
        if ensure_in_workspace(wid, s3_key).is_err() {
            continue;
        }
        let Ok(bytes) = std::fs::read(state.storage.path(s3_key)) else {
            continue;
        };
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        files_scanned += 1;
        for (i, line) in text.lines().enumerate() {
            if hits.len() >= GREP_MAX_HITS {
                hit_cap_reached = true;
                break;
            }
            if re.is_match(line) {
                let snippet: String = line.chars().take(GREP_SNIPPET_CHARS).collect();
                hits.push(format!("{filename}:{}: {snippet}", i + 1));
            }
        }
    }

    let body = if hits.is_empty() {
        format!("Keine Treffer für /{pattern}/ in {files_scanned} Datei(en).")
    } else {
        let mut out = format!(
            "{} Treffer für /{pattern}/ in {files_scanned} Datei(en):\n\n",
            hits.len()
        );
        for h in &hits {
            out.push_str(h);
            out.push('\n');
        }
        if hit_cap_reached {
            out.push_str("\n[Trefferlimit erreicht — Muster oder Suche einschränken]");
        }
        out
    };
    Ok(body)
}

/// PDF-Text-Extraktion (Phase 6b-2i). `pdf-extract` ist CPU-gebunden und
/// läuft daher auf dem Blocking-Pool — schmal abgegrenzte Ausnahme zur
/// CLAUDE.md-§11-Regel, die für den LLM-Pfad gemünzt ist.
async fn read_pdf(state: &AppState, wid: Uuid, input: &Value) -> ApiResult<String> {
    let filename = input
        .get("filename")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::BadRequest("filename fehlt".into()))?;
    let fname = sanitize_filename(filename)?;
    if !fname.to_lowercase().ends_with(".pdf") {
        return Err(ApiError::BadRequest(
            "Dateiname muss auf .pdf enden.".into(),
        ));
    }

    // DB-Vorabcheck: Existenz + Größe. „DB ist Wahrheit, Volume ist Bytes" —
    // wenn die Zeile fehlt, ist die Datei für den Workspace nicht sichtbar,
    // egal was im Volume liegt.
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT size_bytes FROM workspace_files \
         WHERE workspace_id = $1 AND filename = $2",
    )
    .bind(wid)
    .bind(&fname)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    let Some((size_bytes,)) = row else {
        return Ok(format!("(Datei '{fname}' nicht gefunden)"));
    };
    if size_bytes > READ_PDF_MAX_INPUT_BYTES {
        return Err(ApiError::BadRequest(format!(
            "PDF zu groß ({size_bytes} Bytes, Limit {} MB).",
            READ_PDF_MAX_INPUT_BYTES / (1024 * 1024)
        )));
    }

    let key = workspace_key(wid, &fname);
    ensure_in_workspace(wid, &key)?;
    let path = state.storage.path(&key);
    if !path.is_file() {
        return Ok(format!("(Datei '{fname}' nicht gefunden)"));
    }

    // `pdf-extract` ist CPU-gebunden → Blocking-Pool, damit ein langes
    // PDF nicht den Async-Reaktor (und damit andere Nutzer) blockiert.
    let path_for_blocking = path.clone();
    let extract_join =
        tokio::task::spawn_blocking(move || pdf_extract::extract_text(&path_for_blocking)).await;

    let extracted = match extract_join {
        Ok(Ok(text)) => text,
        // Beide Fehler-Fälle (Panic im Blocking-Task; pdf-extract-Fehler)
        // werden zu einem freundlichen Tool-Result, damit ein kaputtes
        // PDF nicht den Chat-Turn abreißt.
        Ok(Err(e)) => {
            return Ok(format!("PDF konnte nicht gelesen werden ('{fname}'): {e}"));
        }
        Err(e) => {
            return Ok(format!("PDF-Extraktion abgebrochen ('{fname}'): {e}"));
        }
    };

    let total_bytes = extracted.len();
    let body = if extracted.trim().is_empty() {
        format!(
            "--- {fname} ({total_bytes} Bytes) ---\n\
             [leere Extraktion — vermutlich gescanntes PDF ohne OCR]"
        )
    } else if total_bytes > READ_PDF_MAX_OUTPUT_BYTES {
        // Char-basiert kürzen, damit wir nicht mitten in einem
        // Multi-Byte-Codepoint abschneiden.
        let truncated: String = extracted
            .chars()
            .take(READ_PDF_MAX_OUTPUT_BYTES / 4)
            .collect();
        format!(
            "--- {fname} ({total_bytes} Bytes, gekürzt) ---\n{truncated}\n\
             \n[gekürzt — Extraktion überschreitet {} KB]",
            READ_PDF_MAX_OUTPUT_BYTES / 1024
        )
    } else {
        format!("--- {fname} ({total_bytes} Bytes) ---\n{extracted}")
    };
    Ok(body)
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
        GREP_TOOL => grep_in_files(state, wid, input).await,
        READ_PDF_TOOL => read_pdf(state, wid, input).await,
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

/// xlsx-Bytes aus einem String-Grid (eine Tabelle).
fn build_xlsx_bytes(sheet: &str, rows: &[Vec<String>]) -> ApiResult<Vec<u8>> {
    let mut wb = rust_xlsxwriter::Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name(sheet)
        .map_err(|e| ApiError::BadRequest(format!("Sheet-Name: {e}")))?;
    for (r, row) in rows.iter().enumerate() {
        for (c, val) in row.iter().enumerate() {
            if val.is_empty() {
                continue;
            }
            ws.write_string(r as u32, c as u16, val)
                .map_err(|e| ApiError::Internal(anyhow::anyhow!("{e}")))?;
        }
    }
    wb.save_to_buffer()
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("{e}")))
}

/// xlsx-Bytes ins Volume schreiben + `workspace_files` upserten +
/// `fs-changed` broadcasten.
async fn save_xlsx(
    state: &AppState,
    wid: Uuid,
    uploaded_by: Uuid,
    fname: &str,
    bytes: &[u8],
) -> ApiResult<()> {
    let key = workspace_key(wid, fname);
    ensure_in_workspace(wid, &key)?;
    let path = state.storage.path(&key);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ApiError::Internal(anyhow::anyhow!("{e}")))?;
    }
    std::fs::write(&path, bytes).map_err(|e| ApiError::Internal(anyhow::anyhow!("{e}")))?;
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
    .bind(fname)
    .bind(&key)
    .bind(bytes.len() as i64)
    .bind(uploaded_by)
    .execute(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    state
        .ws
        .publish(Some(wid), "fs-changed", serde_json::Value::Null);
    Ok(())
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
    let bytes = build_xlsx_bytes(sheet, rows)?;
    save_xlsx(state, wid, uploaded_by, &fname, &bytes).await?;
    Ok(format!(
        "'{fname}' geschrieben ({} Zeilen, Sheet '{sheet}').",
        rows.len()
    ))
}

// --- xlsx-Zellen gezielt ändern (Phase 6b-2f) -----------------------------

/// `"B2"` → `(row, col)` 0-basiert (bijektives Base-26 für die Spalte).
fn parse_cell_ref(s: &str) -> Option<(usize, usize)> {
    let s = s.trim();
    let split = s.find(|c: char| c.is_ascii_digit())?;
    let (letters, digits) = s.split_at(split);
    if letters.is_empty() || digits.is_empty() {
        return None;
    }
    let mut col = 0usize;
    for ch in letters.chars() {
        if !ch.is_ascii_alphabetic() {
            return None;
        }
        col = col * 26 + (ch.to_ascii_uppercase() as usize - 'A' as usize + 1);
    }
    let row: usize = digits.parse().ok()?;
    if col == 0 || row == 0 {
        return None;
    }
    Some((row - 1, col - 1))
}

fn changes_from_input(input: &Value) -> Vec<(String, String)> {
    input
        .get("changes")
        .and_then(|c| c.as_object())
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

/// Liest eine xlsx-Datei als String-Grid (gewähltes oder erstes Sheet).
fn read_xlsx_grid(
    state: &AppState,
    wid: Uuid,
    filename: &str,
    sheet: Option<&str>,
) -> ApiResult<(String, Vec<Vec<String>>)> {
    let fname = sanitize_filename(filename)?;
    let key = workspace_key(wid, &fname);
    ensure_in_workspace(wid, &key)?;
    let bytes = std::fs::read(state.storage.path(&key))
        .map_err(|_| ApiError::BadRequest(format!("Datei '{fname}' nicht gefunden.")))?;
    let mut wb: Xlsx<_> = calamine::open_workbook_from_rs(std::io::Cursor::new(bytes))
        .map_err(|e| ApiError::BadRequest(format!("Keine xlsx: {e}")))?;
    let names = wb.sheet_names().to_vec();
    let active = sheet
        .filter(|s| names.iter().any(|n| n == s))
        .map(|s| s.to_string())
        .or_else(|| names.first().cloned())
        .ok_or_else(|| ApiError::BadRequest("Keine Tabelle.".into()))?;
    let range = wb
        .worksheet_range(&active)
        .map_err(|e| ApiError::BadRequest(format!("Sheet: {e}")))?;
    let grid = range
        .rows()
        .map(|r| {
            r.iter()
                .map(|c| match c {
                    Data::Empty => String::new(),
                    other => other.to_string(),
                })
                .collect()
        })
        .collect();
    Ok((active, grid))
}

fn set_cell(grid: &mut Vec<Vec<String>>, row: usize, col: usize, v: &str) {
    if grid.len() <= row {
        grid.resize(row + 1, Vec::new());
    }
    if grid[row].len() <= col {
        grid[row].resize(col + 1, String::new());
    }
    grid[row][col] = v.to_string();
}

fn updatecells_preview(state: &AppState, wid: Uuid, input: &Value) -> ApiResult<Value> {
    let filename = str_in(input, "filename");
    let sheet_in = input.get("sheet").and_then(|s| s.as_str());
    let (sheet, grid) = read_xlsx_grid(state, wid, &filename, sheet_in)?;
    let mut changes = Vec::new();
    for (cell, after) in changes_from_input(input) {
        let (r, c) = parse_cell_ref(&cell)
            .ok_or_else(|| ApiError::BadRequest(format!("Ungültige Zelle: {cell}")))?;
        let before = grid
            .get(r)
            .and_then(|row| row.get(c))
            .cloned()
            .unwrap_or_default();
        changes.push(json!({ "cell": cell, "before": before, "after": after }));
    }
    Ok(json!({
        "kind": "updateCells",
        "path": sanitize_filename(&filename)?,
        "sheet": sheet,
        "changes": changes,
    }))
}

async fn do_update_cells(
    state: &AppState,
    wid: Uuid,
    uploaded_by: Uuid,
    input: &Value,
) -> ApiResult<String> {
    let filename = str_in(input, "filename");
    let fname = sanitize_filename(&filename)?;
    let sheet_in = input.get("sheet").and_then(|s| s.as_str());
    let (sheet, mut grid) = read_xlsx_grid(state, wid, &fname, sheet_in)?;
    let changes = changes_from_input(input);
    for (cell, val) in &changes {
        let (r, c) = parse_cell_ref(cell)
            .ok_or_else(|| ApiError::BadRequest(format!("Ungültige Zelle: {cell}")))?;
        set_cell(&mut grid, r, c, val);
    }
    let bytes = build_xlsx_bytes(&sheet, &grid)?;
    save_xlsx(state, wid, uploaded_by, &fname, &bytes).await?;
    Ok(format!(
        "{} Zelle(n) in '{fname}' geändert (Sheet '{sheet}').",
        changes.len()
    ))
}

// --- Delegation / Bulk-Worker (Phase 6b-2g) -------------------------------

fn col_letter(mut idx: usize) -> String {
    let mut s = String::new();
    idx += 1;
    while idx > 0 {
        let r = (idx - 1) % 26;
        s.insert(0, (b'A' + r as u8) as char);
        idx = (idx - 1) / 26;
    }
    s
}

fn letters_to_index(s: &str) -> Option<usize> {
    if s.is_empty() || !s.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let mut col = 0usize;
    for ch in s.chars() {
        col = col * 26 + (ch.to_ascii_uppercase() as usize - 'A' as usize + 1);
    }
    Some(col - 1)
}

pub struct DelegatePlan {
    pub filename: String,
    pub sheet: String,
    pub grid: Vec<Vec<String>>,
    pub headers: Vec<String>,
    pub target_col: usize,
    pub target_header: String,
    pub creates_col: bool,
    pub prompt_template: String,
    /// Grid-Zeilenindizes der Datenzeilen (ohne Kopfzeile).
    pub data_rows: Vec<usize>,
}

pub fn delegate_plan(state: &AppState, wid: Uuid, input: &Value) -> ApiResult<DelegatePlan> {
    let filename = sanitize_filename(&str_in(input, "filename"))?;
    let sheet_in = input.get("sheet").and_then(|s| s.as_str());
    let prompt_template = str_in(input, "promptTemplate");
    let target_spec = str_in(input, "targetColumn");
    if prompt_template.trim().is_empty() || target_spec.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "promptTemplate und targetColumn erforderlich.".into(),
        ));
    }
    let (sheet, grid) = read_xlsx_grid(state, wid, &filename, sheet_in)?;
    let headers: Vec<String> = grid.first().cloned().unwrap_or_default();

    // Zielspalte: Buchstabe → Index; sonst Header-Treffer; sonst neue Spalte.
    let (target_col, target_header, creates_col) =
        if let Some(idx) = letters_to_index(target_spec.trim()) {
            let h = headers
                .get(idx)
                .cloned()
                .unwrap_or_else(|| target_spec.clone());
            (idx, h, idx >= headers.len())
        } else if let Some(idx) = headers
            .iter()
            .position(|h| h.trim().eq_ignore_ascii_case(target_spec.trim()))
        {
            (idx, headers[idx].clone(), false)
        } else {
            (headers.len().max(1), target_spec.clone(), true)
        };

    let data_rows: Vec<usize> = (1..grid.len())
        .filter(|&r| grid[r].iter().any(|c| !c.trim().is_empty()))
        .collect();
    if data_rows.len() > DELEGATE_MAX_ROWS {
        return Err(ApiError::BadRequest(format!(
            "Zu viele Zeilen ({} > {DELEGATE_MAX_ROWS}). Bitte eingrenzen.",
            data_rows.len()
        )));
    }
    Ok(DelegatePlan {
        filename,
        sheet,
        grid,
        headers,
        target_col,
        target_header,
        creates_col,
        prompt_template,
        data_rows,
    })
}

/// Rendert das Prompt-Template für eine konkrete Datenzeile.
pub fn render_prompt(plan: &DelegatePlan, row: usize) -> String {
    let empty = String::new();
    let cells = plan.grid.get(row).unwrap_or(&plan.grid[0]);
    let mut out = plan.prompt_template.clone();
    for c in 0..plan.headers.len().max(cells.len()) {
        let val = cells.get(c).unwrap_or(&empty);
        if let Some(h) = plan.headers.get(c) {
            if !h.trim().is_empty() {
                out = out.replace(&format!("{{{{{}}}}}", h.trim()), val);
            }
        }
        out = out.replace(&format!("{{{{{}}}}}", col_letter(c)), val);
    }
    out
}

pub fn delegate_preview_json(plan: &DelegatePlan, worker_label: &str) -> Value {
    let samples: Vec<Value> = plan
        .data_rows
        .iter()
        .take(3)
        .map(|&r| {
            json!({
                "rowLabel": format!("Zeile {}", r + 1),
                "renderedPrompt": render_prompt(plan, r)
            })
        })
        .collect();
    json!({
        "kind": "delegateIntoXlsxColumn",
        "path": plan.filename,
        "sheet": plan.sheet,
        "targetColumn": col_letter(plan.target_col),
        "targetCreatesColumn": plan.creates_col,
        "rowCount": plan.data_rows.len(),
        "workerLabel": worker_label,
        "samplePrompts": samples,
    })
}

/// Schreibt die Worker-Ergebnisse in die Zielspalte und speichert.
pub async fn save_delegation(
    state: &AppState,
    wid: Uuid,
    uploaded_by: Uuid,
    plan: &DelegatePlan,
    results: &[(usize, String)],
) -> ApiResult<String> {
    let mut grid = plan.grid.clone();
    if plan.creates_col || plan.headers.get(plan.target_col).is_none() {
        set_cell(&mut grid, 0, plan.target_col, &plan.target_header);
    }
    for (row, val) in results {
        set_cell(&mut grid, *row, plan.target_col, val);
    }
    let bytes = build_xlsx_bytes(&plan.sheet, &grid)?;
    save_xlsx(state, wid, uploaded_by, &plan.filename, &bytes).await?;
    Ok(format!(
        "{} Zeile(n) verarbeitet, Spalte '{}' in '{}' geschrieben.",
        results.len(),
        col_letter(plan.target_col),
        plan.filename
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

/// Packt eine vorhandene .docx neu und ersetzt dabei nur
/// `word/document.xml` (alle anderen Teile verbatim → Formatierung bleibt).
fn repack_docx(orig: &[u8], new_document_xml: &str) -> ApiResult<Vec<u8>> {
    use std::io::{Read, Write};
    let mut zin = zip::ZipArchive::new(std::io::Cursor::new(orig))
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
            new_document_xml.as_bytes().to_vec()
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

fn fill_template(template_bytes: &[u8], reps: &[(String, String)]) -> ApiResult<Vec<u8>> {
    // Vorlagen-document.xml lesen, Platzhalter ersetzen, Zip neu packen.
    let (_, doc) = {
        use std::io::Read;
        let mut zin = zip::ZipArchive::new(std::io::Cursor::new(template_bytes))
            .map_err(|e| ApiError::BadRequest(format!("Keine gültige .docx: {e}")))?;
        let mut s = String::new();
        zin.by_name("word/document.xml")
            .map_err(|_| ApiError::BadRequest("Vorlage ohne word/document.xml.".into()))?
            .read_to_string(&mut s)
            .map_err(|e| ApiError::Internal(anyhow::anyhow!("{e}")))?;
        ((), s)
    };
    let mut filled = doc;
    for (k, v) in reps {
        filled = filled.replace(&format!("{{{{{k}}}}}"), &xml_escape(v));
    }
    repack_docx(template_bytes, &filled)
}

/// Absätze-XML vor `<w:sectPr` bzw. `</w:body>` einfügen.
fn insert_paragraphs(doc: &str, paragraphs: &[String]) -> String {
    let paras: String = paragraphs
        .iter()
        .map(|p| {
            format!(
                "<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
                xml_escape(p)
            )
        })
        .collect();
    if let Some(pos) = doc.find("<w:sectPr") {
        format!("{}{}{}", &doc[..pos], paras, &doc[pos..])
    } else if let Some(pos) = doc.rfind("</w:body>") {
        format!("{}{}{}", &doc[..pos], paras, &doc[pos..])
    } else {
        format!("{doc}{paras}")
    }
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

// --- An vorhandene .docx anhängen (Phase 6b-2e) ---------------------------

/// Liest `word/document.xml` einer Workspace-.docx, falls vorhanden.
fn read_docx_doc_opt(
    state: &AppState,
    wid: Uuid,
    filename: &str,
) -> ApiResult<Option<(Vec<u8>, String)>> {
    use std::io::Read;
    let fname = sanitize_filename(filename)?;
    let key = workspace_key(wid, &fname);
    ensure_in_workspace(wid, &key)?;
    let Ok(bytes) = std::fs::read(state.storage.path(&key)) else {
        return Ok(None);
    };
    let mut zin = zip::ZipArchive::new(std::io::Cursor::new(&bytes))
        .map_err(|e| ApiError::BadRequest(format!("Keine gültige .docx: {e}")))?;
    let mut s = String::new();
    zin.by_name("word/document.xml")
        .map_err(|_| ApiError::BadRequest("Datei ohne word/document.xml.".into()))?
        .read_to_string(&mut s)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("{e}")))?;
    Ok(Some((bytes, s)))
}

/// Sichtbarer Text aus document.xml (für die Tail-Vorschau).
fn docx_text(doc: &str) -> String {
    let mut out = String::new();
    let mut rest = doc;
    while let Some(start) = rest.find("<w:t") {
        let after = &rest[start..];
        let Some(gt) = after.find('>') else { break };
        let body = &after[gt + 1..];
        let Some(end) = body.find("</w:t>") else {
            break;
        };
        out.push_str(
            &body[..end]
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&amp;", "&"),
        );
        out.push(' ');
        rest = &body[end + 6..];
    }
    out
}

fn appenddocx_preview(
    state: &AppState,
    wid: Uuid,
    filename: &str,
    paragraphs: &[String],
) -> ApiResult<Value> {
    let fname = sanitize_filename(filename)?;
    let existing = read_docx_doc_opt(state, wid, &fname)?;
    let tail = existing.as_ref().map(|(_, d)| {
        let t = docx_text(d);
        let start = t.len().saturating_sub(400);
        t[start..].to_string()
    });
    let mut preview_text = paragraphs.join("\n");
    if preview_text.len() > 800 {
        preview_text.truncate(800);
        preview_text.push('…');
    }
    Ok(json!({
        "kind": "appendToDocx",
        "path": fname,
        "blockCount": paragraphs.len(),
        "previewText": preview_text,
        "createsFile": existing.is_none(),
        "existingTail": tail,
    }))
}

async fn do_append_docx(
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
    let bytes = match read_docx_doc_opt(state, wid, &fname)? {
        Some((orig, doc)) => repack_docx(&orig, &insert_paragraphs(&doc, paragraphs))?,
        None => build_docx(paragraphs)?,
    };

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
        "{} Absätze an '{fname}' angehängt.",
        paragraphs.len()
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
        APPEND_DOCX_TOOL => appenddocx_preview(
            state,
            wid,
            &str_in(input, "filename"),
            &paras_from_input(input),
        ),
        UPDATE_CELLS_TOOL => updatecells_preview(state, wid, input),
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
        APPEND_DOCX_TOOL => {
            do_append_docx(
                state,
                wid,
                uploaded_by,
                &str_in(input, "filename"),
                &paras_from_input(input),
            )
            .await
        }
        UPDATE_CELLS_TOOL => do_update_cells(state, wid, uploaded_by, input).await,
        other => Err(ApiError::BadRequest(format!("Kein Write-Tool: {other}"))),
    }
}

#[cfg(test)]
mod pdf_fixture_tests {
    //! Roundtrip-Smoketest für die hand-gebaute Mini-PDF, die in den
    //! Integrationstests (`tests/integration.rs::make_minimal_pdf`) erzeugt
    //! wird. Hält die Konstruktion ohne Postgres ehrlich.

    fn minimal_pdf(text: &str) -> Vec<u8> {
        let stream = format!("BT /F1 24 Tf 72 720 Td ({text}) Tj ET\n");
        let stream_bytes = stream.as_bytes();
        let mut out = Vec::<u8>::new();
        let mut offsets = [0usize; 6];
        out.extend_from_slice(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n");
        offsets[1] = out.len();
        out.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
        offsets[2] = out.len();
        out.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");
        offsets[3] = out.len();
        out.extend_from_slice(
            b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
              /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>\nendobj\n",
        );
        offsets[4] = out.len();
        out.extend_from_slice(
            b"4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n",
        );
        offsets[5] = out.len();
        let header = format!("5 0 obj\n<< /Length {} >>\nstream\n", stream_bytes.len());
        out.extend_from_slice(header.as_bytes());
        out.extend_from_slice(stream_bytes);
        out.extend_from_slice(b"endstream\nendobj\n");
        let xref_offset = out.len();
        out.extend_from_slice(b"xref\n0 6\n");
        out.extend_from_slice(b"0000000000 65535 f \n");
        for off in offsets.iter().skip(1) {
            out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n")
                .as_bytes(),
        );
        out
    }

    #[test]
    fn extract_roundtrips_known_text() {
        let bytes = minimal_pdf("ProcessFox PDF Test");
        let text =
            pdf_extract::extract_text_from_mem(&bytes).expect("hand-gebauter PDF muss parsen");
        assert!(text.contains("ProcessFox PDF Test"), "extrahiert: {text:?}");
    }
}
