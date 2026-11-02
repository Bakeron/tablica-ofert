//! Opening the encrypted database, schema migrations and startup backups.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::{AppError, AppResult};
use crate::secrets;

pub const DB_FILE: &str = "tablica.db";
const BACKUP_DIR: &str = "kopie";
const BACKUPS_KEPT: usize = 5;

/// Schema changes, in order. A database at `user_version = n` has the first `n` applied.
const MIGRATIONS: &[&str] = &[
    include_str!("migrations/001_init.sql"),
    include_str!("migrations/002_profiles.sql"),
    include_str!("migrations/003_sources.sql"),
    include_str!("migrations/004_mail.sql"),
    include_str!("migrations/005_ai.sql"),
    include_str!("migrations/006_agenda.sql"),
];

/// Opens a SQLCipher database with a raw 256-bit key (64 hex chars) and checks that
/// the key actually decrypts it.
pub fn open(path: &Path, key_hex: &str) -> AppResult<Connection> {
    if key_hex.len() != 64 || !key_hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AppError::Locked("database key has a wrong format".into()));
    }
    let conn = Connection::open(path)?;
    // Safe to format: the key is validated as hex above.
    conn.execute_batch(&format!("PRAGMA key = \"x'{key_hex}'\";"))?;
    // With a wrong key SQLCipher fails here with "file is not a database".
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))
        .map_err(|e| AppError::Locked(format!("cannot read the database: {e}")))?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    Ok(conn)
}

pub fn migrate(conn: &mut Connection) -> AppResult<()> {
    let current: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let latest = MIGRATIONS.len() as i64;
    if current > latest {
        return Err(AppError::Locked(format!(
            "the database comes from a newer app version (schema {current}, this app knows {latest})"
        )));
    }
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (index + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

/// Copies the database file to `kopie/` when it differs from the newest copy.
/// Keeps the 5 newest copies. Copies stay encrypted with the same key.
pub fn backup_if_changed(db_path: &Path, stamp: &str) -> AppResult<Option<PathBuf>> {
    if !db_path.exists() {
        return Ok(None);
    }
    let dir = db_path.with_file_name(BACKUP_DIR);
    fs::create_dir_all(&dir)?;
    let current = fs::read(db_path)?;
    let mut copies = list_backups(&dir)?;
    if let Some(newest) = copies.last() {
        if fs::read(newest)? == current {
            return Ok(None);
        }
    }
    let target = dir.join(format!("tablica-{stamp}.db"));
    if target.exists() {
        return Ok(None);
    }
    let temp = target.with_extension("tmp");
    fs::write(&temp, &current)?;
    fs::rename(&temp, &target)?;
    copies.push(target.clone());
    while copies.len() > BACKUPS_KEPT {
        fs::remove_file(copies.remove(0))?;
    }
    Ok(Some(target))
}

fn list_backups(dir: &Path) -> AppResult<Vec<PathBuf>> {
    let mut copies: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("tablica-") && n.ends_with(".db"))
        })
        .collect();
    // The timestamp in the name sorts chronologically.
    copies.sort();
    Ok(copies)
}

/// Full startup: key from the credential store (created on first run), open, back up, migrate.
/// Any failure leaves the app without a connection - writes stay blocked instead of
/// silently starting with an empty list.
/// `backup` = false for the hourly background run - otherwise five hourly copies would push
/// out the older, possibly still good ones.
pub fn open_app_database(data_dir: &Path, backup: bool) -> AppResult<Connection> {
    fs::create_dir_all(data_dir)?;
    let path = data_dir.join(DB_FILE);
    let exists = path.exists();
    let key = match secrets::get(secrets::DB_KEY)? {
        Some(key) => key,
        None if exists => {
            return Err(AppError::Locked(
                "the database exists, but its key is missing from Windows Credential Manager".into(),
            ));
        }
        None => {
            let key = secrets::random_hex(32)?;
            secrets::set(secrets::DB_KEY, &key)?;
            key
        }
    };
    let mut conn = open(&path, &key)?;
    if exists && backup {
        // Not fatal: the app still works, the next start tries again.
        if let Err(e) = backup_if_changed(&path, &crate::clock::file_stamp()) {
            eprintln!("backup failed: {e}");
        }
    }
    let before: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    migrate(&mut conn)?;
    // Version 5 added scores without AI - count them once for offers saved before.
    if (1..5).contains(&before) {
        let tx = conn.transaction()?;
        crate::profiles::recheck_all(&tx)?;
        tx.commit()?;
    }
    Ok(conn)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub const TEST_KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    pub fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tablica-test-{name}-{}",
            secrets::random_hex(6).unwrap()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    pub fn test_db() -> Connection {
        let mut conn = open(&temp_dir("db").join(DB_FILE), TEST_KEY).unwrap();
        migrate(&mut conn).unwrap();
        conn
    }

    #[test]
    fn file_is_encrypted_and_wrong_key_is_rejected() {
        let path = temp_dir("enc").join(DB_FILE);
        {
            let mut conn = open(&path, TEST_KEY).unwrap();
            migrate(&mut conn).unwrap();
            conn.execute("INSERT INTO settings (key, value) VALUES ('probe', 'secret-value')", [])
                .unwrap();
        }
        let raw = fs::read(&path).unwrap();
        assert!(!raw.windows(12).any(|w| w == b"secret-value"));
        assert!(!raw.starts_with(b"SQLite format 3"));

        let other = "ff".repeat(32);
        assert!(matches!(open(&path, &other), Err(AppError::Locked(_))));
        assert!(open(&path, TEST_KEY).is_ok());
    }

    #[test]
    fn malformed_key_is_refused() {
        let path = temp_dir("badkey").join(DB_FILE);
        assert!(open(&path, "abc").is_err());
        assert!(open(&path, &format!("{}'; --", &TEST_KEY[..60])).is_err());
    }

    #[test]
    fn migrations_are_idempotent_and_newer_schema_locks() {
        let mut conn = test_db();
        migrate(&mut conn).unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
        assert!(matches!(migrate(&mut conn), Err(AppError::Locked(_))));
    }

    #[test]
    fn backup_only_when_changed_and_keeps_five() {
        let dir = temp_dir("backup");
        let db = dir.join(DB_FILE);
        fs::write(&db, b"v1").unwrap();
        assert!(backup_if_changed(&db, "20260101-000001").unwrap().is_some());
        assert!(backup_if_changed(&db, "20260101-000002").unwrap().is_none());
        for i in 2..10 {
            fs::write(&db, format!("v{i}")).unwrap();
            backup_if_changed(&db, &format!("20260101-0000{i:02}")).unwrap();
        }
        let copies = list_backups(&dir.join(BACKUP_DIR)).unwrap();
        assert_eq!(copies.len(), BACKUPS_KEPT);
        assert_eq!(fs::read(copies.last().unwrap()).unwrap(), b"v9");
    }
}
