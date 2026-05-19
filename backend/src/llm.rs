//! LLM-Provider mit Streaming (Phase 6a, CLAUDE.md §10). Anthropic
//! (Messages-API, Prompt-Caching) und OpenAI-kompatibel (OpenAI,
//! OpenRouter). Key wird serverseitig injiziert, nie ans Frontend.

use anyhow::{anyhow, bail, Context, Result};
use futures_util::StreamExt;
use serde_json::{json, Value};

pub struct ChatMsg {
    /// `user` | `assistant`
    pub role: String,
    pub content: String,
}

const MAX_TOKENS: u32 = 4096;

/// Streamt die Assistenten-Antwort. `on_delta` wird je Text-Chunk
/// aufgerufen und gibt `false` zurück, um den Stream abzubrechen
/// (Cancel). Der bis dahin akkumulierte Text wird zurückgegeben.
pub async fn stream_chat(
    http: &reqwest::Client,
    provider: &str,
    api_key: &str,
    model: &str,
    system: &str,
    history: &[ChatMsg],
    mut on_delta: impl FnMut(&str) -> bool,
) -> Result<String> {
    match provider {
        "anthropic" => anthropic(http, api_key, model, system, history, &mut on_delta).await,
        "openai" | "openrouter" => {
            let base = if provider == "openai" {
                "https://api.openai.com/v1"
            } else {
                "https://openrouter.ai/api/v1"
            };
            openai_compat(http, base, api_key, model, system, history, &mut on_delta).await
        }
        other => bail!("Unbekannter Provider: {other}"),
    }
}

/// Liest eine SSE-Antwort zeilenweise und ruft `on_event` je `data:`-JSON.
/// `on_event` gibt `true` zurück, wenn der Stream beendet ist.
async fn read_sse(
    resp: reqwest::Response,
    mut on_event: impl FnMut(&str) -> Result<bool>,
) -> Result<()> {
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        bail!("Provider-Fehler HTTP {}: {body}", status.as_u16());
    }
    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.context("Stream abgebrochen")?;
        buf.push_str(&String::from_utf8_lossy(&bytes));
        while let Some(nl) = buf.find('\n') {
            let line = buf[..nl].trim().to_string();
            buf.drain(..=nl);
            if let Some(data) = line.strip_prefix("data:") {
                let data = data.trim();
                if data == "[DONE]" {
                    return Ok(());
                }
                if !data.is_empty() && on_event(data)? {
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}

async fn anthropic(
    http: &reqwest::Client,
    api_key: &str,
    model: &str,
    system: &str,
    history: &[ChatMsg],
    on_delta: &mut impl FnMut(&str) -> bool,
) -> Result<String> {
    let messages: Vec<Value> = history
        .iter()
        .map(|m| json!({ "role": m.role, "content": m.content }))
        .collect();
    // System als Block mit cache_control = Prompt-Caching (CLAUDE.md §10).
    let body = json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "stream": true,
        "system": [{
            "type": "text",
            "text": system,
            "cache_control": { "type": "ephemeral" }
        }],
        "messages": messages,
    });
    let resp = http
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await
        .context("Anthropic-Request fehlgeschlagen")?;

    let mut full = String::new();
    read_sse(resp, |data| {
        let v: Value = serde_json::from_str(data).map_err(|e| anyhow!("SSE-JSON: {e}"))?;
        match v.get("type").and_then(|t| t.as_str()) {
            Some("content_block_delta") => {
                if let Some(t) = v.pointer("/delta/text").and_then(|t| t.as_str()) {
                    full.push_str(t);
                    if !on_delta(t) {
                        return Ok(true); // Abbruch (Cancel)
                    }
                }
                Ok(false)
            }
            Some("message_stop") => Ok(true),
            Some("error") => {
                let msg = v
                    .pointer("/error/message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unbekannt");
                bail!("Anthropic: {msg}")
            }
            _ => Ok(false),
        }
    })
    .await?;
    Ok(full)
}

async fn openai_compat(
    http: &reqwest::Client,
    base: &str,
    api_key: &str,
    model: &str,
    system: &str,
    history: &[ChatMsg],
    on_delta: &mut impl FnMut(&str) -> bool,
) -> Result<String> {
    let mut messages = vec![json!({ "role": "system", "content": system })];
    for m in history {
        messages.push(json!({ "role": m.role, "content": m.content }));
    }
    let body = json!({
        "model": model,
        "stream": true,
        "messages": messages,
    });
    let resp = http
        .post(format!("{base}/chat/completions"))
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .context("LLM-Request fehlgeschlagen")?;

    let mut full = String::new();
    read_sse(resp, |data| {
        let v: Value = serde_json::from_str(data).map_err(|e| anyhow!("SSE-JSON: {e}"))?;
        if let Some(t) = v
            .pointer("/choices/0/delta/content")
            .and_then(|c| c.as_str())
        {
            full.push_str(t);
            if !on_delta(t) {
                return Ok(true); // Abbruch (Cancel)
            }
        }
        Ok(false)
    })
    .await?;
    Ok(full)
}

// =========================================================================
// Tool-Calling (Phase 6b-1) — non-streaming Einzelschritt im ReAct-Loop.
// =========================================================================

use crate::tools::ToolSpec;

#[derive(Clone)]
pub struct ToolUse {
    pub id: String,
    pub name: String,
    pub input: Value,
}

pub struct ToolResult {
    pub id: String,
    pub content: String,
}

pub enum Turn {
    User(String),
    Assistant(String),
    ToolUse(Vec<ToolUse>),
    ToolResults(Vec<ToolResult>),
}

pub struct Step {
    /// Finaler Text (wenn keine Tools aufgerufen werden).
    pub text: Option<String>,
    /// Vom Modell angeforderte Tool-Aufrufe.
    pub calls: Vec<ToolUse>,
}

