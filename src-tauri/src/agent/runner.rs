//! Agent runner — drives the send → stream → tool-call → tool-result loop.
//!
//! One [`AgentRunner`] is created per `AppState`. Each call to
//! [`AgentRunner::run_turn`] processes a single user message: it streams the
//! assistant's response, executes any tool calls (gated on approval), feeds
//! results back to the model, and repeats until the model emits an
//! `end_turn` stop reason, calls `task_complete`, or the per-turn safety cap
//! is reached.
//!
//! Cancellation: callers can flip the per-conversation [`CancelFlag`] via
//! [`AgentRunner::cancel`] to abort an in-flight turn between provider
//! events.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures_util::StreamExt;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

use crate::agent::approval::{ApprovalDecision, ApprovalGate};
use crate::agent::history::{Conversation, ConversationStore};
use crate::agent::provider::{
    CompletionRequest, Message, MessageContent, Provider, Role, StreamEvent, Usage,
};
use crate::agent::tools::{all_tool_schemas, dispatch_tool, ApprovalMode, ToolCall};
use crate::error::{Result, SkyhookError};
use crate::session::SessionManager;

/// Per-conversation cancellation flag shared between the Tauri command
/// surface and the running turn.
pub type CancelFlag = Arc<AtomicBool>;

/// Cancellation registry — `conversation_id -> CancelFlag`.
#[derive(Default)]
pub struct CancelRegistry {
    flags: Mutex<HashMap<String, CancelFlag>>,
}

impl CancelRegistry {
    /// Insert (or reset) and return the cancel flag for `conversation_id`.
    pub async fn register(&self, conversation_id: &str) -> CancelFlag {
        let flag = Arc::new(AtomicBool::new(false));
        self.flags
            .lock()
            .await
            .insert(conversation_id.to_string(), flag.clone());
        flag
    }

