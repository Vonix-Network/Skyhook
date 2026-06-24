//! Anthropic Messages API provider with streaming + 4-breakpoint prompt caching.
//!
//! Implements [`Provider`] against `POST https://api.anthropic.com/v1/messages`
//! with `stream: true`. SSE events are parsed via `eventsource-stream` and
//! mapped onto [`StreamEvent`]s.
//!
//! # Cache strategy
//!
//! Anthropic allows up to 4 `cache_control` breakpoints per request. We use:
//!
//! 1. The trailing text block of `system` (system prompt + tool defs prefix).
//! 2. The `tools` array — we mark the last tool with `cache_control` so the
//!    serialized tool list is part of the cacheable prefix.
//! 3. The earliest user turn (first message), once the conversation has 4+
//!    messages, to lock in the opening exchange.
//! 4. The most recent assistant message before the current user turn (sliding
//!    window) — re-cached just-in-time on every turn.

use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures_util::{stream, Stream, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use std::pin::Pin;

use crate::agent::provider::{
    CompletionRequest, EventStream, Message, MessageContent, Provider, Role, StreamEvent,
    ToolSchema, Usage,
};
use crate::error::SkyhookError;

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Anthropic Messages API provider.
pub struct AnthropicProvider {
    api_key: String,
    base_url: String,
    client: reqwest::Client,
}

impl AnthropicProvider {
    /// Construct a new provider. `base_url` defaults to
    /// `https://api.anthropic.com` when `None`.
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            api_key,
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            client,
        }
    }

    /// Build the JSON request body, applying the cache-control strategy.
    fn build_request_body(&self, req: &CompletionRequest) -> Value {
        // --- System: one text block, cache_control on the (only) block. ---
        let system = json!([
            {
                "type": "text",
                "text": req.system,
                "cache_control": { "type": "ephemeral" }
            }
        ]);

        // --- Tools: standard shape; cache_control on the last tool. ---
        let mut tools: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                })
            })
            .collect();
        if let Some(last) = tools.last_mut() {
            if let Some(obj) = last.as_object_mut() {
                obj.insert(
                    "cache_control".to_string(),
                    json!({ "type": "ephemeral" }),
                );
            }
        }

        // --- Messages: convert canonical -> Anthropic wire shape. ---
        // We may apply up to two additional cache breakpoints here (2 of 4),
        // since system + last-tool already consume 2.
        let n = req.messages.len();
        let mut messages: Vec<Value> = req
            .messages
            .iter()
            .filter(|m| !matches!(m.role, Role::System))
            .map(message_to_wire)
            .collect();

        // Breakpoint 3: earliest user turn, once the conversation is non-trivial.
        if n >= 4 {
            if let Some(first) = messages.first_mut() {
                mark_last_content_cached(first);
            }
        }
        // Breakpoint 4: most-recent message before the current pending turn.
        // We treat "last message" as the breakpoint (sliding window).
        if let Some(last) = messages.last_mut() {
            // Avoid double-marking when len == 1 (already marked above only if n >= 4 and == 1, impossible).
            mark_last_content_cached(last);
        }

        json!({
            "model": req.model,
            "max_tokens": req.max_tokens,
            "system": system,
            "tools": tools,
            "messages": messages,
            "stream": true,
        })
    }
}

/// Convert a canonical [`Message`] into Anthropic's wire JSON.
fn message_to_wire(msg: &Message) -> Value {
    let role = match msg.role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "user", // shouldn't happen — filtered upstream
    };
    let content: Vec<Value> = msg.content.iter().map(content_to_wire).collect();
    json!({ "role": role, "content": content })
}

fn content_to_wire(c: &MessageContent) -> Value {
    match c {
        MessageContent::Text { text } => json!({ "type": "text", "text": text }),
        MessageContent::ToolUse { id, name, input } => {
            json!({ "type": "tool_use", "id": id, "name": name, "input": input })
        }
        MessageContent::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => json!({
            "type": "tool_result",
            "tool_use_id": tool_use_id,
            "content": content,
            "is_error": is_error,
        }),
        MessageContent::Thinking { thinking } => {
            json!({ "type": "thinking", "thinking": thinking })
        }
    }
}

