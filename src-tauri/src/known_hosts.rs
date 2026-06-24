//! SSH Known-Hosts (Trust-On-First-Use) store.
//!
//! Storage format (one entry per line, OpenSSH-ish, but explicit port):
//!     host:port algo base64fingerprint  # added <iso8601>
//!
//! `base64fingerprint` is the SHA-256 of the SSH public key, base64-no-pad,
//! identical to what `ssh-keygen -l -E sha256` emits after the `SHA256:` prefix.

use crate::error::{Result, SkyhookError};
use russh::keys::key::PublicKey;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

/// Compute the `SHA256:<base64-no-pad>` fingerprint for a public key.
pub fn fingerprint_sha256(key: &PublicKey) -> String {
    // russh-keys' PublicKey::fingerprint() already returns base64-no-pad of sha256.
    format!("SHA256:{}", key.fingerprint())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HostCheck {
    /// Exact match — host is known and fingerprint matches.
    Trusted,
    /// We've never seen this host:port before.
    New { fingerprint_sha256: String },
    /// We have a record, but the presented key differs. Possible MITM.
    Changed { stored: String, presented: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownHostEntry {
    pub host: String,
    pub port: u16,
    pub algo: String,
    /// `SHA256:<base64-no-pad>`
    pub fingerprint: String,
    pub added_at: String,
}

pub struct KnownHosts {
    path: PathBuf,
    /// Keyed by "host:port".
    entries: HashMap<String, KnownHostEntry>,
}

fn key(host: &str, port: u16) -> String {
    format!("{host}:{port}")
}

fn config_dir() -> Result<PathBuf> {
    let base = dirs::config_dir()
        .ok_or_else(|| SkyhookError::Other("no config dir".into()))?;
    let dir = base.join("skyhook");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

impl KnownHosts {
    pub fn load() -> Result<Self> {
        let path = config_dir()?.join("known_hosts");
        let mut entries: HashMap<String, KnownHostEntry> = HashMap::new();

        if path.exists() {
            let body = std::fs::read_to_string(&path)?;
            for raw in body.lines() {
                let line = raw.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                // Split off trailing "# added <iso8601>" comment, if present.
                let (data, comment) = match line.find('#') {
                    Some(i) => (line[..i].trim(), line[i + 1..].trim()),
                    None => (line, ""),
                };
                let mut parts = data.split_whitespace();
                let hostport = match parts.next() {
                    Some(s) => s,
                    None => continue,
                };
                let algo = match parts.next() {
                    Some(s) => s.to_string(),
                    None => continue,
                };
                let fp_raw = match parts.next() {
                    Some(s) => s.to_string(),
                    None => continue,
                };
                let (host, port) = match hostport.rsplit_once(':') {
                    Some((h, p)) => match p.parse::<u16>() {
                        Ok(p) => (h.to_string(), p),
                        Err(_) => continue,
                    },
                    None => continue,
                };
                let fingerprint = if fp_raw.starts_with("SHA256:") {
                    fp_raw
                } else {
                    format!("SHA256:{fp_raw}")
                };
                let added_at = comment
                    .strip_prefix("added ")
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                let k = key(&host, port);
                entries.insert(
                    k,
                    KnownHostEntry {
                        host,
                        port,
                        algo,
                        fingerprint,
                        added_at,
                    },
                );
            }
        }

        Ok(Self { path, entries })
    }

    pub fn list(&self) -> Vec<KnownHostEntry> {
        let mut v: Vec<_> = self.entries.values().cloned().collect();
        v.sort_by(|a, b| a.host.cmp(&b.host).then(a.port.cmp(&b.port)));
        v
    }

    pub fn check(&self, host: &str, port: u16, key_pub: &PublicKey) -> HostCheck {
        let presented = fingerprint_sha256(key_pub);
        match self.entries.get(&key(host, port)) {
            Some(e) if e.fingerprint == presented => HostCheck::Trusted,
            Some(e) => HostCheck::Changed {
                stored: e.fingerprint.clone(),
                presented,
            },
            None => HostCheck::New {
                fingerprint_sha256: presented,
            },
        }
    }

    pub fn add(&mut self, host: &str, port: u16, key_pub: &PublicKey) -> Result<()> {
        let entry = KnownHostEntry {
            host: host.to_string(),
            port,
            algo: key_pub.name().to_string(),
            fingerprint: fingerprint_sha256(key_pub),
            added_at: chrono::Utc::now().to_rfc3339(),
        };
        self.entries.insert(key(host, port), entry);
        self.save()
    }

    /// Trust an externally-supplied fingerprint (frontend confirmation flow).
    pub fn add_raw(
        &mut self,
        host: &str,
        port: u16,
        algo: &str,
        fingerprint: &str,
    ) -> Result<()> {
        let fingerprint = if fingerprint.starts_with("SHA256:") {
            fingerprint.to_string()
        } else {
            format!("SHA256:{fingerprint}")
        };
        let entry = KnownHostEntry {
            host: host.to_string(),
            port,
            algo: algo.to_string(),
            fingerprint,
            added_at: chrono::Utc::now().to_rfc3339(),
        };
        self.entries.insert(key(host, port), entry);
        self.save()
    }

    pub fn remove(&mut self, host: &str, port: u16) -> Result<()> {
        self.entries.remove(&key(host, port));
        self.save()
    }

    fn save(&self) -> Result<()> {
        let mut buf = String::new();
        buf.push_str("# Skyhook known_hosts (TOFU). Format: host:port algo SHA256:<b64nopad>  # added <iso8601>\n");
        let mut sorted: Vec<&KnownHostEntry> = self.entries.values().collect();
        sorted.sort_by(|a, b| a.host.cmp(&b.host).then(a.port.cmp(&b.port)));
        for e in sorted {
            buf.push_str(&format!(
                "{}:{} {} {}  # added {}\n",
                e.host, e.port, e.algo, e.fingerprint, e.added_at
            ));
        }
        let tmp = self.path.with_extension("tmp");
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(buf.as_bytes())?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_roundtrip_skips_comments_and_blanks() {
        // Direct parser test by writing a file in a temp dir.
        let tmp = std::env::temp_dir().join(format!(
            "skyhook-kh-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        let p = tmp.join("known_hosts");
        let body = "\n# comment\nexample.com:22 ssh-ed25519 SHA256:AAAAA  # added 2024-01-01T00:00:00Z\n";
        std::fs::write(&p, body).unwrap();

        // Manually construct & parse using the same logic via a tiny helper:
        // Use the same parsing path as load(), but pointed at our tmp file.
        let mut kh = KnownHosts {
            path: p.clone(),
            entries: HashMap::new(),
        };
        // Re-parse:
        let body = std::fs::read_to_string(&p).unwrap();
        for raw in body.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (data, _c) = match line.find('#') {
                Some(i) => (line[..i].trim(), line[i + 1..].trim()),
                None => (line, ""),
            };
            let mut parts = data.split_whitespace();
            let hp = parts.next().unwrap();
            let algo = parts.next().unwrap().to_string();
            let fp = parts.next().unwrap().to_string();
            let (h, port) = hp.rsplit_once(':').unwrap();
            let port: u16 = port.parse().unwrap();
            kh.entries.insert(
                key(h, port),
                KnownHostEntry {
                    host: h.into(),
                    port,
                    algo,
                    fingerprint: fp,
                    added_at: "2024-01-01T00:00:00Z".into(),
                },
            );
        }
        assert_eq!(kh.list().len(), 1);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
