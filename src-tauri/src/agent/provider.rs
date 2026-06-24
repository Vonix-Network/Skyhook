//! Provider trait and shared types for AI chat providers.
//!
//! This module defines the abstract `Provider` interface implemented by
//! concrete backends such as Anthropic and OpenAI, along with the canonical
//! message / streaming-event shapes used throughout the agent stack.

use async_trait::async_trait;
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

use crate::error::SkyhookError;

/// One piece of structured content inside a `Message`.
///
/// The variants mirror Anthropic's content-block model — this is our canonical
/// on-disk representation; OpenAI history is converted into this shape on read.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageContent {
    /// Plain text content from the user or assistant.
    Text { text: String },
    /// An assistant-issued tool invocation.
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// A user-side message carrying the result of a previous tool call.
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
    /// Extended-thinking / reasoning content (Claude Sonnet 4.5+, o-series).
    Thinking { thinking: String },
}

/// Role of a single message in the conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
}

/// A conversation message (canonical Anthropic-style shape).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<MessageContent>,
    #[serde(default)]
    pub timestamp: i64,
    #[serde(default)]
    pub usage: Option<Usage>,
}

/// Token-usage counters returned by a provider after a streamed turn.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_creation_tokens: u32,
}

/// A tool exposed to the model.
///
/// `input_schema` is a JSON-Schema object describing the tool's arguments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Incremental events emitted by a provider during a streamed completion.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// First event of a stream — the provider has accepted the request.
    MessageStart,
    /// A chunk of assistant-visible text.
    TextDelta(String),
    /// A chunk of extended-thinking / reasoning text.
    ThinkingDelta(String),
    /// The model began a tool invocation; `id` and `name` are now known.
    ToolUseStart { id: String, name: String },
    /// A partial JSON fragment for the tool's `input` field.
    ToolUseInputDelta { id: String, partial_json: String },
    /// The tool-use content block has been fully streamed.
    ToolUseEnd { id: String },
    /// Final event of the stream with stop reason and aggregate token usage.
    MessageEnd { stop_reason: String, usage: Usage },
    /// Terminal error emitted by the provider.
    Error(String),
}

/// One completion request, provider-agnostic.
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    /// Fully rendered system-prompt text (cacheable prefix).
    pub system: String,
    /// Prior conversation, in chronological order.
    pub messages: Vec<Message>,
    /// Tools the model may call.
    pub tools: Vec<ToolSchema>,
    /// Provider-specific model id.
    pub model: String,
    /// Maximum tokens to generate.
    pub max_tokens: u32,
}

/// Boxed, pinned stream of `StreamEvent`s — the return type of `stream_chat`.
pub type EventStream = Pin<Box<dyn Stream<Item = StreamEvent> + Send>>;

/// Abstract chat provider (Anthropic, OpenAI, …).
#[async_trait]
pub trait Provider: Send + Sync {
    /// Short, stable identifier — e.g. `"anthropic"` or `"openai"`.
    fn name(&self) -> &'static str;

    /// Issue a streaming chat-completion request.
    async fn stream_chat(&self, req: CompletionRequest) -> Result<EventStream, SkyhookError>;

    /// Return the list of model ids this provider exposes.
    async fn list_models(&self) -> Result<Vec<String>, SkyhookError>;
}