/// Ein Schritt im Tool-Loop (kein Streaming): Text (fertig) oder Calls.
pub async fn tool_step(
    http: &reqwest::Client,
    provider: &str,
    api_key: &str,
    model: &str,
    system: &str,
    turns: &[Turn],
    tools: &[ToolSpec],
) -> Result<Step> {
    match provider {
        "anthropic" => anthropic_step(http, api_key, model, system, turns, tools).await,
        "openai" | "openrouter" => {
            let base = if provider == "openai" {
                "https://api.openai.com/v1"
            } else {
                "https://openrouter.ai/api/v1"
            };
            openai_step(http, base, api_key, model, system, turns, tools).await
        }
        other => bail!("Unbekannter Provider: {other}"),
    }
}

async fn anthropic_step(
    http: &reqwest::Client,
    api_key: &str,
    model: &str,
    system: &str,
    turns: &[Turn],
    tools: &[ToolSpec],
) -> Result<Step> {
    let messages: Vec<Value> = turns
        .iter()
        .map(|t| match t {
            Turn::User(s) => json!({ "role": "user", "content": s }),
            Turn::Assistant(s) => {
                json!({ "role": "assistant", "content": s })
            }
            Turn::ToolUse(uses) => json!({
                "role": "assistant",
                "content": uses.iter().map(|u| json!({
                    "type": "tool_use", "id": u.id,
                    "name": u.name, "input": u.input
                })).collect::<Vec<_>>()
            }),
            Turn::ToolResults(rs) => json!({
                "role": "user",
                "content": rs.iter().map(|r| json!({
                    "type": "tool_result",
                    "tool_use_id": r.id, "content": r.content
                })).collect::<Vec<_>>()
            }),
        })
        .collect();
    let tool_defs: Vec<Value> = tools
        .iter()
        .map(|t| {
            json!({ "name": t.name, "description": t.description,
                    "input_schema": t.schema })
        })
        .collect();
    let body = json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "system": [{ "type": "text", "text": system,
                     "cache_control": { "type": "ephemeral" } }],
        "tools": tool_defs,
        "messages": messages,
    });
    let v: Value = http
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await
        .context("Anthropic-Request fehlgeschlagen")?
        .json()
        .await
        .context("Anthropic-Antwort ungültig")?;
    if let Some(msg) = v.pointer("/error/message").and_then(|m| m.as_str()) {
        bail!("Anthropic: {msg}");
    }
    let mut text = String::new();
    let mut calls = Vec::new();
    if let Some(arr) = v.get("content").and_then(|c| c.as_array()) {
        for block in arr {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    if let Some(s) = block.get("text").and_then(|s| s.as_str()) {
                        text.push_str(s);
                    }
                }
                Some("tool_use") => calls.push(ToolUse {
                    id: block
                        .get("id")
                        .and_then(|i| i.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    name: block
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    input: block.get("input").cloned().unwrap_or(json!({})),
                }),
                _ => {}
            }
        }
    }
    Ok(Step {
        text: if calls.is_empty() { Some(text) } else { None },
        calls,
    })
}

async fn openai_step(
    http: &reqwest::Client,
    base: &str,
    api_key: &str,
    model: &str,
    system: &str,
    turns: &[Turn],
    tools: &[ToolSpec],
) -> Result<Step> {
    let mut messages = vec![json!({ "role": "system", "content": system })];
    for t in turns {
        match t {
            Turn::User(s) => messages.push(json!({ "role": "user", "content": s })),
            Turn::Assistant(s) => messages.push(json!({ "role": "assistant", "content": s })),
            Turn::ToolUse(uses) => messages.push(json!({
                "role": "assistant",
                "content": null,
                "tool_calls": uses.iter().map(|u| json!({
                    "id": u.id, "type": "function",
                    "function": {
                        "name": u.name,
                        "arguments": u.input.to_string()
                    }
                })).collect::<Vec<_>>()
            })),
            Turn::ToolResults(rs) => {
                for r in rs {
                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": r.id,
                        "content": r.content
                    }));
                }
            }
        }
    }
    let tool_defs: Vec<Value> = tools
        .iter()
        .map(|t| {
            json!({ "type": "function", "function": {
                "name": t.name, "description": t.description,
                "parameters": t.schema } })
        })
        .collect();
    let body = json!({
        "model": model, "messages": messages, "tools": tool_defs,
    });
    let v: Value = http
        .post(format!("{base}/chat/completions"))
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .context("LLM-Request fehlgeschlagen")?
        .json()
        .await
        .context("LLM-Antwort ungültig")?;
    if let Some(msg) = v.pointer("/error/message").and_then(|m| m.as_str()) {
        bail!("LLM: {msg}");
    }
    let msg = v
        .pointer("/choices/0/message")
        .cloned()
        .unwrap_or(json!({}));
    let mut calls = Vec::new();
    if let Some(tcs) = msg.get("tool_calls").and_then(|t| t.as_array()) {
        for tc in tcs {
            let args = tc
                .pointer("/function/arguments")
                .and_then(|a| a.as_str())
                .unwrap_or("{}");
            calls.push(ToolUse {
                id: tc
                    .get("id")
                    .and_then(|i| i.as_str())
                    .unwrap_or_default()
                    .to_string(),
                name: tc
                    .pointer("/function/name")
                    .and_then(|n| n.as_str())
                    .unwrap_or_default()
                    .to_string(),
                input: serde_json::from_str(args).unwrap_or(json!({})),
            });
        }
    }
    let text = msg
        .get("content")
        .and_then(|c| c.as_str())
        .unwrap_or_default()
        .to_string();
    Ok(Step {
        text: if calls.is_empty() { Some(text) } else { None },
        calls,
    })
}
