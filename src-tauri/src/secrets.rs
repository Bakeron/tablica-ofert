//! Secrets (database key, later API keys and mailbox password) live only in the
//! OS credential store - Windows Credential Manager on PC.

use crate::error::{AppError, AppResult};

const SERVICE: &str = "pl.prywatne.tablicaofert";
/// Development builds use their own key and their own data folder.
pub const DB_KEY: &str = if cfg!(debug_assertions) { "database-key-dev" } else { "database-key" };

pub fn init() -> AppResult<()> {
    #[cfg(windows)]
    {
        let store = windows_native_keyring_store::Store::new()
            .map_err(|e| AppError::Secret(e.to_string()))?;
        keyring_core::set_default_store(store);
    }
    Ok(())
}

fn entry(name: &str) -> AppResult<keyring_core::Entry> {
    keyring_core::Entry::new(SERVICE, name).map_err(|e| AppError::Secret(e.to_string()))
}

pub fn get(name: &str) -> AppResult<Option<String>> {
    match entry(name)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring_core::Error::NoEntry) => Ok(None),
        Err(e) => Err(AppError::Secret(e.to_string())),
    }
}

pub fn set(name: &str, value: &str) -> AppResult<()> {
    entry(name)?
        .set_password(value)
        .map_err(|e| AppError::Secret(e.to_string()))
}

/// Random bytes from the OS generator, as lowercase hex.
pub fn random_hex(bytes: usize) -> AppResult<String> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).map_err(|e| AppError::Secret(e.to_string()))?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}