    /// Set the flag for `conversation_id`, if any. Returns whether one existed.
    pub async fn cancel(&self, conversation_id: &str) -> bool {
        if let Some(f) = self.flags.lock().await.get(conversation_id) {
            f.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    /// Remove the flag (turn finished).
    pub async fn clear(&self, conversation_id: &str) {
        self.flags.lock().await.remove(conversation_id);
    }
}

/// Top-level runner. Cheap to clone (everything inside is `Arc`).
#[derive(Clone)]
pub struct AgentRunner {
    pub provider: Arc<dyn Provider>,
    pub sessions: Arc<SessionManager>,
    pub session_id: String,
    pub store: Arc<ConversationStore>,
    pub approvals: Arc<ApprovalGate>,
    pub cancels: Arc<CancelRegistry>,
    pub app: AppHandle,
    pub max_turns: u32,
}

impl AgentRunner {
    /// Append `user_input` to `conv`, then loop send/stream/tool/repeat until
    /// the model finishes its turn or the safety cap is hit.
    pub async fn run_turn(
        &self,
        conv: &mut Conversation,
        user_input: String,
        approval_mode: ApprovalMode,
        system_text: String,
    ) -> Result<()> {
        let conversation_id = conv.meta.id.clone();
        let cancel = self.cancels.register(&conversation_id).await;

        // Append the user turn.
        conv.messages.push(Message {
            role: Role::User,
            content: vec![MessageContent::Text { text: user_input }],
            timestamp: chrono::Utc::now().timestamp(),
            usage: None,
        });
        conv.meta.updated_at = chrono::Utc::now().timestamp();
        self.store.save(conv).await?;

        let mut iterations: u32 = 0;
        let tools = all_tool_schemas();

        let final_result = loop {
            if cancel.load(Ordering::SeqCst) {
                self.emit_error(&conversation_id, "cancelled");
                break Ok(());
            }
            iterations += 1;
            if iterations > self.max_turns {
                self.emit_error(&conversation_id, "max_turns_exceeded");
                break Ok(());
            }

            let req = CompletionRequest {
                system: system_text.clone(),
                messages: conv.messages.clone(),
                tools: tools.clone(),
                model: conv.meta.model.clone(),
                max_tokens: 4096,
            };

            let mut stream = self.provider.stream_chat(req).await?;

            let mut assistant_text = String::new();
            let mut thinking_text = String::new();
            #[derive(Default)]
            struct PendingTool {
                name: String,
                input_buf: String,
            }
            let mut pending: HashMap<String, PendingTool> = HashMap::new();
            let mut tool_order: Vec<String> = Vec::new();
            let mut stop_reason = String::from("end_turn");
            let mut usage = Usage::default();
            let mut stream_err: Option<String> = None;

            while let Some(event) = stream.next().await {
                if cancel.load(Ordering::SeqCst) {
                    stream_err = Some("cancelled".into());
                    break;
                }
                match event {
                    StreamEvent::MessageStart => {}
                    StreamEvent::TextDelta(d) => {
                        assistant_text.push_str(&d);
                        let _ = self.app.emit(
                            "agent-message-delta",
                            json!({ "conversation_id": conversation_id, "content_delta": d }),
                        );
                    }
                    StreamEvent::ThinkingDelta(d) => {
                        thinking_text.push_str(&d);
                        let _ = self.app.emit(
                            "agent-thinking-delta",
                            json!({ "conversation_id": conversation_id, "content_delta": d }),
                        );
                    }
                    StreamEvent::ToolUseStart { id, name } => {
                        tool_order.push(id.clone());
                        pending.insert(
                            id,
                            PendingTool {
                                name,
                                input_buf: String::new(),
                            },
                        );
                    }
                    StreamEvent::ToolUseInputDelta { id, partial_json } => {
                        if let Some(p) = pending.get_mut(&id) {
                            p.input_buf.push_str(&partial_json);
                        }
                    }
                    StreamEvent::ToolUseEnd { .. } => {}
                    StreamEvent::MessageEnd {
                        stop_reason: sr,
                        usage: u,
                    } => {
                        stop_reason = sr;
                        usage = u;
                    }
                    StreamEvent::Error(e) => {
                        stream_err = Some(e);
                        break;
                    }
                }
            }

            if let Some(e) = stream_err {
                self.emit_error(&conversation_id, &e);
                break Ok(());
            }

            // Build assistant message content.
            let mut assistant_content = Vec::new();
            if !thinking_text.is_empty() {
                assistant_content.push(MessageContent::Thinking {
                    thinking: thinking_text,
                });
            }
            if !assistant_text.is_empty() {
                assistant_content.push(MessageContent::Text {
                    text: assistant_text,
                });
            }
            // Resolve pending tool calls in stream order.
            let mut parsed_calls: Vec<(String, ToolCall)> = Vec::new();
            for id in &tool_order {
                if let Some(p) = pending.remove(id) {
                    let input: Value = if p.input_buf.trim().is_empty() {
                        json!({})
                    } else {
                        serde_json::from_str(&p.input_buf).unwrap_or(json!({}))
                    };
                    assistant_content.push(MessageContent::ToolUse {
                        id: id.clone(),
                        name: p.name.clone(),
                        input: input.clone(),
                    });
                    match ToolCall::from_name_and_input(&p.name, input) {
                        Ok(call) => parsed_calls.push((id.clone(), call)),
                        Err(e) => {
                            // Keep the tool_use in history; we'll feed back an
                            // error tool_result below.
                            parsed_calls.push((
                                id.clone(),
                                ToolCall::TaskComplete {
                                    summary: format!("(parse error: {e})"),
                                },
                            ));
                        }
                    }
                }
            }

            conv.messages.push(Message {
                role: Role::Assistant,
                content: assistant_content,
                timestamp: chrono::Utc::now().timestamp(),
                usage: Some(usage.clone()),
            });
            conv.meta.updated_at = chrono::Utc::now().timestamp();
            self.store.save(conv).await?;

            // No tool calls — turn is complete.
            if parsed_calls.is_empty() {
                self.emit_turn_end(&conversation_id, &usage);
                break Ok(());
            }

            // Walk through calls, executing or rejecting as appropriate.
            let mut tool_results: Vec<MessageContent> = Vec::new();
            let mut saw_task_complete = false;
            for (call_id, call) in &parsed_calls {
                if cancel.load(Ordering::SeqCst) {
                    break;
                }

                // Notify UI that a tool is being invoked.
                let args_value = serde_json::to_value(call).unwrap_or(json!({}));
                let _ = self.app.emit(
                    "agent-tool-call",
                    json!({
                        "conversation_id": conversation_id,
                        "call_id": call_id,
                        "tool_name": call.name(),
                        "args": args_value,
                    }),
                );

                let needs = call.needs_approval(approval_mode);
                if needs {
                    let _ = self.app.emit(
                        "agent-tool-approval",
                        json!({
                            "conversation_id": conversation_id,
                            "call_id": call_id,
                            "tool_name": call.name(),
                            "args": args_value,
                            "preview": call.preview(),
                        }),
                    );
                    let decision = self.approvals.request(call_id.clone()).await;
                    if let ApprovalDecision::Rejected(reason) = decision {
                        let msg = format!("User rejected: {reason}");
                        let _ = self.app.emit(
                            "agent-tool-result",
                            json!({
                                "conversation_id": conversation_id,
                                "call_id": call_id,
                                "ok": false,
                                "output_preview": msg.chars().take(200).collect::<String>(),
                            }),
                        );
                        tool_results.push(MessageContent::ToolResult {
                            tool_use_id: call_id.clone(),
                            content: msg,
                            is_error: true,
                        });
                        continue;
                    }
                }

                // Dispatch.
                let outcome =
                    dispatch_tool(&self.sessions, &self.app, &self.session_id, call).await;
                let (ok, content) = match outcome {
                    Ok(out) => (true, out),
                    Err(e) => (false, format!("tool error: {e}")),
                };
                let _ = self.app.emit(
                    "agent-tool-result",
                    json!({
                        "conversation_id": conversation_id,
                        "call_id": call_id,
                        "ok": ok,
                        "output_preview": content.chars().take(400).collect::<String>(),
                    }),
                );
                tool_results.push(MessageContent::ToolResult {
                    tool_use_id: call_id.clone(),
                    content,
                    is_error: !ok,
                });

                if matches!(call, ToolCall::TaskComplete { .. }) {
                    saw_task_complete = true;
                }
            }

            // Append the tool_result message (user role per Anthropic shape).
            if !tool_results.is_empty() {
                conv.messages.push(Message {
                    role: Role::User,
                    content: tool_results,
                    timestamp: chrono::Utc::now().timestamp(),
                    usage: None,
                });
                conv.meta.updated_at = chrono::Utc::now().timestamp();
                self.store.save(conv).await?;
            }

            if saw_task_complete || matches!(stop_reason.as_str(), "end_turn" | "stop") {
                self.emit_turn_end(&conversation_id, &usage);
                break Ok(());
            }
            if cancel.load(Ordering::SeqCst) {
                self.emit_error(&conversation_id, "cancelled");
                break Ok(());
            }
            // Otherwise loop and let the model react to tool results.
        };

        self.cancels.clear(&conversation_id).await;
        final_result
    }

    fn emit_turn_end(&self, conversation_id: &str, usage: &Usage) {
        let _ = self.app.emit(
            "agent-turn-end",
            json!({
                "conversation_id": conversation_id,
                "usage": {
                    "input": usage.input_tokens,
                    "output": usage.output_tokens,
                    "cache_read": usage.cache_read_tokens,
                    "cache_creation": usage.cache_creation_tokens,
                }
            }),
        );
    }

    fn emit_error(&self, conversation_id: &str, err: &str) {
        let _ = self.app.emit(
            "agent-error",
            json!({ "conversation_id": conversation_id, "error": err }),
        );
    }

    /// Request cancellation of an in-flight turn for `conversation_id`.
    pub async fn cancel(&self, conversation_id: &str) {
        self.cancels.cancel(conversation_id).await;
        self.approvals.cancel_all().await;
    }
}

/// User-facing agent configuration (mirrors v0.6.0-PLAN.md schema).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentSettings {
    /// `"anthropic"` or `"openai"`.
    pub default_provider: String,
    pub anthropic_model: String,
    pub openai_model: String,
    /// Safety cap for runner iterations per `agent_send_message`.
    pub max_turns_per_invocation: u32,
    /// One of `"manual" | "auto_read" | "yolo"`.
    pub default_approval_mode: String,
    /// One of `"low" | "medium" | "high"` (o-series).
    pub reasoning_effort: String,
    pub show_thinking: bool,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            default_provider: "anthropic".into(),
            anthropic_model: "claude-sonnet-4-5-20250929".into(),
            openai_model: "gpt-4o".into(),
            max_turns_per_invocation: 20,
            default_approval_mode: "auto_read".into(),
            reasoning_effort: "medium".into(),
            show_thinking: false,
        }
    }
}

