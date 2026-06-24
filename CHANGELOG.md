# Changelog

## [Unreleased]
### Added
- Initial scaffold: Tauri 2 + React/TS + Rust backend.
- Encrypted connection vault (AES-256-GCM + Argon2id; master key in OS keyring).
- SFTP backend via russh 0.46 + russh-sftp 2.3 (password / key / agent auth).
- React UI: sidebar, multi-session tabs, dual-history file browser, transfer queue, connection editor.
- CI workflow building on Ubuntu / macOS / Windows.
