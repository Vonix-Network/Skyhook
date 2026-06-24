# Skyhook

A modern, fast SFTP client. Cross-platform desktop app built on Tauri 2 + Rust + React.

> **Status:** v0.1 — early. Connection vault, dual-pane browser, transfer queue, multi-session tabs all working. Mount-as-drive, Monaco editor, and integrated SSH terminal coming next.

## Why another SFTP client

WinSCP and FileZilla look like Windows 7. Cyberduck is OK but slow. Termius is a subscription. Mountain Duck is closed-source and expensive. We can do better — a clean, native-feeling, keyboard-first client with a real connection vault, that ships as a 15 MB binary instead of 200 MB.

## Architecture

```
┌─────────────────────────────────────────────────┐
│  React + TypeScript UI (Vite, zustand, lucide)  │
├─────────────────────────────────────────────────┤
│  Tauri IPC bridge                                │
├─────────────────────────────────────────────────┤
│  Rust core                                       │
│   • russh   — pure-Rust SSH client               │
│   • russh-sftp — SFTP v3                         │
│   • aes-gcm + argon2id — encrypted vault         │
│   • keyring — OS keystore for master key         │
└─────────────────────────────────────────────────┘
```

- **No OpenSSL.** All crypto is pure Rust (russh + RustCrypto).
- **No bundled Chromium.** Tauri uses the OS webview — typical install is 10–20 MB.
- **Connection secrets** are stored locally in `vault.bin`, encrypted with AES-256-GCM. The master key lives in the OS keyring (macOS Keychain, GNOME Keyring, Windows Credential Manager).

## Features

### v0.1 (current)
- 🔑 Encrypted connection vault (password / private key / SSH agent)
- 📂 File browser with history, breadcrumbs, sortable columns, multi-select
- 📑 Multiple concurrent sessions in tabs
- ⬆️⬇️ Upload / download via native file picker
- 📋 Live transfer queue
- ➕ Make directory, rename, delete
- 🌓 Clean dark UI

### v0.2 (planned)
- ✏️ Inline file editor (Monaco — same engine as VS Code)
- 🖥️ Integrated SSH terminal tab per session (xterm.js)
- 🔍 Recursive search
- 🧭 Trust-on-first-use known_hosts
- 📐 Side-by-side local/remote drag-drop

### v1.0 (planned)
- 🔌 Mount-as-drive (FUSE / Dokan / macFUSE)
- 🔁 Watch-and-sync folders
- ☁️ Optional cloud-synced bookmark vault

## Building from source

### Prerequisites
- Rust ≥ 1.77 (`curl https://sh.rustup.rs -sSf | sh`)
- Node ≥ 18 and pnpm 9 (`npm i -g pnpm@9`)
- Linux: `libwebkit2gtk-4.1-dev libsoup-3.0-dev libgtk-3-dev libssl-dev libxdo-dev librsvg2-dev`

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

## License
MIT — © Vonix.Network contributors. Built to be sold day-one.
