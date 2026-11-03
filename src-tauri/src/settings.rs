//! Simple app settings (key - value). Only known keys with known values are accepted.
//! Secrets never go here - they live in the credential store (see `secrets`).

use std::collections::BTreeMap;

use rusqlite::{Connection, params};

use crate::error::{AppError, AppResult};

/// What a setting may hold.
enum Rule {
    /// One of the listed words; the first one is the default.
    One(&'static [&'static str]),
    /// Free text checked by a function; `default` when nothing is stored.
    Text { max: usize, default: &'static str, check: fn(&str) -> bool },
}

/// Countries the Adzuna API covers (code used in its address). Poland first - the default.
pub const ADZUNA_COUNTRIES: &[&str] = &[
    "pl", "at", "au", "be", "br", "ca", "ch", "de", "es", "fr", "gb", "in", "it", "mx", "nl", "nz", "sg", "us", "za",
];

/// Allowed keys and their rules.
const ALLOWED: &[(&str, Rule)] = &[
    ("theme", Rule::One(&["system", "dark", "light"])),
    ("language", Rule::One(&["system", "pl", "en"])),
    ("motion", Rule::One(&["system", "full", "reduced"])),
    // Size of all text (PC: also Ctrl + / Ctrl - / Ctrl 0).
    ("text_size", Rule::One(&["normal", "small", "large", "xlarge"])),
    ("sound", Rule::One(&["off", "on"])),
    // "First steps" shown at the first start until finished or skipped.
    ("onboarding", Rule::One(&["pending", "done"])),
    // Collect offers at start and every few hours while the app is open.
    ("fetch_auto", Rule::One(&["on", "off"])),
    ("fetch_every", Rule::One(&["6", "3", "12"])),
    // Task Scheduler runs the app hourly without a window (only the installed app sets it up).
    ("background_task", Rule::One(&["on", "off"])),
    // Windows notification about new offers on the board.
    ("notify_new", Rule::One(&["on", "off"])),
    // Reminders: interviews and deadlines (a day and an hour before), follow-up, high scores.
    ("notify_reminders", Rule::One(&["on", "off"])),
    // Extra parts of the screen - a turned-off part disappears, its data stays.
    ("feature_dates", Rule::One(&["on", "off"])),
    ("feature_contacts", Rule::One(&["off", "on"])),
    ("feature_keys", Rule::One(&["on", "off"])),
    // Google Drive sync and, optionally, an own Google project (empty = built in).
    ("sync_drive", Rule::One(&["off", "on"])),
    ("google_client_id", Rule::Text { max: 200, default: "", check: is_client_id }),
    ("source_adzuna", Rule::One(&["off", "on"])),
    ("source_remotive", Rule::One(&["off", "on"])),
    ("source_weworkremotely", Rule::One(&["off", "on"])),
    ("source_remoteok", Rule::One(&["off", "on"])),
    ("source_jooble", Rule::One(&["off", "on"])),
    ("adzuna_country", Rule::One(ADZUNA_COUNTRIES)),
    ("source_mail", Rule::One(&["off", "on"])),
    ("mail_host", Rule::Text { max: 253, default: "imap.gmail.com", check: is_host }),
    ("mail_port", Rule::Text { max: 5, default: "993", check: is_port }),
    ("mail_user", Rule::Text { max: 254, default: "", check: is_mail_login }),
    // AI: provider, model (empty = the provider's default), daily cap of calls.
    ("ai_provider", Rule::One(&["none", "gemini", "groq", "openrouter", "ollama", "anthropic", "openai"])),
    ("ai_model", Rule::Text { max: 120, default: "", check: is_model }),
    ("ai_daily_limit", Rule::Text { max: 5, default: "200", check: is_limit }),
    ("ai_auto", Rule::One(&["on", "off"])),
    ("ollama_url", Rule::Text { max: 200, default: "http://localhost:11434", check: is_base_url }),
    // Board: hide offers scored below this (offers you acted on stay).
    ("hide_below", Rule::One(&["off", "30", "40", "50", "60", "70"])),
];

