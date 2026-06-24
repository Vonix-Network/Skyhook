//! OpenAI Chat Completions provider implementation.
//!
//! Talks to `POST /v1/chat/completions` with `stream: true` and translates
//! incremental SSE `data:` frames into the provider-agnostic [`StreamEvent`]
//! enum. Also implements [`Provider::list_models`] via `GET /v1/models`,
//! filtered to chat-capable model families.

use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures_util::{stream::poll_fn, Stream, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::agent::provider::{
    CompletionRequest, Message, MessageContent, Provider, Role, StreamEvent, ToolSchema, Usage,
};
use crate::error::{Result, SkyhookError};

/// Default OpenAI REST base URL.
const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// Hardcoded fallback model list used when `GET /v1/models` fails.
const FALLBACK_MODELS: &[&str] = &["gpt-4o", "gpt-4o-mini", "gpt-4.1", "o3", "o4-mini"];

/// OpenAI Chat Completions provider.
///
/// Construct with [`OpenAIProvider::new`]. The `base_url` argument is optional
/// and is mostly useful for tests / OpenAI-compatible gateways.
pub struct OpenAIProvider {
    api_key: String,
    base_url: String,
    http: reqwest::Client,
}

impl OpenAIProvider {
    /// Build a new provider. `base_url` defaults to `https://api.openai.com/v1`
    /// when `None` is passed.
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(600))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            api_key,
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            http,
        }
    }

    /// Build the request body for `/v1/chat/completions`.
    fn build_body(&self, req: &CompletionRequest) -> Value {
        let mut messages = Vec::with_capacity(req.messages.len() + 1);
        if !req.system.is_empty() {
            messages.push(json!({ "role": "system", "content": req.system }));
        }
        for m in &req.messages {
            messages.extend(message_to_openai(m));
        }

        let tools: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    }
                })
            })
            .collect();

        // Stable per-conversation hash for prompt caching. Built off the
        // static prefix (system + tool defs) so the key stays constant for
        // the whole conversation.
        let cache_key = stable_cache_key(&req.system, &req.tools);

        let mut body = json!({
            "model": req.model,
            "messages": messages,
            "stream": true,
            "max_tokens": req.max_tokens,
            "user": "skyhook",
            "store": false,
            "stream_options": { "include_usage": true },
            "prompt_cache_key": cache_key,
        });
        if !tools.is_empty() {
            body["tools"] = Value::Array(tools);
        }
        body
    }
}

#[async_trait]
impl Provider for OpenAIProvider {
    fn name(&self) -> &'static str {
        "openai"
    }

    async fn stream_chat(
        &self,
        req: CompletionRequest,
    ) -> std::result::Result<Pin<Box<dyn Stream<Item = StreamEvent> + Send>>, SkyhookError> {
        let body = self.build_body(&req);
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));

        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| SkyhookError::Other(format!("openai request: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(SkyhookError::Other(format!(
                "openai http {status}: {text}"
            )));
        }

        let (tx, mut rx) = mpsc::channel::<StreamEvent>(64);
        tokio::spawn(async move {
            let mut started = false;
            let mut active_tool: Option<String> = None;
            let mut last_usage: Option<Usage> = None;
            let mut stop_reason: Option<String> = None;

            let mut stream = resp.bytes_stream().eventsource();
            while let Some(ev) = stream.next().await {
                let ev = match ev {
                    Ok(e) => e,
                    Err(e) => {
                        let _ = tx.send(StreamEvent::Error(format!("sse: {e}"))).await;
                        return;
                    }
                };
                let data = ev.data;
                if data == "[DONE]" {
                    break;
                }
                let chunk: ChatChunk = match serde_json::from_str(&data) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx
                            .send(StreamEvent::Error(format!("decode: {e}: {data}")))
                            .await;
                        return;
                    }
                };

                if let Some(u) = chunk.usage {
                    last_usage = Some(Usage {
                        input_tokens: u.prompt_tokens.unwrap_or(0),
                        output_tokens: u.completion_tokens.unwrap_or(0),
                        cache_read_tokens: u
                            .prompt_tokens_details
                            .as_ref()
                            .and_then(|d| d.cached_tokens)
                            .unwrap_or(0),
                        cache_creation_tokens: 0,
                    });
                }

                let Some(choice) = chunk.choices.into_iter().next() else {
                    continue;
                };

                if !started && choice.delta.role.as_deref() == Some("assistant") {
                    started = true;
                    if tx.send(StreamEvent::MessageStart).await.is_err() {
                        return;
                    }
                }

                if let Some(text) = choice.delta.content {
                    if !text.is_empty() {
                        if !started {
                            started = true;
                            let _ = tx.send(StreamEvent::MessageStart).await;
                        }
                        if tx.send(StreamEvent::TextDelta(text)).await.is_err() {
                            return;
                        }
                    }
                }

                if let Some(tcs) = choice.delta.tool_calls {
                    for tc in tcs {
                        // A new tool call starts when we see an `id`.
                        if let Some(id) = tc.id.clone() {
                            if let Some(prev) = active_tool.take() {
                                let _ = tx.send(StreamEvent::ToolUseEnd { id: prev }).await;
                            }
                            let name = tc
                                .function
                                .as_ref()
                                .and_then(|f| f.name.clone())
                                .unwrap_or_default();
                            active_tool = Some(id.clone());
                            if tx
                                .send(StreamEvent::ToolUseStart { id, name })
                                .await
                                .is_err()
                            {
                                return;
                            }
                        }
                        if let Some(args) = tc.function.and_then(|f| f.arguments) {
                            if !args.is_empty() {
                                let id = active_tool.clone().unwrap_or_default();
                                if tx
                                    .send(StreamEvent::ToolUseInputDelta {
                                        id,
                                        partial_json: args,
                                    })
                                    .await
                                    .is_err()
                                {
                                    return;
                                }
                            }
                        }
                    }
                }

                if let Some(reason) = choice.finish_reason {
                    stop_reason = Some(reason);
                }
            }

            if let Some(id) = active_tool.take() {
                let _ = tx.send(StreamEvent::ToolUseEnd { id }).await;
            }
            let _ = tx
                .send(StreamEvent::MessageEnd {
                    stop_reason: stop_reason.unwrap_or_else(|| "stop".to_string()),
                    usage: last_usage.unwrap_or_default(),
                })
                .await;
        });

        Ok(Box::pin(poll_fn(move |cx| rx.poll_recv(cx))))
    }

    async fn list_models(&self) -> Result<Vec<String>> {
        let url = format!("{}/models", self.base_url.trim_end_matches('/'));
        let result: Result<Vec<String>> = async {
            let resp = self
                .http
                .get(&url)
                .bearer_auth(&self.api_key)
                .send()
                .await
                .map_err(|e| SkyhookError::Other(format!("openai models: {e}")))?;
            if !resp.status().is_success() {
                return Err(SkyhookError::Other(format!(
                    "openai models http {}",
                    resp.status()
                )));
            }
            let parsed: ModelsResponse = resp
                .json()
                .await
                .map_err(|e| SkyhookError::Other(format!("openai models decode: {e}")))?;
            let mut names: Vec<String> = parsed
                .data
                .into_iter()
                .map(|m| m.id)
                .filter(|id| is_chat_model(id))
                .collect();
            names.sort();
            names.dedup();
            Ok(names)
        }
        .await;

        match result {
            Ok(v) if !v.is_empty() => Ok(v),
            _ => Ok(FALLBACK_MODELS.iter().map(|s| s.to_string()).collect()),
        }
    }
}