/// Shared agent runtime placed into Tauri state by `lib.rs` (Wave 3).
///
/// Holds everything the agent Tauri commands need that is *not* covered by
/// the existing [`crate::AppState`]: the conversation store, approval gate,
/// cancellation registry, current provider selection, and the user-facing
/// agent settings. Wave 1 ships the type and command surface; Wave 3 wires
/// the `manage(AgentRuntime::new(...))` call in `lib.rs`.
pub struct AgentRuntime {
    pub store: Arc<ConversationStore>,
    pub approvals: Arc<ApprovalGate>,
    pub cancels: Arc<CancelRegistry>,
    /// Currently selected provider, if any. Swapped in/out by
    /// `agent_save_settings` / `agent_set_api_key`.
    pub provider: tokio::sync::RwLock<Option<Arc<dyn Provider>>>,
    /// Persisted agent settings.
    pub settings: tokio::sync::RwLock<crate::agent::AgentSettings>,
}

impl AgentRuntime {
    /// Build an empty runtime — provider is `None` until an API key is set.
    pub fn new() -> Result<Self> {
        Ok(Self {
            store: Arc::new(ConversationStore::new()?),
            approvals: Arc::new(ApprovalGate::new()),
            cancels: Arc::new(CancelRegistry::default()),
            provider: tokio::sync::RwLock::new(None),
            settings: tokio::sync::RwLock::new(crate::agent::AgentSettings::default()),
        })
    }
}


// `SkyhookError` is used in trait bounds via Result<_>; this `use` keeps the
// import live even if we ever stop referencing it directly above.
#[allow(dead_code)]
fn _force_use_err(e: SkyhookError) -> SkyhookError {
    e
}
