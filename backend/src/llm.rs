//! LLM-Provider mit Streaming (Phase 6a, CLAUDE.md §10). Anthropic
//! (Messages-API, Prompt-Caching) und OpenAI-kompatibel (OpenAI,
//! OpenRouter). Key wird serverseitig injiziert, nie ans Frontend.
//!
//! Phase 6d-2: optionaler Reasoning-/Extended-Thinking-Pfad — pro Agent
//! per `LlmOptions { reasoning_enabled }` aktivierbar. Provider-spezifisch:
//! Anthropic schaltet auf das `thinking`-Feld der Messages-API um und
//! parst `thinking_delta`-SSE-Events; OpenAI-compat (OpenRouter)
//! schaltet das `reasoning`-Feld nur dann zu, wenn das Modell ein
//! bekanntes Reasoning-Pattern matched — sonst lehnen einige OR-Routes
//! das unbekannte Feld mit 400 ab.

use anyhow::{anyhow, bail, Context, Result};
use futures_util::StreamExt;
use serde_json::{json, Value};

pub struct ChatMsg {
    /// `user` | `assistant`
    pub role: String,
    pub content: String,
}

const MAX_TOKENS: u32 = 4096;
/// Anthropic Extended Thinking: separater Token-Budget für `thinking`-Blöcke.
/// Konservativ klein gehalten — der Caller (`reasoning_enabled`-Toggle)
/// entscheidet, ob die Mehrkosten überhaupt fallen.
const ANTHROPIC_THINKING_BUDGET_TOKENS: u32 = 4000;
/// OpenAI/OpenRouter Reasoning-Effort. Wird nur bei Modellen aus der
/// `MODELS_WITH_REASONING`-Liste mitgesendet (s. `model_supports_or_reasoning`).
const OPENAI_REASONING_EFFORT: &str = "medium";

/// Optionen, die orthogonal zu `provider`/`model` durch alle LLM-Pfade
/// fließen. Heute nur Reasoning — bewusst eigener Struct, damit künftige
/// Schalter (`max_tokens`-Override, `temperature`, …) keine
/// Signatur-Aufweitung erzwingen.
#[derive(Clone, Copy, Default)]
pub struct LlmOptions {
    /// Per Agent (`agents.reasoning_enabled`, Phase 6d-2). Aktiviert
    /// Anthropic Extended Thinking bzw. OpenAI-compat-`reasoning`-Feld,
    /// **sofern** das Modell-Pattern es unterstützt (sonst no-op, kein
    /// Request-Fehler).
    pub reasoning_enabled: bool,
}

/// Ergebnis eines erfolgreich beendeten Streams: akkumulierter
/// Antworttext **und** akkumuliertes Reasoning (leer, wenn nichts
/// gestreamt wurde — beim derzeitigen Frontend-Vertrag äquivalent zu
/// „kein Chain-of-Thought").
pub struct StreamOutcome {
    pub text: String,
    pub reasoning: String,
}

/// Erkennt OpenRouter-/OpenAI-Modelle, die das `reasoning`-Feld
/// akzeptieren. Falsch-negativ ist okay (Reasoning kommt halt nicht),
/// falsch-positiv wäre schlimmer (manche Provider lehnen unbekannte
/// Felder mit 400 ab). Liste konservativ halten und bei Bedarf
/// erweitern — Quelle: https://openrouter.ai/models
fn model_supports_or_reasoning(model: &str) -> bool {
    let m = model.to_lowercase();
    // OpenAI o-Serien
    m.starts_with("o1")
        || m.starts_with("o3")
        || m.starts_with("o4")
        // Anbieter-spezifische Reasoning-Modelle, die über OpenRouter
        // (oder direkt) erreichbar sind
        || (m.contains("deepseek") && m.contains("r1"))
        || (m.contains("qwen") && m.contains("qwq"))
        || m.contains("thinking")
        || (m.contains("grok") && m.contains("reasoning"))
}

