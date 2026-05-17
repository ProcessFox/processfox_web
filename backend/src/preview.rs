//! Server-seitige Office-Vorschau (PLAN.md Phase 5). Liefert exakt die
//! JSON-Strukturen, die das Frontend schon kennt:
//! - docx → `{ html }` (Text-only, sicher escaped)
//! - xlsx → Sheet-Grid (max. 1000×50)
//! - pptx → Folien-Outline (Titel + Bullets)

use std::io::Cursor;

use anyhow::{Context, Result};
use calamine::{Data, Reader, Xlsx};
use quick_xml::events::Event;
use quick_xml::Reader as XmlReader;
use serde_json::{json, Value};
use zip::ZipArchive;

const MAX_ROWS: usize = 1000;
const MAX_COLS: usize = 50;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Extrahiert Text aus allen Elementen mit lokalem Namen `local` (z. B.
/// `w:t` für docx, `a:t` für pptx) in Dokument-Reihenfolge.
fn extract_texts(xml: &[u8], local: &str) -> Vec<String> {
    let mut reader = XmlReader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut out = Vec::new();
    let mut capture = false;
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if local_name(e.name().as_ref()) == local => {
                capture = true;
            }
            Ok(Event::End(e)) if local_name(e.name().as_ref()) == local => {
                capture = false;
            }
            Ok(Event::Text(t)) if capture => {
                out.push(t.unescape().unwrap_or_default().into_owned());
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}

fn local_name(qname: &[u8]) -> String {
    let s = String::from_utf8_lossy(qname);
    s.rsplit(':').next().unwrap_or(&s).to_string()
}

pub fn docx_html(bytes: &[u8]) -> Result<Value> {
    let mut zip = ZipArchive::new(Cursor::new(bytes)).context("docx ist kein ZIP")?;
    let mut xml = String::new();
    {
        use std::io::Read;
        let mut f = zip
            .by_name("word/document.xml")
            .context("word/document.xml fehlt")?;
        f.read_to_string(&mut xml)?;
    }
    // Absatz = <w:p>; Text = <w:t>. Pro Absatz die enthaltenen w:t-Runs
    // zusammenfügen.
    let mut html = String::new();
    for para in split_on_paragraphs(xml.as_bytes()) {
        let text = extract_texts(&para, "t").join("");
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        html.push_str(&format!("<p>{}</p>", esc(trimmed)));
    }
    if html.is_empty() {
        html.push_str("<p><em>Kein Text gefunden.</em></p>");
    }
    Ok(json!({ "html": html }))
}

/// Zerlegt das document.xml grob an `<w:p ...>`-Grenzen, damit Absätze
/// erhalten bleiben.
fn split_on_paragraphs(xml: &[u8]) -> Vec<Vec<u8>> {
    let s = String::from_utf8_lossy(xml);
    let mut parts = Vec::new();
    let mut rest = s.as_ref();
    while let Some(idx) = rest.find("<w:p ").or_else(|| rest.find("<w:p>")) {
        let after = &rest[idx..];
        let end = after[1..]
            .find("<w:p ")
            .or_else(|| after[1..].find("<w:p>"))
            .map(|e| e + 1)
            .unwrap_or(after.len());
        parts.push(after.as_bytes()[..end].to_vec());
        rest = &after[end..];
    }
    if parts.is_empty() {
        parts.push(xml.to_vec());
    }
    parts
}

pub fn xlsx_preview(bytes: &[u8], sheet: Option<&str>) -> Result<Value> {
    let mut wb: Xlsx<_> = calamine::open_workbook_from_rs(Cursor::new(bytes))
        .context("xlsx konnte nicht gelesen werden")?;
    let sheets = wb.sheet_names().to_vec();
    if sheets.is_empty() {
        return Ok(json!({
            "sheets": [], "activeSheet": "", "rows": [],
            "totalRows": 0, "totalCols": 0, "truncated": false
        }));
    }
    let active = sheet
        .filter(|s| sheets.iter().any(|x| x == s))
        .map(|s| s.to_string())
        .unwrap_or_else(|| sheets[0].clone());
    let range = wb
        .worksheet_range(&active)
        .context("Sheet konnte nicht gelesen werden")?;
    let total_rows = range.height();
    let total_cols = range.width();
    let truncated = total_rows > MAX_ROWS || total_cols > MAX_COLS;

    let mut rows: Vec<Vec<String>> = Vec::new();
    for row in range.rows().take(MAX_ROWS) {
        rows.push(
            row.iter()
                .take(MAX_COLS)
                .map(|c| match c {
                    Data::Empty => String::new(),
                    other => other.to_string(),
                })
                .collect(),
        );
    }
    Ok(json!({
        "sheets": sheets,
        "activeSheet": active,
        "rows": rows,
        "totalRows": total_rows,
        "totalCols": total_cols,
        "truncated": truncated,
    }))
}

pub fn pptx_preview(bytes: &[u8]) -> Result<Value> {
    let mut zip = ZipArchive::new(Cursor::new(bytes)).context("pptx ist kein ZIP")?;
    // Slide-Dateien sammeln + numerisch sortieren.
    let mut slide_files: Vec<(u32, String)> = zip
        .file_names()
        .filter(|n| n.starts_with("ppt/slides/slide") && n.ends_with(".xml"))
        .filter_map(|n| {
            n.trim_start_matches("ppt/slides/slide")
                .trim_end_matches(".xml")
                .parse::<u32>()
                .ok()
                .map(|num| (num, n.to_string()))
        })
        .collect();
    slide_files.sort_by_key(|(n, _)| *n);

    let mut slides = Vec::new();
    for (idx, (_num, name)) in slide_files.iter().enumerate() {
        use std::io::Read;
        let mut xml = Vec::new();
        if let Ok(mut f) = zip.by_name(name) {
            let _ = f.read_to_end(&mut xml);
        }
        let texts: Vec<String> = extract_texts(&xml, "t")
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let (title, body) = match texts.split_first() {
            Some((t, rest)) => (Some(t.clone()), rest.to_vec()),
            None => (None, Vec::new()),
        };
        slides.push(json!({
            "index": idx + 1,
            "title": title,
            "body": body,
            "notes": Vec::<String>::new(),
        }));
    }
    Ok(json!({ "slides": slides }))
}
