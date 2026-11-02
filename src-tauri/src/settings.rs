//! Simple app settings (key - value). Only known keys with known values are accepted.
//! Secrets never go here - they live in the credential store (see `secrets`).

use std::collections::BTreeMap;

use rusqlite::{Connection, params};

use crate::error::{AppError, AppResult};

/// Allowed keys and their allowed values.
const ALLOWED: &[(&str, &[&str])] = &[
    ("theme", &["system", "dark", "light"]),
    ("language", &["system", "pl", "en"]),
    ("motion", &["system", "full", "reduced"]),
    ("sound", &["off", "on"]),
    // "First steps" shown at the first start until finished or skipped.
    ("onboarding", &["pending", "done"]),
];

pub fn all(conn: &Connection) -> AppResult<BTreeMap<String, String>> {
    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
    let stored = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    // Defaults first, stored values on top; unknown stored keys are ignored.
    Ok(ALLOWED
        .iter()
        .map(|(key, values)| {
            let value = stored
                .get(*key)
                .filter(|v| values.contains(&v.as_str()))
                .cloned()
                .unwrap_or_else(|| values[0].to_string());
            (key.to_string(), value)
        })
        .collect())
}

pub fn set(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    let allowed = ALLOWED
        .iter()
        .find(|(k, _)| *k == key)
        .ok_or_else(|| AppError::Invalid(format!("unknown setting {key}")))?;
    if !allowed.1.contains(&value) {
        return Err(AppError::Invalid(format!("wrong value for {key}")));
    }
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_db;

    #[test]
    fn defaults_and_changes() {
        let conn = test_db();
        assert_eq!(all(&conn).unwrap()["theme"], "system");
        set(&conn, "theme", "dark").unwrap();
        set(&conn, "theme", "light").unwrap();
        assert_eq!(all(&conn).unwrap()["theme"], "light");
        assert!(set(&conn, "theme", "pink").is_err());
        assert!(set(&conn, "apiKey", "secret").is_err());
    }
}
