# Changelog

All notable changes to Skyhook will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.6.0] – 2026-06-24

### Added

#### Integrated AI agent (headline feature)
Skyhook now ships with a first-class AI agent that drives the existing SFTP +
SSH primitives. The agent lives in a right-docked panel and can read, write,
upload, download, and execute on the connected remote server.

- **Providers**: Anthropic Claude (Messages API) and OpenAI (Chat Completions),
  selectable per conversation.
- **Prompt caching**: Anthropic uses a 4-breakpoint strategy (system block +
  last tool def + earliest user turn when conv ≥ 4 messages + last message)
  for ~75% input-token savings on long conversations. OpenAI uses
  `prompt_cache_key` over the static prefix.
- **Tools** (route through `SessionHandle`, so they hit the live SSH+SFTP
  session): `sftp_list_dir`, `sftp_read_file`, `sftp_stat`, `sftp_walk`,
  `sftp_write_file`, `sftp_make_dir`, `sftp_remove`, `sftp_rename`,
  `sftp_download`, `sftp_upload`, `shell_exec` (ephemeral PTY with exit-
  code sentinel), `task_complete`.
- **Approval modes** per connection: Manual (every write/exec confirmed),
  Auto-read (reads free, writes confirmed; the default), Yolo (everything
  flows; requires typed `YOLO` confirmation to enable).
- **Streaming UI**: real-time text/thinking deltas, tool-call cards with
  collapsible args + output, ApprovalCard with **Monaco inline diff** for
  `sftp_write_file` (fetches current remote contents and diffs against the
  proposed new content).
- **Conversations**: persistent per connection at
  `<config>/skyhook/agent/<connection_id>/<conversation_id>.json`, list +
  rename + delete + new from the sidebar.
- **Token usage** (input / output / cache-read / cache-creation) shown
  after each turn.
- **Extended thinking** support for Sonnet 4.5+ when enabled.
- **Cancel-in-flight** turn via the panel header.

#### Backend infrastructure
- `src/agent/` module: `Provider` trait, `AnthropicProvider`, `OpenAIProvider`,
  `Keystore` (OS keyring, service `skyhook`, entry `agent-key-<provider>`),
  `Tools` enum + JSON-Schema, `ApprovalGate` (oneshot-per-call), `ConversationStore`
  (atomic JSON writes), `AgentRunner` (streaming/tool loop), `AgentRuntime`
  (Tauri-managed state).
- 15 new Tauri commands: `agent_list_models`, `agent_*_conversation` (5),
  `agent_send_message`, `agent_cancel`, `agent_approve_tool`,
  `agent_reject_tool`, `agent_*_api_key` (3), `agent_get_settings`,
  `agent_save_settings`.
- 7 new events: `agent-message-delta`, `agent-thinking-delta`,
  `agent-tool-call`, `agent-tool-approval`, `agent-tool-result`,
  `agent-turn-end`, `agent-error`.

#### Frontend
- `lib/agent-store.ts` (Zustand) holding active conversation, streaming
  buffers, pending approval, usage.
- `components/Agent/`: `AgentPanel`, `MessageList`, `Message`, `Composer`,
  `ConversationList`, `AgentSettings`, `ToolCallCard`, `ApprovalCard`,
  `DiffPreview`.
- Sidebar gains a per-connection approval-mode override (Shield icon →
  Default / Manual / Auto-read / Yolo, persisted in localStorage).
- Settings modal adds an "Agent" section that opens the Agent Settings
  modal (API keys, default provider, model dropdowns, max turns, default
  approval mode, reasoning effort for o-series, show-thinking toggle).
- Browser toolbar gains a "Bot" toggle button for the Agent panel.

### Changed
- New deps: `reqwest 0.12` (rustls-tls + json + stream),
  `eventsource-stream 0.2`, `futures-util 0.3`.
- AppState gains `agent: Arc<AgentRuntime>`.

### Migration notes
- API keys stored in the OS keyring under service `skyhook`, entry
  `agent-key-anthropic` / `agent-key-openai`. Never written to disk in
  plaintext.
- Conversation history lives at `<config>/skyhook/agent/` and is
  per-connection.

## [0.3.0] – 2026-06-24

### Added

