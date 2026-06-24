# Changelog

All notable changes to Skyhook will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/Vonix-Network/Skyhook/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/Vonix-Network/Skyhook/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/Vonix-Network/Skyhook/releases/tag/v0.1.0