fn is_client_id(v: &str) -> bool {
    v.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
}

fn is_model(v: &str) -> bool {
    v.chars().all(|c| c.is_ascii_alphanumeric() || "-_.:/@".contains(c))
}

fn is_limit(v: &str) -> bool {
    v.parse::<u32>().is_ok_and(|n| (1..=10_000).contains(&n))
}

/// Address of a local AI server: http(s), a host and maybe a port - nothing else.
fn is_base_url(v: &str) -> bool {
    let Some(rest) = v.strip_prefix("http://").or_else(|| v.strip_prefix("https://")) else { return false };
    let rest = rest.trim_end_matches('/');
    let (host, port) = match rest.rsplit_once(':') {
        Some((h, p)) => (h, Some(p)),
        None => (rest, None),
    };
    is_host(host) && port.is_none_or(is_port)
}

fn is_host(v: &str) -> bool {
    !v.is_empty()
        && !v.starts_with(['.', '-'])
        && v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
}

fn is_port(v: &str) -> bool {
    v.parse::<u16>().is_ok_and(|p| p > 0)
}

/// A login is usually the e-mail address; empty = not set yet.
fn is_mail_login(v: &str) -> bool {
    v.chars().all(|c| !c.is_control() && !c.is_whitespace())
}

fn default_of(rule: &Rule) -> &'static str {
    match rule {
        Rule::One(values) => values[0],
        Rule::Text { default, .. } => default,
    }
}

fn accepts(rule: &Rule, value: &str) -> bool {
    match rule {
        Rule::One(values) => values.contains(&value),
        Rule::Text { max, check, .. } => value.chars().count() <= *max && check(value),
    }
}

pub fn all(conn: &Connection) -> AppResult<BTreeMap<String, String>> {
    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
    let stored = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    // Defaults first, stored values on top; unknown stored keys are ignored.
    Ok(ALLOWED
        .iter()
        .map(|(key, rule)| {
            let value = stored
                .get(*key)
                .filter(|v| accepts(rule, v))
                .cloned()
                .unwrap_or_else(|| default_of(rule).to_string());
            (key.to_string(), value)
        })
        .collect())
}

/// One setting (its default when not stored).
pub fn get(conn: &Connection, key: &str) -> AppResult<String> {
    all(conn)?
        .remove(key)
        .ok_or_else(|| AppError::Invalid(format!("unknown setting {key}")))
}

pub fn is_on(conn: &Connection, key: &str) -> AppResult<bool> {
    Ok(get(conn, key)? == "on")
}

pub fn set(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    let (_, rule) = ALLOWED
        .iter()
        .find(|(k, _)| *k == key)
        .ok_or_else(|| AppError::Invalid(format!("unknown setting {key}")))?;
    let value = value.trim();
    if !accepts(rule, value) {
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
        assert_eq!(get(&conn, "adzuna_country").unwrap(), "pl");
        assert!(!is_on(&conn, "source_adzuna").unwrap());
        assert!(is_on(&conn, "fetch_auto").unwrap());
    }

    #[test]
    fn text_settings_are_checked() {
        let conn = test_db();
        set(&conn, "mail_host", " imap.example.com ").unwrap();
        assert_eq!(get(&conn, "mail_host").unwrap(), "imap.example.com");
        assert!(set(&conn, "mail_host", "imap.example.com/../x").is_err());
        assert!(set(&conn, "mail_host", "").is_err());
        assert!(set(&conn, "mail_port", "70000").is_err());
        set(&conn, "mail_port", "143").unwrap();
        assert!(set(&conn, "mail_user", "a b@example.com").is_err());
        set(&conn, "mail_user", "oferty@example.com").unwrap();
        assert!(set(&conn, "adzuna_country", "xx").is_err());
    }
}