/// Return `true` for chat-capable model families. Excludes audio/image/embed
/// variants by inspecting common suffixes.
fn is_chat_model(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    let prefix_ok = lower.starts_with("gpt-")
        || lower.starts_with("o1")
        || lower.starts_with("o3")
        || lower.starts_with("o4")
        || lower.starts_with("chatgpt");
    if !prefix_ok {
        return false;
    }
    // Filter out non-chat modalities.
    const BAD: &[&str] = &[
        "audio",
        "tts",
        "whisper",
        "embedding",
        "moderation",
        "image",
        "vision-preview",
        "realtime",
        "transcribe",
        "search",
    ];
    !BAD.iter().any(|b| lower.contains(b))
}

/// Build a stable cache key over the static prefix of the request.
fn stable_cache_key(system: &str, tools: &[ToolSchema]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    system.hash(&mut h);
    for t in tools {
        t.name.hash(&mut h);
        t.description.hash(&mut h);
        t.input_schema.to_string().hash(&mut h);
    }
    format!("skyhook-{:016x}", h.finish())
}

/// Translate a single internal [`Message`] into the one or more OpenAI
/// chat-completions message objects it maps to.
fn message_to_openai(m: &Message) -> Vec<Value> {
    let role = match m.role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "system",
    };

    let mut out = Vec::new();
    let mut text_parts: Vec<String> = Vec::new();
    let mut tool_calls: Vec<Value> = Vec::new();
    let mut tool_results: Vec<(String, String, bool)> = Vec::new();

    for c in &m.content {
        match c {
            MessageContent::Text { text } => text_parts.push(text.clone()),
            MessageContent::ToolUse { id, name, input } => {
                tool_calls.push(json!({
                    "id": id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": input.to_string(),
                    }
                }));
            }
            MessageContent::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                tool_results.push((tool_use_id.clone(), content.clone(), *is_error));
            }
            MessageContent::Thinking { .. } => { /* OpenAI rejects on input */ }
        }
    }

    // Tool results always go as their own role:"tool" messages.
    for (id, content, _is_err) in tool_results {
        out.push(json!({
            "role": "tool",
            "tool_call_id": id,
            "content": content,
        }));
    }

    let has_text = !text_parts.is_empty();
    let has_calls = !tool_calls.is_empty();
    if has_text || has_calls {
        let mut msg = json!({ "role": role });
        if has_text {
            msg["content"] = Value::String(text_parts.join(""));
        } else {
            msg["content"] = Value::Null;
        }
        if has_calls {
            msg["tool_calls"] = Value::Array(tool_calls);
        }
        out.push(msg);
    }

    out
}

// --- wire types ---

#[derive(Debug, Deserialize)]
struct ChatChunk {
    #[serde(default)]
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<ChunkUsage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    #[serde(default)]
    delta: ChatDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ChatDelta {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ChatToolCall>>,
}

#[derive(Debug, Deserialize)]
struct ChatToolCall {
    #[serde(default)]
    #[allow(dead_code)]
    index: Option<u32>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<ChatToolFn>,
}

#[derive(Debug, Deserialize)]
struct ChatToolFn {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChunkUsage {
    #[serde(default)]
    prompt_tokens: Option<u32>,
    #[serde(default)]
    completion_tokens: Option<u32>,
    #[serde(default)]
    prompt_tokens_details: Option<PromptTokensDetails>,
}

#[derive(Debug, Deserialize)]
struct PromptTokensDetails {
    #[serde(default)]
    cached_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
}
