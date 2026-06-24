# Skyhook

> A modern, fast SFTP client. Cross-platform desktop app built on Tauri 2 + Rust + React.

[![Release](https://img.shields.io/github/v/release/Vonix-Network/Skyhook?style=flat-square)](https://github.com/Vonix-Network/Skyhook/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](LICENSE)
[![CI](https://img.shields.io/github/actions/workflow/status/Vonix-Network/Skyhook/ci.yml?branch=main&style=flat-square)](https://github.com/Vonix-Network/Skyhook/actions)
[![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%202-24C8DB?style=flat-square)](https://tauri.app)

**Status:** v0.3.0 — production-ready. SFTP, integrated SSH terminal, file editor, transfer queue with live throughput, connection import/export, polished UI. Mount-as-drive and watch-and-sync on the v1.0 roadmap.

## Why another SFTP client

WinSCP and FileZilla look like Windows 7. Cyberduck is OK but slow. Termius is a subscription. Mountain Duck is closed-source and expensive. There's room for a clean, native-feeling, keyboard-first SFTP client with a real connection vault — that ships as a small native binary and stays out of your way.

## Download

Pre-built installers are attached to every tagged release.

| Platform | Installer |
|---|---|
| Windows 10/11 (x64) | `Skyhook_<ver>_x64-setup.exe` (NSIS) or `Skyhook_<ver>_x64_en-US.msi` |
| macOS (Universal — Intel + Apple Silicon) | `Skyhook_<ver>_x64.dmg` |
| Linux (x86_64) | `Skyhook_<ver>_amd64.AppImage` or `Skyhook_<ver>_amd64.deb` |

Grab the latest from the [Releases page](https://github.com/Vonix-Network/Skyhook/releases/latest).

> ⚠ The Windows installer is **not yet code-signed** — SmartScreen may warn on first run. Choose "More info → Run anyway". A signed build is planned once a cert is purchased.

## Features

### Shipped (v0.3.0)

**Connections & sessions**
- 🔑 Encrypted connection vault — AES-256-GCM + Argon2id, master key in the OS keyring (Keychain / GNOME Keyring / Windows Credential Manager).
- 🔐 Three auth methods: password, private key (with optional passphrase), SSH agent (Unix).
- 🛰️ Multi-session tabs — one click per server, focus dedup so the same connection never opens twice.
- 🔄 Session state machine — `Connecting → Connected → Degraded → Closed`, with a 30s heartbeat probe and automatic reconnect (1s/2s/5s backoff).
- 🛡️ Trust-on-first-use known_hosts (SHA-256 fingerprint storage, OpenSSH-compatible format).
- 📤 Connection import/export as versioned JSON (no secrets — credentials re-entered after import).

**SSH terminal**
- 💻 Integrated PTY shell per session (xterm.js).
- 🔀 Multiple concurrent shells per connection on independent SSH channels.
- 📐 Resize forwarding (SSH WINDOW_CHANGE).
- 📋 Copy/paste (Ctrl+Shift+C/V on Linux/Win, Cmd+C/V on macOS).

**File browser**
- 📂 History (back/forward/up), breadcrumb path bar with editable navigation.
- 🖱️ Right-click context menus, F2 inline rename, Del to delete (configurable confirm), F5 refresh, Ctrl+L focus path.
- ⬆️⬇️ Native file picker upload/download.
- 📦 OS drag-drop upload (drop files or folders from Explorer/Finder).
- 🗂️ Sortable columns (Name / Size / Modified / Perms), folders always first.
- 🙈 Hidden-file toggle.
- ⌨️ Keyboard-first nav: arrows, Enter, Backspace, Shift/Ctrl+click for multi-select.
- 🪟 Resizable sidebar + transfer panel.

**Transfers**
- 📋 Live transfer queue with state events (queued / active / paused / done / cancelled / error).
- ⏸️ Pause / Resume / Cancel / Retry per row.
- 📁 Recursive folder upload + download.
- 🔢 Configurable concurrency (default 2 in flight).
- 📊 Live throughput (MB/s) + ETA, with stall detection.

**Editor**
- ✏️ Inline Monaco editor — same engine as VS Code. Double-click any text file.
- 🎨 Syntax highlighting auto-detected by extension (JSON, YAML, TOML, properties, INI, shell, JS/TS, Python, Markdown, …).
- 💾 Ctrl+S / Cmd+S saves back via SFTP. Dirty indicator in the tab.
- 🚫 10 MB hard cap.

**Cross-cutting UI**
- 🌓 Clean dark UI with a single cyan accent.
- 🔔 Toast notifications.
- ⚙️ Settings (theme, confirm-on-delete, hidden-file default, transfer concurrency, editor word-wrap), persisted to disk.
- ℹ️ About dialog with version + GitHub link.
- 🗃️ Known-hosts manager.
- ⌨️ Press `?` for the shortcuts overlay.
- 📐 Window position/size persistence across launches.
- 🔁 Auto-focus last-used connection on startup.

### Roadmap (v0.4+)
- 🔍 Recursive remote search.
- 📐 Side-by-side local/remote dual-pane view.
- 🔑 In-app SSH keygen.

### Roadmap (v1.0)
- 🔌 Mount-as-drive (FUSE / Dokan / macFUSE).
- 🔁 Watch-and-sync folders.
- ☁️ Optional cloud-synced bookmarks.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│  React + TypeScript UI                              │
│   • Vite, zustand, Monaco, lucide-react             │
├─────────────────────────────────────────────────────┤
│  Tauri 2 IPC bridge + event bus                     │
│   • session-state-changed, transfer-progress,        │
│     session-heartbeat                               │
├─────────────────────────────────────────────────────┤
│  Rust core                                          │
│   • russh — pure-Rust SSH client                    │
│   • russh-sftp — SFTP v3                            │
│   • Session actor (Tokio task per connection)       │
│     ├─ mpsc op queue (serializes channel access)    │
│     ├─ heartbeat (30s)                              │
│     └─ auto-reconnect with backoff                  │
│   • Transfer engine (Semaphore-gated concurrency)   │
│   • aes-gcm + argon2id encrypted vault              │
│   • keyring — master key in OS keystore             │
│   • OpenSSH-compatible known_hosts                  │
│   • atomic JSON settings persistence                │
└─────────────────────────────────────────────────────┘
```

### Design properties

- **No OpenSSL.** All crypto is pure Rust (russh + RustCrypto).
- **No bundled Chromium.** Tauri uses the OS webview — installs are ~7 MB.
- **One SFTP channel per connection, ever.** The session actor pattern means clicking Connect ten times still produces exactly one live session. Hosts that cap per-user SFTP sessions (Pterodactyl/Wings, etc.) don't get hammered.
- **Connection secrets** are stored locally in `<config>/skyhook/vault.bin`, encrypted with AES-256-GCM. The master key never touches disk — it lives in the OS keyring.
- **Settings + known_hosts** are atomic writes (write-temp, rename) — no torn files on power loss.

## Building from source

### Prerequisites

- Rust ≥ 1.77 — `curl https://sh.rustup.rs -sSf | sh`
- Node ≥ 18 and pnpm 9 — `npm i -g pnpm@9`
- Platform deps:
  - **Linux:** `libwebkit2gtk-4.1-dev libsoup-3.0-dev libgtk-3-dev libssl-dev libxdo-dev librsvg2-dev libayatana-appindicator3-dev`
  - **macOS:** Xcode command-line tools (`xcode-select --install`)
  - **Windows:** Visual Studio 2022 Build Tools (C++ workload) + WebView2 (preinstalled on Win 11)

### Dev

```bash
pnpm install
pnpm tauri:dev
```

### Release build

```bash
pnpm tauri:build
# binary in src-tauri/target/release/skyhook
# installers in src-tauri/target/release/bundle/
```

### Tests

```bash
# Backend unit tests (path normalization, vault crypto, settings round-trip, known_hosts parsing)
cd src-tauri && cargo test

# Frontend type-check
pnpm exec tsc --noEmit
```

## Contributing

PRs welcome. Conventions:

- Follow [Conventional Commits](https://www.conventionalcommits.org/) for messages — the CHANGELOG generator depends on them.
- All notable changes go in [CHANGELOG.md](CHANGELOG.md) under `## [Unreleased]` until the next release.
- This project follows [SemVer 2.0](https://semver.org/). Bug fixes = patch, new backward-compatible features = minor, breaking = major.
- Run `cargo test` and `pnpm exec tsc --noEmit` before opening a PR — CI will block on failures anyway.

## Security

If you find a vulnerability, **please don't open a public issue**. Email `security@vonix.network` or open a private security advisory on GitHub.

Skyhook stores connection credentials encrypted at rest. The threat model assumes a hostile local user can already read your home directory — if they can, no SFTP client can save you. The vault protects against passive disk imaging and casual snooping, not against an attacker with code-exec on your account.

## License

[MIT](LICENSE) — © Vonix.Network contributors.
