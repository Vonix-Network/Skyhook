//! Per-provider API key storage backed by the OS keyring.
//!
//! Keys are stored under service `"skyhook"` with entry name
//! `"agent-key-<provider>"`. The `<provider>` slug is normalized to lowercase
//! to keep the entry name stable across callers.

use crate::error::{Result, SkyhookError};

/// Service name used for all keyring entries managed by this module.
const SERVICE: &str = "skyhook";

/// Build a normalized keyring entry name for the given provider slug.
fn entry_name(provider: &str) -> String {
    format!("agent-key-{}", provider.trim().to_ascii_lowercase())
}

/// Construct a keyring entry, mapping any backend error into [`SkyhookError::Vault`].
fn entry(provider: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, &entry_name(provider))
        .map_err(|e| SkyhookError::Vault(format!("keyring entry ({provider}): {e}")))
}

/// Store the API key for `provider`, overwriting any previous value.
pub fn set_key(provider: &str, key: &str) -> Result<()> {
    let entry = entry(provider)?;
    entry
        .set_password(key)
        .map_err(|e| SkyhookError::Vault(format!("keyring set ({provider}): {e}")))
}

/// Fetch the API key for `provider`.
///
/// Returns `Ok(None)` if no entry exists for that provider; any other backend
/// error is propagated as [`SkyhookError::Vault`].
pub fn get_key(provider: &str) -> Result<Option<String>> {
    let entry = entry(provider)?;
    match entry.get_password() {
        Ok(s) => Ok(Some(s)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(SkyhookError::Vault(format!("keyring get ({provider}): {e}"))),
    }
}

/// Remove the stored API key for `provider`. Returns `Ok(())` if the entry
/// was already absent.
pub fn remove_key(provider: &str) -> Result<()> {
    let entry = entry(provider)?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(SkyhookError::Vault(format!(
            "keyring delete ({provider}): {e}"
        ))),
    }
}

/// `true` iff a non-empty key is currently stored for `provider`.
pub fn has_key(provider: &str) -> bool {
    matches!(get_key(provider), Ok(Some(ref s)) if !s.is_empty())
}
