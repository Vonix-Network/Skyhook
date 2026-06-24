//! Persistent user settings (JSON at `<config>/skyhook/settings.json`).

use crate::error::{Result, SkyhookError};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

pub fn default_theme() -> String {
    "system".into()
}
pub fn default_true() -> bool {
    true
}
pub fn default_concurrency() -> u32 {
    2
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct WindowState {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub x: Option<i32>,
    pub y: Option<i32>,
    #[serde(default)]
    pub maximized: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Settings {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_true")]
    pub confirm_on_delete: bool,
    #[serde(default = "default_true")]
    pub editor_word_wrap: bool,
    #[serde(default = "default_concurrency")]
    pub transfer_concurrency: u32,
    #[serde(default)]
    pub last_active_connection_id: Option<String>,
    #[serde(default)]
    pub window: WindowState,
    #[serde(default = "default_true")]
    pub show_hidden_files: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            confirm_on_delete: default_true(),
            editor_word_wrap: default_true(),
            transfer_concurrency: default_concurrency(),
            last_active_connection_id: None,
            window: WindowState::default(),
            show_hidden_files: default_true(),
        }
    }
}

pub struct SettingsStore {
    path: PathBuf,
    current: Settings,
}

fn config_dir() -> Result<PathBuf> {
    let base = dirs::config_dir().ok_or_else(|| SkyhookError::Other("no config dir".into()))?;
    let dir = base.join("skyhook");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

impl SettingsStore {
    pub fn load() -> Result<Self> {
        let path = config_dir()?.join("settings.json");
        let current = if path.exists() {
            match std::fs::read(&path) {
                Ok(bytes) if !bytes.is_empty() => {
                    // Backward-compat: missing fields fall back to defaults via serde(default).
                    serde_json::from_slice::<Settings>(&bytes).unwrap_or_default()
                }
                _ => Settings::default(),
            }
        } else {
            Settings::default()
        };
        Ok(Self { path, current })
    }

    pub fn get(&self) -> Settings {
        self.current.clone()
    }

    pub fn save(&mut self, s: Settings) -> Result<()> {
        let json = serde_json::to_vec_pretty(&s)?;
        let tmp = self.path.with_extension("json.tmp");
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(&json)?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, &self.path)?;
        self.current = s;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_helpers() {
        let s = Settings::default();
        assert_eq!(s.theme, "system");
        assert!(s.confirm_on_delete);
        assert!(s.editor_word_wrap);
        assert_eq!(s.transfer_concurrency, 2);
        assert!(s.show_hidden_files);
        assert_eq!(s.last_active_connection_id, None);
        assert_eq!(s.window, WindowState::default());
    }

    #[test]
    fn roundtrip_defaults() {
        let s = Settings::default();
        let bytes = serde_json::to_vec(&s).unwrap();
        let back: Settings = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn missing_fields_get_defaults() {
        // Older / partial JSON should not crash.
        let partial = br#"{"theme":"dark"}"#;
        let s: Settings = serde_json::from_slice(partial).unwrap();
        assert_eq!(s.theme, "dark");
        assert!(s.confirm_on_delete);
        assert_eq!(s.transfer_concurrency, 2);
        assert!(s.show_hidden_files);
    }

    #[test]
    fn empty_object_is_full_default() {
        let s: Settings = serde_json::from_slice(b"{}").unwrap();
        assert_eq!(s, Settings::default());
    }
}
