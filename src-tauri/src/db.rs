//! Opening the encrypted database, schema migrations and startup backups.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::{AppError, AppResult};
use crate::{diag, keycopy, secrets};

pub const DB_FILE: &str = "tablica.db";
pub const BACKUP_DIR: &str = "kopie";
const BACKUPS_KEPT: usize = 5;

/// Schema changes, in order. A database at `user_version = n` has the first `n` applied.
const MIGRATIONS: &[&str] = &[
    include_str!("migrations/001_init.sql"),
    include_str!("migrations/002_profiles.sql"),
    include_str!("migrations/003_sources.sql"),
    include_str!("migrations/004_mail.sql"),
    include_str!("migrations/005_ai.sql"),
    include_str!("migrations/006_agenda.sql"),
    include_str!("migrations/007_sync.sql"),
    include_str!("migrations/008_source_queries.sql"),
    include_str!("migrations/009_api_totals.sql"),
    include_str!("migrations/010_filter_rules.sql"),
    include_str!("migrations/011_translation.sql"),
    include_str!("migrations/012_content_removed.sql"),
    include_str!("migrations/013_himalayas_checked.sql"),
    include_str!("migrations/014_ai_providers.sql"),
    include_str!("migrations/015_freshness.sql"),
    include_str!("migrations/016_clean_start.sql"),
    include_str!("migrations/017_learned.sql"),
    include_str!("migrations/018_reread_alerts.sql"),
    include_str!("migrations/019_user_changed_at.sql"),
    include_str!("migrations/020_offer_links.sql"),
    include_str!("migrations/021_ai_precise.sql"),
    include_str!("migrations/022_ai_spend.sql"),
    include_str!("migrations/023_own.sql"),
    include_str!("migrations/024_renewed.sql"),
    include_str!("migrations/025_tree.sql"),
    include_str!("migrations/026_dup.sql"),
    include_str!("migrations/027_day_rate.sql"),
];

/// The version with the tree of duplicates (`025_tree.sql`).
const TREE_VERSION: usize = 25;

/// The version with day rates and "English required?" (`027_day_rate.sql`).
const DAY_VERSION: usize = 27;

