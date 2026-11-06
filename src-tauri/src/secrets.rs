//! Secrets (database key, API keys, mailbox password) live only in the OS credential
//! store - Windows Credential Manager on PC. They never go to the database, backups,
//! CSV export, logs or AI. The UI only ever sees the last 4 characters.

use crate::error::{AppError, AppResult};

const SERVICE: &str = "pl.prywatne.tablicaofert";
/// Development builds use their own key and their own data folder.
pub const DB_KEY: &str = if cfg!(debug_assertions) { "database-key-dev" } else { "database-key" };

/// Secrets the user may type in the settings. Anything else is refused.
pub const USER_SECRETS: &[&str] = &[
    "adzuna-app-id",
    "adzuna-app-key",
    "jooble-key",
    "mail-password",
    "ai-key-gemini",
    "ai-key-groq",
    "ai-key-openrouter",
    "ai-key-anthropic",
    "ai-key-openai",
    "google-client-secret",
];

pub const MAX_SECRET: usize = 512;

pub fn init() -> AppResult<()> {
    #[cfg(windows)]
    {
        let store = windows_native_keyring_store::Store::new()
            .map_err(|e| AppError::Secret(e.to_string()))?;
        keyring_core::set_default_store(store);
    }
    // Android Keystore (Tauri prepares the Android context the store needs).
    #[cfg(target_os = "android")]
    {
        let store = android_native_keyring_store::Store::new().map_err(|e| AppError::Secret(e.to_string()))?;
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

pub fn delete(name: &str) -> AppResult<()> {
    match entry(name)?.delete_credential() {
        Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::Secret(e.to_string())),
    }
}

fn user_secret(name: &str) -> AppResult<&'static str> {
    USER_SECRETS
        .iter()
        .copied()
        .find(|n| *n == name)
        .ok_or_else(|| AppError::Invalid(format!("unknown secret {name}")))
}

/// Checks a value typed by the user: trimmed, not empty, no spaces or control characters.
pub fn clean_value(value: &str) -> AppResult<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > MAX_SECRET || value.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(AppError::Invalid("the key has a wrong format".into()));
    }
    Ok(value.to_string())
}

/// Saves a secret typed in the settings.
pub fn set_user(name: &str, value: &str) -> AppResult<()> {
    set(user_secret(name)?, &clean_value(&without_groups(name, value))?)
}

/// Google shows an app password in four groups ("abcd efgh ijkl mnop") and the spaces come
/// along when it is copied - they are not part of the password.
fn without_groups(name: &str, value: &str) -> String {
    if name == "mail-password" {
        value.chars().filter(|c| *c != ' ').collect()
    } else {
        value.to_string()
    }
}

/// A saved key or password the user typed in the settings - only for "Show key" clicked
/// by the person (never the database or sync keys: `user_secret` lets through only user ones).
pub fn reveal_user(name: &str) -> AppResult<Option<String>> {
    get(user_secret(name)?)
}

pub fn delete_user(name: &str) -> AppResult<()> {
    delete(user_secret(name)?)
}

/// Last 4 characters (what the settings show), `None` = not set.
pub fn hint(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= 8 {
        return "••••".into();
    }
    format!("••••{}", chars[chars.len() - 4..].iter().collect::<String>())
}

/// Every user secret that is set, as name -> hint.
pub fn user_hints() -> AppResult<std::collections::BTreeMap<String, String>> {
    let mut out = std::collections::BTreeMap::new();
    for name in USER_SECRETS {
        if let Some(value) = get(name)? {
            out.insert(name.to_string(), hint(&value));
        }
    }
    Ok(out)
}

/// Random bytes from the OS generator, as lowercase hex.
pub fn random_hex(bytes: usize) -> AppResult<String> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).map_err(|e| AppError::Secret(e.to_string()))?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hint_shows_only_the_end() {
        assert_eq!(hint("abcdefghijkl1234"), "••••1234");
        assert_eq!(hint("short"), "••••");
    }

    #[test]
    fn app_password_groups_are_joined() {
        assert_eq!(clean_value(&without_groups("mail-password", "abcd efgh ijkl mnop")).unwrap(), "abcdefghijklmnop");
        assert!(clean_value(&without_groups("adzuna-app-key","abcd efgh")).is_err());
    }

    #[test]
    fn values_and_names_are_checked() {
        assert_eq!(clean_value("  key-123 ").unwrap(), "key-123");
        assert!(clean_value("").is_err());
        assert!(clean_value("two words").is_err());
        assert!(clean_value(&"x".repeat(MAX_SECRET + 1)).is_err());
        assert!(user_secret("database-key").is_err());
        assert!(user_secret("adzuna-app-key").is_ok());
    }
}
