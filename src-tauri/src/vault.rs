use crate::error::{Result, SkyhookError};
use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use argon2::{Argon2, Algorithm, Version, Params};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMethod {
    Password { password: String },
    Key { private_key: String, passphrase: Option<String> },
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
    pub default_path: Option<String>,
    pub color: Option<String>,
    #[serde(default)]
    pub created_at: i64,
}

impl Connection {
    pub fn new(name: String, host: String, port: u16, username: String, auth: AuthMethod) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            host,
            port,
            username,
            auth,
            default_path: None,
            color: None,
            created_at: chrono::Utc::now().timestamp(),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct VaultData {
    #[serde(default)]
    pub connections: Vec<Connection>,
}

pub struct Vault {
    data: VaultData,
    path: PathBuf,
    key: [u8; 32],
}

const VAULT_VERSION: u8 = 1;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;

impl Vault {
    pub fn config_dir() -> Result<PathBuf> {
        let base = dirs::config_dir()
            .ok_or_else(|| SkyhookError::Vault("no config dir".into()))?;
        let dir = base.join("skyhook");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    pub fn load_or_default() -> Result<Self> {
        let path = Self::config_dir()?.join("vault.bin");
        // Master key: stored in OS keyring, generated on first run
        let key = Self::load_or_create_master_key()?;
        let data = if path.exists() {
            let blob = std::fs::read(&path)?;
            Self::decrypt(&blob, &key).unwrap_or_default()
        } else {
            VaultData::default()
        };
        Ok(Self { data, path, key })
    }

    fn load_or_create_master_key() -> Result<[u8; 32]> {
        let entry = keyring::Entry::new("skyhook", "vault-master")
            .map_err(|e| SkyhookError::Vault(format!("keyring: {e}")))?;
        match entry.get_password() {
            Ok(b64) => {
                use base64_lite::*;
                let bytes = decode(&b64).map_err(|_| SkyhookError::Vault("bad key".into()))?;
                if bytes.len() != 32 {
                    return Err(SkyhookError::Vault("bad key length".into()));
                }
                let mut k = [0u8; 32];
                k.copy_from_slice(&bytes);
                Ok(k)
            }
            Err(_) => {
                let mut k = [0u8; 32];
                OsRng.fill_bytes(&mut k);
                let b64 = base64_lite::encode(&k);
                entry
                    .set_password(&b64)
                    .map_err(|e| SkyhookError::Vault(format!("keyring set: {e}")))?;
                Ok(k)
            }
        }
    }

    fn encrypt(plaintext: &[u8], key: &[u8; 32]) -> Result<Vec<u8>> {
        // Derive per-write subkey via argon2 over random salt, then AES-GCM
        let mut salt = [0u8; SALT_LEN];
        OsRng.fill_bytes(&mut salt);
        let params = Params::new(19 * 1024, 2, 1, Some(32))
            .map_err(|e| SkyhookError::Crypto(e.to_string()))?;
        let a2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let mut subkey = [0u8; 32];
        a2.hash_password_into(key, &salt, &mut subkey)
            .map_err(|e| SkyhookError::Crypto(e.to_string()))?;
        let cipher = Aes256Gcm::new_from_slice(&subkey)
            .map_err(|e| SkyhookError::Crypto(e.to_string()))?;
        let mut nonce_bytes = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ct = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| SkyhookError::Crypto(e.to_string()))?;
        let mut out = Vec::with_capacity(1 + SALT_LEN + NONCE_LEN + ct.len());
        out.push(VAULT_VERSION);
        out.extend_from_slice(&salt);
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ct);
        Ok(out)
    }

    fn decrypt(blob: &[u8], key: &[u8; 32]) -> Result<VaultData> {
        if blob.is_empty() || blob[0] != VAULT_VERSION {
            return Err(SkyhookError::Vault("bad version".into()));
        }
        if blob.len() < 1 + SALT_LEN + NONCE_LEN {
            return Err(SkyhookError::Vault("blob too short".into()));
        }
        let salt = &blob[1..1 + SALT_LEN];
        let nonce_bytes = &blob[1 + SALT_LEN..1 + SALT_LEN + NONCE_LEN];
        let ct = &blob[1 + SALT_LEN + NONCE_LEN..];
        let params = Params::new(19 * 1024, 2, 1, Some(32))
            .map_err(|e| SkyhookError::Crypto(e.to_string()))?;
        let a2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let mut subkey = [0u8; 32];
        a2.hash_password_into(key, salt, &mut subkey)
            .map_err(|e| SkyhookError::Crypto(e.to_string()))?;
        let cipher = Aes256Gcm::new_from_slice(&subkey)
            .map_err(|e| SkyhookError::Crypto(e.to_string()))?;
        let nonce = Nonce::from_slice(nonce_bytes);
        let pt = cipher
            .decrypt(nonce, ct)
            .map_err(|e| SkyhookError::Crypto(e.to_string()))?;
        let data: VaultData = serde_json::from_slice(&pt)?;
        Ok(data)
    }

