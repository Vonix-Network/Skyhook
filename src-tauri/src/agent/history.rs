//! Conversation persistence — JSON files under `<config>/skyhook/agent/`.
//!
//! Layout: `<config>/skyhook/agent/<connection_id>/<conversation_id>.json`.
//!
//! Files are written atomically (`*.tmp` + rename) to survive crashes
//! mid-write. The on-disk shape matches Anthropic's content-block model
//! ([`crate::agent::provider::Message`]), keeping a single canonical form
//! regardless of which provider produced the turn.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::fs;
use uuid::Uuid;

use crate::agent::provider::Message;
use crate::error::{Result, SkyhookError};

/// Metadata returned in conversation listings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMeta {
    pub id: String,
    pub connection_id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub provider: String,
    pub model: String,
}

/// Full persisted conversation (meta + message log).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    #[serde(flatten)]
    pub meta: ConversationMeta,
    #[serde(default)]
    pub messages: Vec<Message>,
}

/// On-disk conversation store.
pub struct ConversationStore {
    base_dir: PathBuf,
}

impl ConversationStore {
    /// Open (and create as needed) the conversation directory under the
    /// user's config dir.
    pub fn new() -> Result<Self> {
        let base = dirs::config_dir()
            .ok_or_else(|| SkyhookError::Other("no config dir".into()))?
            .join("skyhook")
            .join("agent");
        std::fs::create_dir_all(&base)?;
        Ok(Self { base_dir: base })
    }

    /// Open a store at an arbitrary directory (used by tests).
    #[allow(dead_code)]
    pub fn with_base(base: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&base)?;
        Ok(Self { base_dir: base })
    }

    fn conn_dir(&self, connection_id: &str) -> PathBuf {
        self.base_dir.join(connection_id)
    }

    #[allow(dead_code)]
    fn conv_path(&self, connection_id: &str, conversation_id: &str) -> PathBuf {
        self.conn_dir(connection_id)
            .join(format!("{conversation_id}.json"))
    }

    /// List every saved conversation (meta only) for `connection_id`, newest
    /// `updated_at` first.
    pub async fn list(&self, connection_id: &str) -> Result<Vec<ConversationMeta>> {
        let dir = self.conn_dir(connection_id);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut rd = fs::read_dir(&dir).await?;
        let mut out = Vec::new();
        while let Some(entry) = rd.next_entry().await? {
            let p = entry.path();
            if p.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            if let Ok(bytes) = fs::read(&p).await {
                if let Ok(conv) = serde_json::from_slice::<Conversation>(&bytes) {
                    out.push(conv.meta);
                }
            }
        }
        out.sort_by_key(|b| std::cmp::Reverse(b.updated_at));
        Ok(out)
    }

    /// Load a conversation by id. Scans connection subdirectories.
    pub async fn load(&self, conversation_id: &str) -> Result<Conversation> {
        let path = self.find_path(conversation_id).await?;
        let bytes = fs::read(&path).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Persist `conv` atomically.
    pub async fn save(&self, conv: &Conversation) -> Result<()> {
        let dir = self.conn_dir(&conv.meta.connection_id);
        fs::create_dir_all(&dir).await?;
        let final_path = dir.join(format!("{}.json", conv.meta.id));
        let tmp_path = dir.join(format!("{}.json.tmp", conv.meta.id));
        let bytes = serde_json::to_vec_pretty(conv)?;
        fs::write(&tmp_path, bytes).await?;
        fs::rename(&tmp_path, &final_path).await?;
        Ok(())
    }

    /// Delete a conversation by id.
    pub async fn delete(&self, conversation_id: &str) -> Result<()> {
        let path = self.find_path(conversation_id).await?;
        fs::remove_file(path).await?;
        Ok(())
    }

    /// Create + persist a new empty conversation.
    pub async fn create(
        &self,
        connection_id: String,
        title: String,
        provider: String,
        model: String,
    ) -> Result<Conversation> {
        let now = chrono::Utc::now().timestamp();
        let conv = Conversation {
            meta: ConversationMeta {
                id: Uuid::new_v4().to_string(),
                connection_id,
                title,
                created_at: now,
                updated_at: now,
                provider,
                model,
            },
            messages: Vec::new(),
        };
        self.save(&conv).await?;
        Ok(conv)
    }

    /// Update the title of an existing conversation. Bumps `updated_at`.
    pub async fn rename(&self, conversation_id: &str, title: String) -> Result<()> {
        let mut conv = self.load(conversation_id).await?;
        conv.meta.title = title;
        conv.meta.updated_at = chrono::Utc::now().timestamp();
        self.save(&conv).await
    }

    /// Locate the file path for `conversation_id` across all connection dirs.
    async fn find_path(&self, conversation_id: &str) -> Result<PathBuf> {
        let target = format!("{conversation_id}.json");
        let mut rd = fs::read_dir(&self.base_dir).await?;
        while let Some(entry) = rd.next_entry().await? {
            let sub = entry.path();
            if !sub.is_dir() {
                continue;
            }
            let candidate = sub.join(&target);
            if fs::try_exists(&candidate).await.unwrap_or(false) {
                return Ok(candidate);
            }
        }
        Err(SkyhookError::Other(format!(
            "conversation not found: {conversation_id}"
        )))
    }

    /// Borrow the on-disk base directory (e.g. for diagnostics / tests).
    #[allow(dead_code)]
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }
}