/// Schema version this app writes (copies from newer versions are refused).
pub fn latest_version() -> i64 {
    MIGRATIONS.len() as i64
}

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
        // Version 25: the links merged into one offer become children - in the same transaction,
        // so the step is done completely or not at all.
        if index + 1 == TREE_VERSION {
            crate::tree::convert(&tx)?;
        }
        if index + 1 == DAY_VERSION {
            allow_day_rate(&tx)?;
        }
        tx.pragma_update(None, "user_version", (index + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

/// Version 27: `salary_period` may be "day" too. SQLite cannot change a CHECK in place, so the
/// table's stored definition gets the wider CHECK - the way SQLite's docs give for loosening a
/// constraint (no data changes, no rebuild of a table that eight others point to).
fn allow_day_rate(tx: &rusqlite::Transaction) -> AppResult<()> {
    const OLD: &str = "CHECK (salary_period IN ('month', 'hour'))";
    const NEW: &str = "CHECK (salary_period IN ('month', 'hour', 'day'))";
    let sql: String = tx.query_row("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'offers'", [], |r| r.get(0))?;
    if !sql.contains(OLD) {
        return Err(AppError::Invalid("offers table: the salary period check is not as expected".into()));
    }
    let version: i64 = tx.pragma_query_value(None, "schema_version", |r| r.get(0))?;
    tx.execute_batch("PRAGMA writable_schema = ON")?;
    tx.execute("UPDATE sqlite_master SET sql = ?1 WHERE type = 'table' AND name = 'offers'", [sql.replace(OLD, NEW)])?;
    // A new schema version makes every connection read the changed definition.
    tx.pragma_update(None, "schema_version", version + 1)?;
    tx.execute_batch("PRAGMA writable_schema = OFF")?;
    let check: String = tx.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if check != "ok" {
        return Err(AppError::Invalid(format!("database check after version 27: {check}")));
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
    open_with_vault(data_dir, backup, &SystemVault)
}

/// Where the database key lives - the system's credential store in the app, memory in tests.
pub trait KeyVault {
    fn get(&self) -> AppResult<Option<String>>;
    fn set(&self, key: &str) -> AppResult<()>;
}

/// Windows Credential Manager / Android Keystore (`secrets`).
pub struct SystemVault;

impl KeyVault for SystemVault {
    fn get(&self) -> AppResult<Option<String>> {
        secrets::get(secrets::DB_KEY)
    }
    fn set(&self, key: &str) -> AppResult<()> {
        secrets::set(secrets::DB_KEY, key)
    }
}

/// Where the key lives, named in the technical details of a locked database.
#[cfg(not(target_os = "android"))]
const VAULT: &str = "Windows Credential Manager";
#[cfg(target_os = "android")]
const VAULT: &str = "the Android Keystore";

fn open_with_vault(data_dir: &Path, backup: bool, vault: &dyn KeyVault) -> AppResult<Connection> {
    fs::create_dir_all(data_dir)?;
    let path = data_dir.join(DB_FILE);
    let exists = path.exists();
    let in_vault = vault.get()?;
    let yes = |b: bool| if b { "yes" } else { "no" };
    diag::log(
        data_dir,
        &format!(
            "start ({}) - database: {}, key in Credential Manager: {}, key copy: {}",
            if backup { "window" } else { "background or reopen" },
            yes(exists),
            yes(in_vault.is_some()),
            yes(keycopy::path(data_dir).exists()),
        ),
    );
    let (key, restored) = match in_vault {
        Some(key) => (key, false),
        // Key gone from Credential Manager: try the DPAPI copy next to the database.
        None if exists => match keycopy::read(data_dir) {
            Ok(Some(key)) => (key, true),
            Ok(None) => {
                diag::log(data_dir, "ERROR key missing from Credential Manager and there is no copy - writes blocked");
                return Err(AppError::Locked(
                    format!("the database exists, but its key is missing from {VAULT}"),
                ));
            }
            Err(e) => {
                diag::log(data_dir, &format!("ERROR key missing from Credential Manager and the copy cannot be read: {e}"));
                return Err(AppError::Locked(format!(
                    "the database key is missing from {VAULT} and its copy cannot be read: {e}"
                )));
            }
        },
        // No database and no key in Credential Manager: the key of the copy, when there is one -
        // a new random key would overwrite it, and the backups in `kopie/` (made with the old
        // key) could never be opened again.
        None => {
            let key = match keycopy::read(data_dir).ok().flatten() {
                Some(old) => {
                    diag::log(data_dir, "no database and no key in Credential Manager - the key copy is used for the new database");
                    old
                }
                None => secrets::random_hex(32)?,
            };
            vault.set(&key)?;
            (key, false)
        }
    };
    // A wrong key fails here, so nothing below ever saves or restores a key that does not open the data.
    let mut conn = open(&path, &key)?;
    if restored {
        vault.set(&key)?;
        diag::log(data_dir, "WARNING key was missing from Credential Manager - restored from the copy");
    }
    // Keep the copy in step with the key (not fatal - the next start tries again).
    if keycopy::read(data_dir).ok().flatten().as_deref() != Some(key.as_str()) {
        match keycopy::write(data_dir, &key) {
            Ok(()) => diag::log(data_dir, "key copy saved"),
            Err(e) => diag::log(data_dir, &format!("key copy failed: {e}")),
        }
    }
    let before: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    // A copy before a migration too, also in the background run (a migration may move offers).
    if exists && (backup || before < latest_version()) {
        // Not fatal: the app still works, the next start tries again.
        if let Err(e) = backup_if_changed(&path, &crate::clock::file_stamp()) {
            eprintln!("backup failed: {e}");
        }
    }
    migrate(&mut conn)?;
    // Version 5 added scores without AI, version 10 new filter reasons - check offers saved
    // before once again.
    if (1..10).contains(&before) {
        let tx = conn.transaction()?;
        crate::profiles::recheck_all(&tx)?;
        tx.commit()?;
    }
    // Version 20 (8.11.2026): new keys of companies and titles, the same job in several cities as
    // one offer, "Staff" / "Head" / "Expert" to check - every offer gets its key and its filter
    // results once more.
    if exists && (1..20).contains(&before) {
        crate::offers::refresh_fingerprints(&mut conn)?;
        let tx = conn.transaction()?;
        crate::profiles::recheck_all(&tx)?;
        tx.commit()?;
    }
    // Version 27: "English required?" for every offer saved before (the filter check fills it).
    if exists && (1..DAY_VERSION as i64).contains(&before) {
        let tx = conn.transaction()?;
        crate::profiles::recheck_all(&tx)?;
        tx.commit()?;
    }
    // "Several profiles" off (the default since 13.11.2026): offers checked before against the
    // profiles asleep now are checked again, once.
    if exists && crate::profiles::sleeping_results(&conn)? {
        let tx = conn.transaction()?;
        crate::profiles::recheck_all(&tx)?;
        tx.commit()?;
    }
    // The temporary rating test is gone from the app (13.11.2026) - so is its row (no migration:
    // a new database version would stop syncing with a phone not updated yet).
    conn.execute("DELETE FROM settings WHERE key = 'rating_test'", [])?;
    // Ratings given before learning came in count at once.
    if before < 20 {
        crate::learn::refresh(&mut conn)?;
    }
    Ok(conn)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub const TEST_KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    /// Once per test run: removes test folders older than an hour left by earlier runs.
    fn clean_old_test_dirs() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let Ok(entries) = fs::read_dir(std::env::temp_dir()) else { return };
            let hour = std::time::Duration::from_secs(3600);
            for entry in entries.flatten() {
                let old = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|age| age > hour);
                let ours = entry.file_name().to_str().is_some_and(|n| n.starts_with("tablica-test-"));
                if ours && old && entry.path().is_dir() {
                    let _ = fs::remove_dir_all(entry.path());
                }
            }
        });
    }

    pub fn temp_dir(name: &str) -> PathBuf {
        clean_old_test_dirs();
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
    fn upgrade_keeps_data_of_an_older_database() {
        let mut conn = open(&temp_dir("upgrade").join(DB_FILE), TEST_KEY).unwrap();
        for (index, sql) in MIGRATIONS.iter().enumerate().take(11) {
            conn.execute_batch(sql).unwrap();
            conn.pragma_update(None, "user_version", (index + 1) as i64).unwrap();
        }
        conn.execute(
            "INSERT INTO offers (uid, fingerprint, title, company, description, notes,
                stage_changed_at, added_at, updated_at)
             VALUES ('u-1', 'fp-1', 'Tester', 'Firma Przykładowa', 'Opis oferty', 'Moja notatka',
                '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO events (uid, offer_id, kind, at, created_at)
             VALUES ('e-1', ?1, 'interview', '2026-02-01T10:00:00Z', '2026-01-01T00:00:00Z')",
            [id],
        )
        .unwrap();

        migrate(&mut conn).unwrap();

        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version, latest_version());
        let detail = crate::offers::detail(&conn, id).unwrap();
        assert_eq!(detail.description.as_deref(), Some("Opis oferty"));
        assert_eq!(detail.notes, "Moja notatka");
        assert_eq!(detail.content_removed, None);
        let events: i64 = conn
            .query_row("SELECT count(*) FROM events WHERE offer_id = ?1", [id], |r| r.get(0))
            .unwrap();
        assert_eq!(events, 1);
    }

    /// Version 27 widens the CHECK in place: a day rate fits, anything else is still refused.
    #[test]
    fn day_rate_fits_after_version_27() {
        let mut conn = open(&temp_dir("dayrate").join(DB_FILE), TEST_KEY).unwrap();
        migrate(&mut conn).unwrap();
        let insert = |uid: &str, period: &str| {
            conn.execute(
                "INSERT INTO offers (uid, fingerprint, title, company, salary_min, salary_period, stage, stage_changed_at, added_at, updated_at)
                 VALUES (?1, ?1, 'Tester', 'Firma Przykładowa', 1200, ?2, 'new', ?3, ?3, ?3)",
                rusqlite::params![uid, period, "2026-11-15T00:00:00Z"],
            )
        };
        assert!(insert("u-day", "day").is_ok());
        assert!(insert("u-week", "week").is_err());
        let check: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap();
        assert_eq!(check, "ok");
    }

    /// Part 1 of 11.11.2026 on a database of version 20 (before it): offers with merged links
    /// become a tree, recruitments and offers added by hand become own - the whole family alike.
    #[test]
    fn upgrade_from_version_20_makes_trees_and_own_offers() {
        let mut conn = open(&temp_dir("upgrade20").join(DB_FILE), TEST_KEY).unwrap();
        for (index, sql) in MIGRATIONS.iter().enumerate().take(20) {
            conn.execute_batch(sql).unwrap();
            conn.pragma_update(None, "user_version", (index + 1) as i64).unwrap();
        }
        let at = "2026-11-01T00:00:00Z";
        let offer = |uid: &str, stage: &str, links: &[(&str, &str)]| -> i64 {
            conn.execute(
                "INSERT INTO offers (uid, fingerprint, title, company, stage, stage_changed_at, added_at, updated_at)
                 VALUES (?1, ?1, 'Tester', 'Firma Przykładowa', ?2, ?3, ?3, ?3)",
                rusqlite::params![uid, stage, at],
            )
            .unwrap();
            let id = conn.last_insert_rowid();
            for (source, url) in links {
                conn.execute(
                    "INSERT INTO offer_sources (offer_id, source, url, first_seen) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![id, source, url, at],
                )
                .unwrap();
            }
            id
        };
        let sent = offer("u-sent", "applied", &[("jjit", "https://justjoin.it/job-offer/a"), ("nfj", "https://nofluffjobs.com/job/a")]);
        let manual = offer("u-manual", "new", &[("manual", "https://www.praca.pl/b.html")]);
        let collected = offer("u-collected", "new", &[("jjit", "https://justjoin.it/job-offer/c"), ("nfj", "https://nofluffjobs.com/job/c")]);

        migrate(&mut conn).unwrap();

        let own_of = |id: i64| crate::offers::detail(&conn, id).unwrap().row.own;
        let sent_row = crate::offers::detail(&conn, sent).unwrap().row;
        assert!(sent_row.own);
        assert_eq!(sent_row.children.len(), 1);
        assert!(sent_row.children[0].own, "a child is own like its parent");
        assert_eq!(sent_row.children[0].stage, crate::model::Stage::New);
        assert!(own_of(manual));
        let collected_row = crate::offers::detail(&conn, collected).unwrap().row;
        assert!(!collected_row.own && !collected_row.children[0].own);
        let own: Vec<i64> = crate::offers::list(&conn, crate::model::View::Own, None).unwrap().iter().map(|r| r.id).collect();
        assert_eq!(own, vec![manual, sent], "to send first, then the recruitment");
        assert_eq!(crate::offers::counts(&conn, None).unwrap().to_send, 1);
    }

    /// The clean start of 6.11.2026: what came by itself goes to the trash, what the user
    /// worked on stays.
    #[test]
    fn clean_start_keeps_only_offers_the_user_worked_on() {
        let mut conn = open(&temp_dir("clean").join(DB_FILE), TEST_KEY).unwrap();
        for (index, sql) in MIGRATIONS.iter().enumerate().take(15) {
            conn.execute_batch(sql).unwrap();
            conn.pragma_update(None, "user_version", (index + 1) as i64).unwrap();
        }
        let offer = |title: &str, source: &str, extra: &str| {
            conn.execute(
                &format!(
                    "INSERT INTO offers (uid, fingerprint, title, company, stage_changed_at, added_at, updated_at)
                     VALUES ('u-{title}', 'fp-{title}', '{title}', 'Firma', '2026-11-01T00:00:00Z',
                        '2026-11-01T00:00:00Z', '2026-11-01T00:00:00Z')"
                ),
                [],
            )
            .unwrap();
            let id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO offer_sources (offer_id, source, first_seen) VALUES (?1, ?2, '2026-11-01T00:00:00Z')",
                rusqlite::params![id, source],
            )
            .unwrap();
            if !extra.is_empty() {
                conn.execute(&extra.replace("{id}", &id.to_string()), []).unwrap();
            }
        };
        offer("Zebrana", "adzuna", "");
        offer("Recznie", "manual", "");
        offer("Gwiazdka", "adzuna", "UPDATE offers SET favorite = 1 WHERE id = {id}");
        offer("Bardzo", "mail", "UPDATE offers SET interest = 'high' WHERE id = {id}");
        offer("Wyslana", "jooble", "UPDATE offers SET stage = 'applied' WHERE id = {id}");
        offer("Notatka", "remotive", "UPDATE offers SET notes = 'zadzwonić' WHERE id = {id}");
        offer("Termin", "adzuna", "INSERT INTO events (uid, offer_id, kind, at, created_at) VALUES ('e-1', {id}, 'deadline', '2026-11-20T10:00:00Z', '2026-11-01T00:00:00Z')");
        offer("Kontakt", "adzuna", "INSERT INTO contacts (offer_id, name, created_at) VALUES ({id}, 'Anna Przykładowa', '2026-11-01T00:00:00Z')");

        migrate(&mut conn).unwrap();

        let trashed: Vec<String> = conn
            .prepare("SELECT title FROM offers WHERE deleted_at IS NOT NULL ORDER BY title")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(trashed, vec!["Zebrana"]);
        let kept: i64 = conn.query_row("SELECT count(*) FROM offers WHERE deleted_at IS NULL", [], |r| r.get(0)).unwrap();
        assert_eq!(kept, 7);
    }

    /// Version 23: offers added by hand or past the stage "new" become the person's own; the rest
    /// (also a starred or "Very" one still at "new") stays collected.
    #[test]
    fn own_offers_are_marked_by_the_migration() {
        let mut conn = open(&temp_dir("own").join(DB_FILE), TEST_KEY).unwrap();
        for (index, sql) in MIGRATIONS.iter().enumerate().take(22) {
            conn.execute_batch(sql).unwrap();
            conn.pragma_update(None, "user_version", (index + 1) as i64).unwrap();
        }
        let offer = |title: &str, source: &str, stage: &str| {
            conn.execute(
                "INSERT INTO offers (uid, fingerprint, title, company, stage, stage_changed_at, added_at, updated_at, favorite)
                 VALUES ('u-' || ?1, 'fp-' || ?1, ?1, 'Firma', ?2, '2026-11-01T00:00:00Z', '2026-11-01T00:00:00Z',
                    '2026-11-01T00:00:00Z', ?1 = 'Gwiazdka')",
                rusqlite::params![title, stage],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO offer_sources (offer_id, source, first_seen) VALUES (?1, ?2, '2026-11-01T00:00:00Z')",
                rusqlite::params![conn.last_insert_rowid(), source],
            )
            .unwrap();
        };
        offer("Zebrana", "adzuna", "new");
        offer("Gwiazdka", "jooble", "new");
        offer("Recznie", "manual", "interested");
        offer("Zainteresowany", "mail", "interested");
        offer("Wyslana", "jooble", "applied");
        offer("Odrzucona", "adzuna", "rejected");

        migrate(&mut conn).unwrap();

        let own: Vec<String> = conn
            .prepare("SELECT title FROM offers WHERE own = 1 ORDER BY title")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(own, vec!["Odrzucona", "Recznie", "Wyslana", "Zainteresowany"]);
        let collected: i64 = conn.query_row("SELECT count(*) FROM offers WHERE own = 0", [], |r| r.get(0)).unwrap();
        assert_eq!(collected, 2);
    }

    /// Fails when an existing migration is edited - a schema change must be a new file.
    #[test]
    fn existing_migrations_are_frozen() {
        // FNV-1a over migrations 001-020 (all in released versions), line endings normalized (Git may check out CRLF).
        const EXISTING: usize = 20;
        const HASH: u64 = 11_219_189_190_065_471_435;
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for sql in &MIGRATIONS[..EXISTING] {
            for b in sql.bytes().filter(|&b| b != b'\r').chain([0u8]) {
                hash ^= b as u64;
                hash = hash.wrapping_mul(0x0100_0000_01b3);
            }
        }
        assert_eq!(hash, HASH, "an existing migration changed - add a new file instead");
    }

    /// The credential store in memory - tests never touch the real one.
    #[derive(Default)]
    struct MemVault(std::cell::RefCell<Option<String>>);

    impl KeyVault for MemVault {
        fn get(&self) -> AppResult<Option<String>> {
            Ok(self.0.borrow().clone())
        }
        fn set(&self, key: &str) -> AppResult<()> {
            *self.0.borrow_mut() = Some(key.to_string());
            Ok(())
        }
    }

    /// The case from 3.11.2026: the key disappeared from the credential store.
    #[cfg(windows)]
    #[test]
    fn lost_key_comes_back_from_the_copy() {
        let dir = temp_dir("vault");
        let vault = MemVault::default();
        {
            // First start: a new key in the vault, its DPAPI copy next to the database.
            let conn = open_with_vault(&dir, false, &vault).unwrap();
            conn.execute("INSERT INTO settings (key, value) VALUES ('probe', 'zostaje')", []).unwrap();
        }
        let key = vault.get().unwrap().expect("key saved");
        assert_eq!(keycopy::read(&dir).unwrap().as_deref(), Some(key.as_str()));

        // Gone from the vault: the copy opens the data and the key goes back.
        let empty = MemVault::default();
        let conn = open_with_vault(&dir, false, &empty).unwrap();
        let value: String = conn.query_row("SELECT value FROM settings WHERE key = 'probe'", [], |r| r.get(0)).unwrap();
        assert_eq!(value, "zostaje");
        assert_eq!(empty.get().unwrap(), Some(key));
    }

    #[cfg(windows)]
    #[test]
    fn no_key_and_no_copy_blocks_instead_of_starting_empty() {
        let dir = temp_dir("nokey");
        open_with_vault(&dir, false, &MemVault::default()).unwrap();
        fs::remove_file(keycopy::path(&dir)).unwrap();
        let result = open_with_vault(&dir, false, &MemVault::default());
        assert!(matches!(result, Err(AppError::Locked(_))));
        // The database is still there, untouched.
        assert!(dir.join(DB_FILE).exists());
    }

    #[cfg(windows)]
    #[test]
    fn a_wrong_key_in_the_vault_never_replaces_the_copy() {
        let dir = temp_dir("wrongkey");
        let vault = MemVault::default();
        open_with_vault(&dir, false, &vault).unwrap();
        let good = vault.get().unwrap().unwrap();
        let wrong = MemVault::default();
        wrong.set(&"ab".repeat(32)).unwrap();
        assert!(matches!(open_with_vault(&dir, false, &wrong), Err(AppError::Locked(_))));
        assert_eq!(keycopy::read(&dir).unwrap(), Some(good));
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