#### SSH terminal (headline)
- **Integrated PTY shell** per session (xterm.js + russh PTY channel). Toggle from the Browser toolbar. Cyan-on-dark theme matching the rest of the app.
- **Multiple shells per connection** — each runs an independent task on its own SSH channel, completely decoupled from the SFTP op queue (terminal input doesn't block file ops and vice-versa).
- **Resize forwarding** — terminal resize → SSH WINDOW_CHANGE via `request_pty` + `window_change`.
- **Copy/paste** — Ctrl+Shift+C / Ctrl+Shift+V on Linux/Win, Cmd+C / Cmd+V on macOS.
- **Exit-code rendering** — `[Process exited with code N]` printed at end-of-session.
- **Lifecycle**: shells force-close cleanly when the parent session goes Degraded or Closed (transparent resume across SSH reconnect is intentionally not attempted; users open a fresh shell).

#### Transfer engine + UX
- **Live throughput** (`throughput_bps`) — EMA-smoothed (α=0.3) bytes/sec, emitted on every progress tick.
- **ETA** (`eta_seconds`) — `(total - bytes) / throughput`, capped at 99 h.
- **Stall heartbeat** — when bytes stop moving for 5 s on an active transfer, the engine emits a single progress event with throughput=0 so the UI can render the stall.
- **Production-grade TransferPanel rewrite**: real progress bars (cyan fill, animated diagonal stripe when active), per-row Pause/Resume/Cancel/Retry/Dismiss, header aggregate (`N active • M queued • K completed`), Clear-completed button, sorted by activity, accessibility (`role=progressbar`, aria-valuenow), empty-state copy.

#### Production polish
- **Window state persistence** — main window size, position, maximized state save on Resize/Move/CloseRequested with a 500 ms debounce; restored on next launch.
- **Last-active connection auto-focus** — settings track the most-recently-active connection; on startup the frontend auto-opens it.
- **Connection import/export** — versioned JSON bundle (`{ version: 1, exported_at, connections: [...] }`) **without secrets**. Each import gets a fresh UUID; duplicate `(host, port, username, name)` rows are skipped. New commands: `export_connections`, `import_connections`.
- **Resizable sidebar + transfer panel** — drag the gutter, sizes persist to settings.
- **Properties modal** — right-click → Properties opens a real modal (Type / Path / Size / Modified / Permissions in rwx + octal), replacing the previous `alert()`.
- **EmptyState component** — illustrated empty-directory message replacing the placeholder text.
- **Shortcuts overlay** — press `?` from anywhere to see all keyboard shortcuts, grouped by section (Global / Browser / Editor / Terminal / Transfers).
- **Sidebar import/export buttons** with native file dialogs.

#### Backend infrastructure
- `read_local_text_file` / `write_local_text_file` Tauri commands for the import/export flow.
- `save_window_state` command for explicit save-before-quit fallbacks.
- `set_last_active_connection` command writing through to settings.

### Changed
- `Transfer` struct gains `throughput_bps: f64` and `eta_seconds: Option<u64>`. Frontend types updated accordingly.
- `subscribeBackendEvents` pipes throughput/ETA into the store.
- Sidebar settings cog now opens the Settings modal (was a no-op).

### Fixed
- `save_window_state` was implemented but missing from `invoke_handler` in the wave-1 backend; wired in the integration step so the command actually works.

### Migration notes
- No on-disk schema changes; the new `throughput_bps`/`eta_seconds` fields are optional (`serde(default)`) so older snapshots round-trip cleanly.

## [0.2.0] – 2026-06-24

### Added

#### Session management (production-grade rewrite)
- **Actor-pattern SFTP backend**: each connection now runs as a Tokio task
  (`SessionActor`) owning the SSH transport and SFTP channel exclusively. All
  ops flow through an mpsc op queue, eliminating lock-ordering races and
  zombie sessions.
- **Explicit state machine**: `Connecting → Connected → Degraded → Closed`
  with deterministic transitions. State is emitted to the frontend via the
  `session-state-changed` Tauri event.
- **Heartbeat liveness probe**: every 30 s the actor `stat`s the cwd. Two
  consecutive failures flip the session to `Degraded` and emit
  `session-heartbeat`.
- **Auto-reconnect with backoff**: 1 s → 2 s → 5 s on `Degraded`. On success
  returns to `Connected`; on exhaustion transitions to `Closed`.
- **Per-connection deduplication**: `SessionManager::connect()` returns the
  existing session if one already exists for the connection id. Clicking
  Connect repeatedly can no longer stack SFTP sessions and trip Wings'
  per-user session cap.
- **Concurrent-op safety**: many frontend calls fan into the actor's mpsc
  queue and execute serially on the single SFTP channel.

#### Transfer engine
- **TransferEngine** with bounded concurrency (`MAX_CONCURRENT = 2` for now,
  exposed via Settings later).
- **Recursive folder upload** (local walk via `std::fs::read_dir`).
- **Recursive folder download** (remote walk via the new `walk` command).
- **Pause / Resume / Cancel** via `AtomicBool` cooperative checks.
- **`transfer-progress` events** for the frontend's live UI.

#### Security
- **Known-hosts TOFU**: `<config>/skyhook/known_hosts` stores
  `host:port algo SHA256:<fingerprint>` in OpenSSH-compatible form. Atomic
  writes. New commands: `known_hosts_list`, `known_hosts_remove`,
  `known_hosts_trust`.
- **Settings persistence**: `<config>/skyhook/settings.json` with backward-
  compatible serde defaults. Atomic writes. New commands: `get_settings`,
  `save_settings`.

#### Editor
- **Monaco editor tab-in-tab**: double-click a text file to edit in place.
  `Ctrl/Cmd+S` saves via SFTP. Dirty indicator in the tab and a status line
  showing line:col, encoding, and language. 10 MB hard cap. Auto-detects
  language by extension.

#### File browser polish
- **Right-click context menus** on file rows (Open / Edit / Download /
  Rename / Delete / Copy path / Properties) and empty space (Refresh / New
  folder / Upload here).
- **Inline rename** (F2, Enter to commit, Esc to cancel).
- **Keyboard nav**: arrows move selection, Enter opens, Backspace goes up,
  Del deletes (configurable confirm), F5 refresh, Ctrl+L focuses path bar.
- **Multi-select**: Shift+click for ranges, Ctrl/Cmd+click to toggle.
- **OS drag-drop**: dropping files/folders from Explorer/Finder uploads them
  to the current remote directory (via Tauri `tauri://drag-drop`).
- **Sortable columns** with direction chevron; folders always first.
- **Hidden-file filter** controlled by Settings.

#### Cross-cutting UI
- **Toast system**: bottom-right stack with slide-in, lucide-react icons,
  auto-dismiss (4 s info/success/warning, 8 s error). All previous `alert()`
  paths replaced.
- **Settings modal**: Appearance (theme), Behavior (confirm-on-delete,
  show-hidden), Transfers (concurrency), Editor (word-wrap).
- **About modal**: version, MIT license, GitHub link.
- **Known-hosts manager**: table of trusted hosts with per-row remove.
- **TabBar**: middle-click closes a tab; hover reveals the close button.

#### Tests + quality
- **12 backend unit tests** covering path normalization (the v0.1.1 mkdir
  fix), fatal-error detection, settings round-trip, known-hosts parsing.
- Zero `unwrap()` outside test code in new modules.
- All public types documented; actor pattern + state machine explained in
  module-level doc comments.

### Changed
- `SessionStatus` replaced by `SessionInfo` (adds explicit `state`,
  `last_error`, `connected_at`). Frontend `Tab` type now carries the
  lifecycle `state` enum, not just a boolean.
- Frontend store now subscribes to backend events rather than polling
  session_status. Faster + correct.
- `SessionManager::connect` is idempotent per connection_id.

### Fixed
- Zombie tabs after channel death are now surfaced as `Degraded`/`Closed`
  with a Reconnect path.
- Concurrent SFTP ops on the same session race-free (queued in the actor).

### Migration notes
- `~/.config/skyhook/vault.bin` is unchanged from 0.1.x.
- New files `~/.config/skyhook/{known_hosts,settings.json}` are created on
  first run.

## [0.1.1] – 2026-06-24

### Added
- **Session deduplication.** Clicking Connect on a connection that is already
  open now focuses the existing tab instead of opening a second SFTP session.
  Wings/Pterodactyl SFTP caps concurrent sessions per user and kills the channel
  when the cap is exceeded — this prevents that footgun.
- **Reconnect banner.** When the backend detects the SSH channel has died
  (e.g. `connection closed`, `EOF`), the affected tab shows a red banner with a
  Reconnect button and the connection dot in the tab bar turns red.
- **Path normalization.** All remote paths sent over SFTP now collapse
  duplicate slashes and strip trailing slashes (except root). Wings rejects
  `//foo` and closes the channel; OpenSSH tolerates it. Be safe everywhere.

### Fixed
- **`mkdir` killing the SFTP session** when invoked at the SFTP jail root.
  Caused by sending `//foldername` (jail root `/` + `/` + name).
- **Silent zombie tabs** when an SFTP op killed the channel. Now surfaced as
  a clear "Session closed by server" banner with a Reconnect button.

### Changed
- `SessionStatus.connected` now reflects real channel health, not a constant
  `true`. The tab connection dot changes color accordingly.
- Bumped frontend, Cargo, and `tauri.conf.json` to `0.1.1`.

## [0.1.0] – 2026-06-24

### Added
- Initial scaffold: Tauri 2 + React/TypeScript + Rust backend.
- Encrypted connection vault (AES-256-GCM + Argon2id; master key stored in OS
  keyring — macOS Keychain, GNOME Keyring, Windows Credential Manager).
- SFTP backend via russh 0.46 + russh-sftp 2.3 (password / key / agent auth).
- React UI: sidebar, multi-session tabs, dual-history file browser, transfer
  queue, connection editor.
- CI workflow building on Ubuntu / macOS / Windows.
- Release workflow producing `.deb`, `.AppImage`, `.dmg`, `.msi`, and NSIS
  installers per tag.

### Fixed
- Windows build failure on `cargo check`: `AgentClient::connect_env` is Unix-
  only. Gated the SSH-agent auth branch behind `#[cfg(unix)]` and returned a
  clear error on Windows.

[Unreleased]: https://github.com/Vonix-Network/Skyhook/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/Vonix-Network/Skyhook/compare/v0.3.0...v0.6.0
[0.3.0]: https://github.com/Vonix-Network/Skyhook/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/Vonix-Network/Skyhook/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/Vonix-Network/Skyhook/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/Vonix-Network/Skyhook/releases/tag/v0.1.0