/// Heuristik für Anthropic Extended Thinking — bewusst minimal: alle
/// Modelle, die das `thinking`-Feld als legalen Body-Parameter
/// akzeptieren (Claude 4-Familie und neuer). Bei `claude-3-*` lehnt die
/// API das Feld mit 400 ab, also nur für 4+ einschalten.
fn model_supports_anthropic_thinking(model: &str) -> bool {
    let m = model.to_lowercase();
    if m.starts_with("claude-3-") || m == "claude-3" {
        return false;
    }
    m.starts_with("claude-")
}

/// Streamt die Assistenten-Antwort. `on_delta` läuft pro Text-Chunk,
/// `on_reasoning` pro Reasoning-/Thinking-Chunk (bei aktiviertem
/// Toggle); beide Closures geben `false` zurück, um den Stream
/// abzubrechen (Cancel). Rückgabe: vollständiger Text **und**
/// vollständiges Reasoning (jeweils evtl. leer).
#[allow(clippy::too_many_arguments)]
pub async fn stream_chat(
    http: &reqwest::Client,
    provider: &str,
    api_key: &str,
    model: &str,
    system: &str,
    history: &[ChatMsg],
    options: LlmOptions,
    mut on_delta: impl FnMut(&str) -> bool,
    mut on_reasoning: impl FnMut(&str) -> bool,
) -> Result<StreamOutcome> {
    match provider {
        "anthropic" => {
            anthropic(
                http,
                api_key,
                model,
                system,
                history,
                options,
                &mut on_delta,
                &mut on_reasoning,
            )
            .await
        }
        "openai" | "openrouter" => {
            let base = if provider == "openai" {
                "https://api.openai.com/v1"
            } else {
                "https://openrouter.ai/api/v1"
            };
            openai_compat(
                http,
                base,
                api_key,
                model,
                system,
                history,
                options,
                &mut on_delta,
                &mut on_reasoning,
            )
            .await
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

/// Baut den Anthropic-Messages-Request-Body. Wird vom Streaming- und
/// vom Tool-Step-Pfad geteilt, damit der `thinking`-Schalter genau eine
/// Stelle hat (und die Unit-Tests den Body inspizieren können).
pub(crate) fn build_anthropic_body(
    model: &str,
    system: &str,
    messages: Vec<Value>,
    tools: Option<&[Value]>,
    options: LlmOptions,
    stream: bool,
) -> Value {
    let mut body = json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "system": [{
            "type": "text",
            "text": system,
            "cache_control": { "type": "ephemeral" }
        }],
        "messages": messages,
    });
    if stream {
        body["stream"] = Value::Bool(true);
    }
    if let Some(t) = tools {
        body["tools"] = Value::Array(t.to_vec());
    }
    if options.reasoning_enabled && model_supports_anthropic_thinking(model) {
        body["thinking"] = json!({
            "type": "enabled",
            "budget_tokens": ANTHROPIC_THINKING_BUDGET_TOKENS,
        });
    }
    body
}

#[allow(clippy::too_many_arguments)]
async fn anthropic(
    http: &reqwest::Client,
    api_key: &str,
    model: &str,
    system: &str,
    history: &[ChatMsg],
    options: LlmOptions,
    on_delta: &mut impl FnMut(&str) -> bool,
    on_reasoning: &mut impl FnMut(&str) -> bool,
) -> Result<StreamOutcome> {
    let messages: Vec<Value> = history
        .iter()
        .map(|m| json!({ "role": m.role, "content": m.content }))
        .collect();
    let body = build_anthropic_body(model, system, messages, None, options, true);
    let resp = http
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await
        .context("Anthropic-Request fehlgeschlagen")?;

    let mut text = String::new();
    let mut reasoning = String::new();
    read_sse(resp, |data| {
        let v: Value = serde_json::from_str(data).map_err(|e| anyhow!("SSE-JSON: {e}"))?;
        match classify_anthropic_event(&v) {
            AnthropicChunk::Text(t) => {
                text.push_str(&t);
                if !on_delta(&t) {
                    return Ok(true);
                }
                Ok(false)
            }
            AnthropicChunk::Reasoning(t) => {
                reasoning.push_str(&t);
                if !on_reasoning(&t) {
                    return Ok(true);
                }
                Ok(false)
            }
            AnthropicChunk::Stop => Ok(true),
            AnthropicChunk::Error(msg) => bail!("Anthropic: {msg}"),
            AnthropicChunk::Other => Ok(false),
        }
    })
    .await?;
    Ok(StreamOutcome { text, reasoning })
}

/// Pure Klassifikation eines Anthropic-SSE-Events. Trennt das Parsen
/// von I/O und Callbacks, damit der Streaming-Pfad ohne HTTP-Mock
/// unit-testbar bleibt (Phase 6d-2).
#[derive(Debug, PartialEq)]
pub(crate) enum AnthropicChunk {
    /// Text-Delta einer regulären Antwort.
    Text(String),
    /// Reasoning-Delta (Extended Thinking).
    Reasoning(String),
    /// Stream-Ende (`message_stop`).
    Stop,
    /// Server-Fehler — Message für die Fehlermeldung.
    Error(String),
    /// Andere Event-Typen, die wir ignorieren (`message_start`,
    /// `content_block_start`, Ping etc.).
    Other,
}

pub(crate) fn classify_anthropic_event(v: &Value) -> AnthropicChunk {
    match v.get("type").and_then(|t| t.as_str()) {
        Some("content_block_delta") => {
            let delta_type = v.pointer("/delta/type").and_then(|t| t.as_str());
            match delta_type {
                Some("thinking_delta") => v
                    .pointer("/delta/thinking")
                    .and_then(|t| t.as_str())
                    .map(|s| AnthropicChunk::Reasoning(s.to_string()))
                    .unwrap_or(AnthropicChunk::Other),
                _ => v
                    .pointer("/delta/text")
                    .and_then(|t| t.as_str())
                    .map(|s| AnthropicChunk::Text(s.to_string()))
                    .unwrap_or(AnthropicChunk::Other),
            }
        }
        Some("message_stop") => AnthropicChunk::Stop,
        Some("error") => AnthropicChunk::Error(
            v.pointer("/error/message")
                .and_then(|m| m.as_str())
                .unwrap_or("unbekannt")
                .to_string(),
        ),
        _ => AnthropicChunk::Other,
    }
}

/// Baut den OpenAI-/OpenRouter-Chat-Completions-Request-Body. Geteilt
/// zwischen Streaming- und Tool-Step-Pfad, damit der `reasoning`-Schalter
/// einen Ort hat (und Tests den Body inspizieren können).
pub(crate) fn build_openai_body(
    model: &str,
    messages: Vec<Value>,
    tools: Option<&[Value]>,
    options: LlmOptions,
    stream: bool,
) -> Value {
    let mut body = json!({
        "model": model,
        "messages": messages,
    });
    if stream {
        body["stream"] = Value::Bool(true);
    }
    if let Some(t) = tools {
        body["tools"] = Value::Array(t.to_vec());
    }
    // Phase 6d-2: nur einsetzen, wenn der Toggle an ist **und** das Modell
    // das Feld kennt. Sonst lehnen einige OpenRouter-Routen den Body mit
    // 400 ab — Falsch-positive sind teurer als Falsch-negative.
    if options.reasoning_enabled && model_supports_or_reasoning(model) {
        body["reasoning"] = json!({ "effort": OPENAI_REASONING_EFFORT });
    }
    body
}

#[allow(clippy::too_many_arguments)]
async fn openai_compat(
    http: &reqwest::Client,
    base: &str,
    api_key: &str,
    model: &str,
    system: &str,
    history: &[ChatMsg],
    options: LlmOptions,
    on_delta: &mut impl FnMut(&str) -> bool,
    on_reasoning: &mut impl FnMut(&str) -> bool,
) -> Result<StreamOutcome> {
    let mut messages = vec![json!({ "role": "system", "content": system })];
    for m in history {
        messages.push(json!({ "role": m.role, "content": m.content }));
    }
    let body = build_openai_body(model, messages, None, options, true);
    let resp = http
        .post(format!("{base}/chat/completions"))
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .context("LLM-Request fehlgeschlagen")?;

    let mut text = String::new();
    let mut reasoning = String::new();
    read_sse(resp, |data| {
        let v: Value = serde_json::from_str(data).map_err(|e| anyhow!("SSE-JSON: {e}"))?;
        let chunk = classify_openai_event(&v);
        if let Some(t) = chunk.reasoning {
            reasoning.push_str(&t);
            if !on_reasoning(&t) {
                return Ok(true);
            }
        }
        if let Some(t) = chunk.text {
            text.push_str(&t);
            if !on_delta(&t) {
                return Ok(true);
            }
        }
        Ok(false)
    })
    .await?;
    Ok(StreamOutcome { text, reasoning })
}

/// Pure Klassifikation eines OpenAI/OpenRouter-SSE-Events. `text` und
/// `reasoning` können in **demselben** Event auftreten (z. B. wenn ein
/// Anbieter beide Felder im selben Delta-Frame liefert). Reasoning hat
/// zwei Feldnamen im Umlauf: `delta.reasoning` (OpenRouter-Standard)
/// und `delta.reasoning_content` (DeepSeek-Style, wird über einige
/// OR-Routen durchgereicht).
#[derive(Debug, Default, PartialEq)]
pub(crate) struct OpenAiChunk {
    pub text: Option<String>,
    pub reasoning: Option<String>,
}

pub(crate) fn classify_openai_event(v: &Value) -> OpenAiChunk {
    let mut chunk = OpenAiChunk::default();
    if let Some(t) = v
        .pointer("/choices/0/delta/reasoning")
        .and_then(|c| c.as_str())
    {
        chunk.reasoning = Some(t.to_string());
    } else if let Some(t) = v
        .pointer("/choices/0/delta/reasoning_content")
        .and_then(|c| c.as_str())
    {
        chunk.reasoning = Some(t.to_string());
    }
    if let Some(t) = v
        .pointer("/choices/0/delta/content")
        .and_then(|c| c.as_str())
    {
        chunk.text = Some(t.to_string());
    }
    chunk
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
    /// Chain-of-Thought / Reasoning für diesen Step (Phase 6d-2). Leer,
    /// wenn das Modell/`LlmOptions` kein Reasoning liefert.
    pub reasoning: String,
}

/// Ein Schritt im Tool-Loop (kein Streaming): Text (fertig) oder Calls.
#[allow(clippy::too_many_arguments)]
pub async fn tool_step(
    http: &reqwest::Client,
    provider: &str,
    api_key: &str,
    model: &str,
    system: &str,
    turns: &[Turn],
    tools: &[ToolSpec],
    options: LlmOptions,
) -> Result<Step> {
    match provider {
        "anthropic" => anthropic_step(http, api_key, model, system, turns, tools, options).await,
        "openai" | "openrouter" => {
            let base = if provider == "openai" {
                "https://api.openai.com/v1"
            } else {
                "https://openrouter.ai/api/v1"
            };
            openai_step(http, base, api_key, model, system, turns, tools, options).await
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
    options: LlmOptions,
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
    let body = build_anthropic_body(model, system, messages, Some(&tool_defs), options, false);
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
    let mut reasoning = String::new();
    let mut calls = Vec::new();
    if let Some(arr) = v.get("content").and_then(|c| c.as_array()) {
        for block in arr {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    if let Some(s) = block.get("text").and_then(|s| s.as_str()) {
                        text.push_str(s);
                    }
                }
                // Phase 6d-2: Anthropic liefert das CoT bei aktivem
                // `thinking`-Feld als eigenen Block-Typ. Wir konkatenieren
                // alle Blöcke — übliche Antwort hat genau einen.
                Some("thinking") => {
                    if let Some(s) = block.get("thinking").and_then(|s| s.as_str()) {
                        reasoning.push_str(s);
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
        reasoning,
    })
}

#[allow(clippy::too_many_arguments)]
async fn openai_step(
    http: &reqwest::Client,
    base: &str,
    api_key: &str,
    model: &str,
    system: &str,
    turns: &[Turn],
    tools: &[ToolSpec],
    options: LlmOptions,
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
    let body = build_openai_body(model, messages, Some(&tool_defs), options, false);
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
    // Phase 6d-2: Reasoning aus dem non-streaming-Response. OpenRouter
    // legt es als `message.reasoning` ab, einige DeepSeek-Routen als
    // `message.reasoning_content`. Beide Wege ohne weitere Schachtelung.
    let reasoning = msg
        .get("reasoning")
        .and_then(|c| c.as_str())
        .or_else(|| msg.get("reasoning_content").and_then(|c| c.as_str()))
        .unwrap_or_default()
        .to_string();
    Ok(Step {
        text: if calls.is_empty() { Some(text) } else { None },
        calls,
        reasoning,
    })
}

// =========================================================================
// Tests (Phase 6d-2). Reine Pur-Funktionen (Body-Builder + SSE-Klassifikator)
// — kein HTTP-Mock nötig.
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- Pattern-Matcher --------------------------------------------------

    #[test]
    fn or_pattern_matches_o_series() {
        assert!(model_supports_or_reasoning("o1"));
        assert!(model_supports_or_reasoning("o1-mini"));
        assert!(model_supports_or_reasoning("o3"));
        assert!(model_supports_or_reasoning("o4-mini"));
    }

    #[test]
    fn or_pattern_matches_deepseek_r1_and_qwen_qwq() {
        assert!(model_supports_or_reasoning("deepseek/deepseek-r1"));
        assert!(model_supports_or_reasoning("deepseek-r1-distill-llama"));
        assert!(model_supports_or_reasoning("qwen/qwq-32b-preview"));
    }

    #[test]
    fn or_pattern_ignores_gpt4o_and_unknown() {
        assert!(!model_supports_or_reasoning("gpt-4o"));
        assert!(!model_supports_or_reasoning("gpt-4o-mini"));
        assert!(!model_supports_or_reasoning("anthropic/claude-3-5-sonnet"));
        assert!(!model_supports_or_reasoning("meta/llama-3-70b"));
    }

    #[test]
    fn anthropic_thinking_only_on_4_plus() {
        // Claude 3 lehnt das `thinking`-Feld ab — wir senden es nicht.
        assert!(!model_supports_anthropic_thinking(
            "claude-3-5-sonnet-20241022"
        ));
        assert!(!model_supports_anthropic_thinking("claude-3-haiku"));
        // Claude 4+ akzeptiert es (Extended Thinking).
        assert!(model_supports_anthropic_thinking("claude-sonnet-4-5"));
        assert!(model_supports_anthropic_thinking("claude-opus-4-7"));
        // Andere Provider-IDs werden nicht versehentlich angenommen.
        assert!(!model_supports_anthropic_thinking("gpt-4o"));
    }

    // --- Request-Body-Builder --------------------------------------------

    #[test]
    fn anthropic_body_omits_thinking_when_disabled() {
        let body = build_anthropic_body(
            "claude-sonnet-4-5",
            "sys",
            vec![],
            None,
            LlmOptions {
                reasoning_enabled: false,
            },
            true,
        );
        assert!(body.get("thinking").is_none(), "body: {body}");
    }

    #[test]
    fn anthropic_body_includes_thinking_when_enabled_and_supported() {
        let body = build_anthropic_body(
            "claude-sonnet-4-5",
            "sys",
            vec![],
            None,
            LlmOptions {
                reasoning_enabled: true,
            },
            true,
        );
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(
            body["thinking"]["budget_tokens"].as_u64().map(|n| n as u32),
            Some(ANTHROPIC_THINKING_BUDGET_TOKENS)
        );
    }

    #[test]
    fn anthropic_body_skips_thinking_for_claude3_even_when_enabled() {
        // Defensive Verteidigung: Claude-3-* würde 400 zurück geben,
        // wenn wir `thinking` mitsendeten. Toggle-On reicht hier nicht.
        let body = build_anthropic_body(
            "claude-3-5-sonnet-20241022",
            "sys",
            vec![],
            None,
            LlmOptions {
                reasoning_enabled: true,
            },
            true,
        );
        assert!(body.get("thinking").is_none(), "body: {body}");
    }

    #[test]
    fn openai_body_omits_reasoning_when_disabled() {
        let body = build_openai_body(
            "deepseek/deepseek-r1",
            vec![],
            None,
            LlmOptions {
                reasoning_enabled: false,
            },
            true,
        );
        assert!(body.get("reasoning").is_none(), "body: {body}");
    }

    #[test]
    fn openai_body_includes_reasoning_for_supported_model() {
        let body = build_openai_body(
            "deepseek/deepseek-r1",
            vec![],
            None,
            LlmOptions {
                reasoning_enabled: true,
            },
            true,
        );
        assert_eq!(body["reasoning"]["effort"], OPENAI_REASONING_EFFORT);
    }

    #[test]
    fn openai_body_skips_reasoning_for_unknown_model_even_when_enabled() {
        // gpt-4o ist kein Reasoning-Modell — der unbekannte Body-Key
        // würde manche OR-Routen mit 400 antworten lassen.
        let body = build_openai_body(
            "gpt-4o",
            vec![],
            None,
            LlmOptions {
                reasoning_enabled: true,
            },
            true,
        );
        assert!(body.get("reasoning").is_none(), "body: {body}");
    }

    // --- SSE-Klassifikatoren ---------------------------------------------

    #[test]
    fn anthropic_parses_thinking_delta() {
        let v = json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": { "type": "thinking_delta", "thinking": "hmm…" }
        });
        assert_eq!(
            classify_anthropic_event(&v),
            AnthropicChunk::Reasoning("hmm…".to_string())
        );
    }

    #[test]
    fn anthropic_parses_text_delta() {
        let v = json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": { "type": "text_delta", "text": "Hallo" }
        });
        assert_eq!(
            classify_anthropic_event(&v),
            AnthropicChunk::Text("Hallo".to_string())
        );
    }

    #[test]
    fn anthropic_legacy_text_delta_without_type_falls_back_to_text() {
        // Älteres SDK-Verhalten ohne `delta.type` — `text` ist dann das
        // einzige Feld. Muss als regulärer Text durchgehen.
        let v = json!({
            "type": "content_block_delta",
            "delta": { "text": "Hi" }
        });
        assert_eq!(
            classify_anthropic_event(&v),
            AnthropicChunk::Text("Hi".to_string())
        );
    }

    #[test]
    fn anthropic_message_stop_terminates() {
        let v = json!({ "type": "message_stop" });
        assert_eq!(classify_anthropic_event(&v), AnthropicChunk::Stop);
    }

    #[test]
    fn anthropic_error_event_surfaces_message() {
        let v = json!({ "type": "error", "error": { "message": "overload" } });
        assert_eq!(
            classify_anthropic_event(&v),
            AnthropicChunk::Error("overload".to_string())
        );
    }

    #[test]
    fn openai_parses_delta_reasoning_field() {
        let v = json!({
            "choices": [{
                "delta": { "reasoning": "lass mich überlegen" }
            }]
        });
        let c = classify_openai_event(&v);
        assert_eq!(c.reasoning.as_deref(), Some("lass mich überlegen"));
        assert!(c.text.is_none());
    }

    #[test]
    fn openai_parses_reasoning_content_field() {
        // DeepSeek-Style — manche OR-Routen liefern den CoT als
        // `reasoning_content` statt `reasoning`.
        let v = json!({
            "choices": [{
                "delta": { "reasoning_content": "step1" }
            }]
        });
        let c = classify_openai_event(&v);
        assert_eq!(c.reasoning.as_deref(), Some("step1"));
    }

    #[test]
    fn openai_parses_content_only_event() {
        let v = json!({
            "choices": [{ "delta": { "content": "ok" } }]
        });
        let c = classify_openai_event(&v);
        assert_eq!(c.text.as_deref(), Some("ok"));
        assert!(c.reasoning.is_none());
    }

    #[test]
    fn openai_combined_reasoning_and_content_in_same_event() {
        let v = json!({
            "choices": [{ "delta": {
                "reasoning": "weil…",
                "content": "die Antwort"
            } }]
        });
        let c = classify_openai_event(&v);
        assert_eq!(c.reasoning.as_deref(), Some("weil…"));
        assert_eq!(c.text.as_deref(), Some("die Antwort"));
    }
}
