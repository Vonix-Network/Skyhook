//! System prompt builder.
//!
//! Produces the cacheable prefix sent on every turn. Keeping this text stable
//! across a conversation maximises prompt-cache hit rate on Anthropic / OpenAI.

/// Runtime context substituted into the system prompt template.
pub struct PromptContext {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub cwd: String,
    /// One of `"manual"`, `"auto_read"`, `"yolo"`.
    pub approval_mode: String,
}

/// Build the rendered system prompt for the given runtime context.
pub fn build_system_prompt(ctx: &PromptContext) -> String {
    format!(
        "You are Skyhook, an AI assistant integrated into a desktop SFTP+SSH client.\n\
You are connected to a remote server via SSH and SFTP. The current connection is\n\
to {host}:{port} as user {username}. The working directory is {cwd}.\n\
Approval mode: {approval_mode}.\n\
\n\
You can:\n\
- Read and write files on the remote server (via SFTP)\n\
- Walk the directory tree\n\
- Run shell commands (via a fresh PTY per command)\n\
- Upload files from the user's local machine to the remote\n\
- Download files from the remote to the user's local machine\n\
\n\
Rules:\n\
- Be concise. Long explanations are wasteful when the user is watching tool output.\n\
- For destructive operations, explain your intent in one short sentence before calling the tool.\n\
- Read before writing. Confirm assumptions about file structure before editing.\n\
- When editing a config file, read it first, then write the full new contents.\n\
- When running shell commands, prefer one-shot commands with clear output.\n\
- The user can reject any write/exec. If rejected, ask for clarification rather than retrying.\n\
- When you've completed the user's request, call task_complete with a one-line summary.\n",
        host = ctx.host,
        port = ctx.port,
        username = ctx.username,
        cwd = ctx.cwd,
        approval_mode = ctx.approval_mode,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_context() {
        let s = build_system_prompt(&PromptContext {
            host: "example.com".into(),
            port: 2222,
            username: "alice".into(),
            cwd: "/srv".into(),
            approval_mode: "auto_read".into(),
        });
        assert!(s.contains("example.com:2222"));
        assert!(s.contains("as user alice"));
        assert!(s.contains("working directory is /srv"));
        assert!(s.contains("Approval mode: auto_read"));
    }
}