/// Add `cache_control: ephemeral` to the last content block of a wire message.
fn mark_last_content_cached(message: &mut Value) {
    let Some(content) = message.get_mut("content").and_then(|v| v.as_array_mut()) else {
        return;
    };
    let Some(last) = content.last_mut() else {
        return;
    };
    if let Some(obj) = last.as_object_mut() {
        obj.insert(
            "cache_control".to_string(),
            json!({ "type": "ephemeral" }),
        );
    }
}

// --- SSE event parsing types ----------------------------------------------

#[derive(Debug, Deserialize)]
struct WireUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
    #[serde(default)]
    cache_creation_input_tokens: u32,
    #[serde(default)]
    cache_read_input_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct MessageStartData {
    message: MessageStartInner,
}

#[derive(Debug, Deserialize)]
struct MessageStartInner {
    #[serde(default)]
    usage: Option<WireUsage>,
}

#[derive(Debug, Deserialize)]
struct ContentBlockStartData {
    index: u32,
    content_block: ContentBlockKind,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(dead_code)]
enum ContentBlockKind {
    Text {
        #[serde(default)]
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        #[serde(default)]
        input: Value,
    },
    Thinking {
        #[serde(default)]
        thinking: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct ContentBlockDeltaData {
    index: u32,
    delta: DeltaKind,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum DeltaKind {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
    ThinkingDelta { thinking: String },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct ContentBlockStopData {
    index: u32,
}

#[derive(Debug, Deserialize)]
struct MessageDeltaData {
    #[serde(default)]
    delta: MessageDeltaInner,
    #[serde(default)]
    usage: Option<WireUsage>,
}

#[derive(Debug, Deserialize, Default)]
struct MessageDeltaInner {
    #[serde(default)]
    stop_reason: Option<String>,
}

/// State a worker keeps while folding SSE chunks into [`StreamEvent`]s.
#[derive(Default)]
pub(crate) struct StreamState {
    /// Map content-block index -> tool_use id (so `ToolUseEnd` knows the id).
    tool_indices: std::collections::HashMap<u32, String>,
    usage: Usage,
    stop_reason: String,
}

/// Parse one SSE `event` + `data` pair into zero or more [`StreamEvent`]s.
///
/// This is exposed at crate level so it can be unit-tested without a real
/// network connection.
pub(crate) fn parse_sse_event(
    state: &mut StreamState,
    event: &str,
    data: &str,
) -> Vec<StreamEvent> {
    let mut out = Vec::new();
    match event {
        "message_start" => {
            out.push(StreamEvent::MessageStart);
            if let Ok(d) = serde_json::from_str::<MessageStartData>(data) {
                if let Some(u) = d.message.usage {
                    state.usage.input_tokens = u.input_tokens;
                    state.usage.output_tokens = u.output_tokens;
                    state.usage.cache_creation_tokens = u.cache_creation_input_tokens;
                    state.usage.cache_read_tokens = u.cache_read_input_tokens;
                }
            }
        }
        "content_block_start" => {
            if let Ok(d) = serde_json::from_str::<ContentBlockStartData>(data) {
                match d.content_block {
                    ContentBlockKind::ToolUse { id, name, .. } => {
                        state.tool_indices.insert(d.index, id.clone());
                        out.push(StreamEvent::ToolUseStart { id, name });
                    }
                    ContentBlockKind::Text { .. }
                    | ContentBlockKind::Thinking { .. }
                    | ContentBlockKind::Other => {}
                }
            }
        }
        "content_block_delta" => {
            if let Ok(d) = serde_json::from_str::<ContentBlockDeltaData>(data) {
                match d.delta {
                    DeltaKind::TextDelta { text } => out.push(StreamEvent::TextDelta(text)),
                    DeltaKind::ThinkingDelta { thinking } => {
                        out.push(StreamEvent::ThinkingDelta(thinking))
                    }
                    DeltaKind::InputJsonDelta { partial_json } => {
                        if let Some(id) = state.tool_indices.get(&d.index).cloned() {
                            out.push(StreamEvent::ToolUseInputDelta { id, partial_json });
                        }
                    }
                    DeltaKind::Other => {}
                }
            }
        }
        "content_block_stop" => {
            if let Ok(d) = serde_json::from_str::<ContentBlockStopData>(data) {
                if let Some(id) = state.tool_indices.remove(&d.index) {
                    out.push(StreamEvent::ToolUseEnd { id });
                }
            }
        }
        "message_delta" => {
            if let Ok(d) = serde_json::from_str::<MessageDeltaData>(data) {
                if let Some(reason) = d.delta.stop_reason {
                    state.stop_reason = reason;
                }
                if let Some(u) = d.usage {
                    // message_delta usage refreshes output count + cache fields.
                    if u.input_tokens > 0 {
                        state.usage.input_tokens = u.input_tokens;
                    }
                    if u.output_tokens > 0 {
                        state.usage.output_tokens = u.output_tokens;
                    }
                    if u.cache_creation_input_tokens > 0 {
                        state.usage.cache_creation_tokens = u.cache_creation_input_tokens;
                    }
                    if u.cache_read_input_tokens > 0 {
                        state.usage.cache_read_tokens = u.cache_read_input_tokens;
                    }
                }
            }
        }
        "message_stop" => {
            out.push(StreamEvent::MessageEnd {
                stop_reason: std::mem::take(&mut state.stop_reason),
                usage: std::mem::take(&mut state.usage),
            });
        }
        "error" => {
            // Anthropic SSE error frames have shape { error: { message: "..." } }.
            let msg = serde_json::from_str::<Value>(data)
                .ok()
                .and_then(|v| {
                    v.get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| data.to_string());
            out.push(StreamEvent::Error(msg));
        }
        _ => {}
    }
    out
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn name(&self) -> &'static str {
        "anthropic"
    }

    async fn stream_chat(&self, req: CompletionRequest) -> Result<EventStream, SkyhookError> {
        let body = self.build_request_body(&req);
        let url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .header("accept", "text/event-stream")
            .json(&body)
            .send()
            .await
            .map_err(|e| SkyhookError::Agent(format!("anthropic request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(SkyhookError::Agent(format!(
                "anthropic http {status}: {text}"
            )));
        }

        let byte_stream = resp.bytes_stream();
        let sse = byte_stream.eventsource();
        let mut state = StreamState::default();

        let mapped = sse.flat_map(move |item| {
            let events: Vec<StreamEvent> = match item {
                Ok(ev) => parse_sse_event(&mut state, &ev.event, &ev.data),
                Err(e) => vec![StreamEvent::Error(format!("sse: {e}"))],
            };
            stream::iter(events)
        });

        let pinned: Pin<Box<dyn Stream<Item = StreamEvent> + Send>> = Box::pin(mapped);
        Ok(pinned)
    }

    async fn list_models(&self) -> Result<Vec<String>, SkyhookError> {
        Ok(vec![
            "claude-sonnet-4-5-20250929".to_string(),
            "claude-opus-4-1-20250805".to_string(),
            "claude-haiku-4-5".to_string(),
        ])
    }
}

// Silence unused-warning when the struct is only constructed via Serialize/Deserialize.
#[allow(dead_code)]
fn _wire_tool_schema_compile_check(_t: &ToolSchema) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(events: &[(&str, &str)]) -> Vec<StreamEvent> {
        let mut state = StreamState::default();
        let mut out = Vec::new();
        for (ev, data) in events {
            out.extend(parse_sse_event(&mut state, ev, data));
        }
        out
    }

    #[test]
    fn parses_full_text_stream() {
        let events = [
            (
                "message_start",
                r#"{"type":"message_start","message":{"id":"msg_1","usage":{"input_tokens":10,"output_tokens":1,"cache_read_input_tokens":4,"cache_creation_input_tokens":0}}}"#,
            ),
            (
                "content_block_start",
                r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
            ),
            (
                "content_block_delta",
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#,
            ),
            (
                "content_block_delta",
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" world"}}"#,
            ),
            (
                "content_block_stop",
                r#"{"type":"content_block_stop","index":0}"#,
            ),
            (
                "message_delta",
                r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":7}}"#,
            ),
            ("message_stop", r#"{"type":"message_stop"}"#),
        ];

        let out = run(&events);

        assert!(matches!(out.first(), Some(StreamEvent::MessageStart)));
        let deltas: Vec<&str> = out
            .iter()
            .filter_map(|e| match e {
                StreamEvent::TextDelta(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(deltas, vec!["Hello", " world"]);

        match out.last() {
            Some(StreamEvent::MessageEnd { stop_reason, usage }) => {
                assert_eq!(stop_reason, "end_turn");
                assert_eq!(usage.input_tokens, 10);
                assert_eq!(usage.output_tokens, 7);
                assert_eq!(usage.cache_read_tokens, 4);
            }
            other => panic!("expected MessageEnd, got {other:?}"),
        }
    }

    #[test]
    fn parses_tool_use_stream() {
        let events = [
            (
                "message_start",
                r#"{"type":"message_start","message":{"id":"msg_2","usage":{"input_tokens":5,"output_tokens":0}}}"#,
            ),
            (
                "content_block_start",
                r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_abc","name":"sftp_list_dir","input":{}}}"#,
            ),
            (
                "content_block_delta",
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"path\":"}}"#,
            ),
            (
                "content_block_delta",
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"\"/tmp\"}"}}"#,
            ),
            (
                "content_block_stop",
                r#"{"type":"content_block_stop","index":0}"#,
            ),
            (
                "message_delta",
                r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":12}}"#,
            ),
            ("message_stop", r#"{"type":"message_stop"}"#),
        ];

        let out = run(&events);
        let started = out
            .iter()
            .find(|e| matches!(e, StreamEvent::ToolUseStart { .. }))
            .expect("ToolUseStart");
        match started {
            StreamEvent::ToolUseStart { id, name } => {
                assert_eq!(id, "toolu_abc");
                assert_eq!(name, "sftp_list_dir");
            }
            _ => unreachable!(),
        }
        let frags: String = out
            .iter()
            .filter_map(|e| match e {
                StreamEvent::ToolUseInputDelta { partial_json, .. } => Some(partial_json.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(frags, r#"{"path":"/tmp"}"#);
        assert!(out
            .iter()
            .any(|e| matches!(e, StreamEvent::ToolUseEnd { id } if id == "toolu_abc")));
        match out.last() {
            Some(StreamEvent::MessageEnd { stop_reason, .. }) => {
                assert_eq!(stop_reason, "tool_use")
            }
            other => panic!("expected MessageEnd, got {other:?}"),
        }
    }

    #[test]
    fn cache_control_applied_to_system_and_last_tool() {
        let provider = AnthropicProvider::new("test".into(), None);
        let tools = vec![
            ToolSchema {
                name: "a".into(),
                description: "x".into(),
                input_schema: json!({}),
            },
            ToolSchema {
                name: "b".into(),
                description: "y".into(),
                input_schema: json!({}),
            },
        ];
        let req = CompletionRequest {
            system: "sys".into(),
            messages: vec![Message {
                role: Role::User,
                content: vec![MessageContent::Text { text: "hi".into() }],
                timestamp: 0,
                usage: None,
            }],
            tools,
            model: "claude-sonnet-4-5-20250929".into(),
            max_tokens: 1024,
        };
        let body = provider.build_request_body(&req);

        let system = &body["system"][0];
        assert_eq!(system["cache_control"]["type"], "ephemeral");

        let tools_arr = body["tools"].as_array().unwrap();
        assert!(tools_arr[0].get("cache_control").is_none());
        assert_eq!(tools_arr[1]["cache_control"]["type"], "ephemeral");

        // Single message — last-message breakpoint should mark its last content.
        let msg0 = &body["messages"][0]["content"];
        let last = msg0.as_array().unwrap().last().unwrap();
        assert_eq!(last["cache_control"]["type"], "ephemeral");
    }
}
