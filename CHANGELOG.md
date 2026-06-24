# Changelog

All notable changes to Skyhook will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/Vonix-Network/Skyhook/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/Vonix-Network/Skyhook/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/Vonix-Network/Skyhook/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/Vonix-Network/Skyhook/releases/tag/v0.1.0