    pub fn save(&self) -> Result<()> {
        let json = serde_json::to_vec(&self.data)?;
        let blob = Self::encrypt(&json, &self.key)?;
        let tmp = self.path.with_extension("bin.tmp");
        std::fs::write(&tmp, &blob)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<Connection> {
        // Return sanitized list (strip secrets for UI display)
        self.data
            .connections
            .iter()
            .map(|c| {
                let mut c = c.clone();
                c.auth = match c.auth {
                    AuthMethod::Password { .. } => AuthMethod::Password { password: String::new() },
                    AuthMethod::Key { passphrase, .. } => AuthMethod::Key {
                        private_key: String::new(),
                        passphrase: passphrase.map(|_| String::new()),
                    },
                    AuthMethod::Agent => AuthMethod::Agent,
                };
                c
            })
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<&Connection> {
        self.data.connections.iter().find(|c| c.id == id)
    }

    pub fn upsert(&mut self, mut conn: Connection) -> Result<Connection> {
        if let Some(existing) = self.data.connections.iter_mut().find(|c| c.id == conn.id) {
            // Preserve secret if UI sent blank
            match (&mut conn.auth, &existing.auth) {
                (AuthMethod::Password { password }, AuthMethod::Password { password: old })
                    if password.is_empty() =>
                {
                    *password = old.clone();
                }
                (
                    AuthMethod::Key { private_key, passphrase },
                    AuthMethod::Key { private_key: old_key, passphrase: old_pp },
                ) => {
                    if private_key.is_empty() {
                        *private_key = old_key.clone();
                    }
                    if matches!(passphrase, Some(s) if s.is_empty()) {
                        *passphrase = old_pp.clone();
                    }
                }
                _ => {}
            }
            *existing = conn.clone();
        } else {
            if conn.id.is_empty() {
                conn.id = uuid::Uuid::new_v4().to_string();
            }
            self.data.connections.push(conn.clone());
        }
        self.save()?;
        Ok(conn)
    }

    pub fn remove(&mut self, id: &str) -> Result<()> {
        self.data.connections.retain(|c| c.id != id);
        self.save()
    }
}

// Tiny inline base64 (avoids dep)
mod base64_lite {
    const T: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    pub fn encode(input: &[u8]) -> String {
        let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
        for chunk in input.chunks(3) {
            let b0 = chunk[0];
            let b1 = chunk.get(1).copied().unwrap_or(0);
            let b2 = chunk.get(2).copied().unwrap_or(0);
            out.push(T[(b0 >> 2) as usize] as char);
            out.push(T[(((b0 & 0b11) << 4) | (b1 >> 4)) as usize] as char);
            if chunk.len() > 1 {
                out.push(T[(((b1 & 0b1111) << 2) | (b2 >> 6)) as usize] as char);
            } else {
                out.push('=');
            }
            if chunk.len() > 2 {
                out.push(T[(b2 & 0b111111) as usize] as char);
            } else {
                out.push('=');
            }
        }
        out
    }
    pub fn decode(input: &str) -> Result<Vec<u8>, ()> {
        let s: Vec<u8> = input.bytes().filter(|&b| b != b'\n' && b != b'\r').collect();
        if s.len() % 4 != 0 {
            return Err(());
        }
        let mut out = Vec::with_capacity(s.len() / 4 * 3);
        for chunk in s.chunks(4) {
            let mut vals = [0u32; 4];
            let mut pad = 0;
            for (i, &c) in chunk.iter().enumerate() {
                if c == b'=' {
                    pad += 1;
                    vals[i] = 0;
                } else {
                    let pos = T.iter().position(|&x| x == c).ok_or(())?;
                    vals[i] = pos as u32;
                }
            }
            let n = (vals[0] << 18) | (vals[1] << 12) | (vals[2] << 6) | vals[3];
            out.push(((n >> 16) & 0xff) as u8);
            if pad < 2 {
                out.push(((n >> 8) & 0xff) as u8);
            }
            if pad < 1 {
                out.push((n & 0xff) as u8);
            }
        }
        Ok(out)
    }
}
