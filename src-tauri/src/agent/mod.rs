//! AI agent integration.
//!
//! - [`provider`] defines the abstract `Provider` trait + shared message types.
//! - [`anthropic`] implements `Provider` against Anthropic's Messages API with
//!   streaming SSE and 4-breakpoint prompt caching.
//! - [`system_prompt`] builds the cacheable prompt prefix.
//!
//! Sibling subagents add OpenAI (`openai`, `keystore`) and the tool / runner /
//! approval / history layers.

pub mod anthropic;
pub mod approval;
pub mod history;
pub mod keystore;
pub mod openai;
pub mod provider;
pub mod runner;
pub mod system_prompt;
pub mod tools;

pub use anthropic::AnthropicProvider;
pub use provider::{
    CompletionRequest, EventStream, Message, MessageContent, Provider, Role, StreamEvent,
    ToolSchema, Usage,
};
pub use runner::{AgentRuntime, AgentSettings};
pub use system_prompt::{build_system_prompt, PromptContext};

use std::sync::Arc;
use tokio::sync::RwLock;

/// Top-level agent service holder placed into `AppState`.
///
/// This is a stub during Wave 1 — sibling subagent C fleshes out the runner,
/// history store, and approval gate. Keeping it `Default` + `Clone`-ish lets
/// `lib.rs` instantiate it without depending on the not-yet-written modules.
#[derive(Default)]
pub struct AgentService {
    /// Optional registered provider (Anthropic by default once a key is set).
    pub providers: RwLock<Vec<Arc<dyn Provider>>>,
}

impl AgentService {
    /// Construct an empty service. Providers are registered later.
    pub fn new() -> Self {
        Self::default()
    }
}
