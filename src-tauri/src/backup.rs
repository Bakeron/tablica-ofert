//! "Save a copy as…", "Load a copy…" and "Export to Excel (CSV)".
//!
//! A saved copy is a SQLCipher database locked with a password the person chooses - unlike the
//! database itself (whose key lives in this computer's credential store), it can be opened on
//! another computer. Secrets (API keys, passwords) are never part of it: they are not in the
//! database at all.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use crate::db;
use crate::error::{AppError, AppResult};

pub const MIN_PASSWORD: usize = 8;

fn check_password(password: &str) -> AppResult<()> {
    if password.chars().count() < MIN_PASSWORD || password.chars().count() > 200 {
        return Err(AppError::Invalid(format!("the password must have at least {MIN_PASSWORD} characters")));
    }
    Ok(())
}

fn schema_version(conn: &Connection, schema: &str) -> AppResult<i64> {
    Ok(conn.query_row(&format!("PRAGMA {schema}.user_version"), [], |r| r.get(0))?)
}

/// Checked before the "Save as" window opens - no point choosing a place for a refused copy.
pub fn save_copy_check(password: &str) -> AppResult<()> {
    check_password(password)
}

/// Writes the whole database into `target`, encrypted with `password`.
pub fn save_copy(conn: &Connection, target: &Path, password: &str) -> AppResult<()> {
    check_password(password)?;
    let temp = target.with_extension("tmp");
    let _ = fs::remove_file(&temp);
    let path = temp.to_str().ok_or_else(|| AppError::Invalid("unusual file path".into()))?;
    // Bound parameters: the path and the password never become part of the SQL text.
    conn.execute("ATTACH DATABASE ?1 AS copy KEY ?2", params![path, password])?;
    let result = (|| -> AppResult<()> {
        conn.query_row("SELECT sqlcipher_export('copy')", [], |_| Ok(()))?;
        let version = schema_version(conn, "main")?;
        conn.execute_batch(&format!("PRAGMA copy.user_version = {version};"))?;
        Ok(())
    })();
    conn.execute("DETACH DATABASE copy", [])?;
    result?;
    fs::rename(&temp, target)?;
    Ok(())
}

/// Opens a saved copy with its password and checks it is a Tablica Ofert database this
/// version can read. Returns the number of offers in it.
pub fn check_copy(source: &Path, password: &str) -> AppResult<i64> {
    let conn = Connection::open(source)?;
    conn.pragma_update(None, "key", password)?;
    let count = conn
        .query_row("SELECT count(*) FROM offers", [], |r| r.get(0))
        .map_err(|_| AppError::Invalid("wrong password or not a copy of Tablica Ofert".into()))?;
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version > db::latest_version() {
        return Err(AppError::Invalid("the copy comes from a newer app version - update the app first".into()));
    }
    Ok(count)
}

/// Replaces the database at `db_path` with the copy: the copy is re-encrypted with this
/// computer's key into a temporary file, which then takes the place of the database
/// (the old database stays in `kopie/` as a backup first).
pub fn load_copy(db_path: &Path, key_hex: &str, source: &Path, password: &str) -> AppResult<PathBuf> {
    check_copy(source, password)?;
    if key_hex.len() != 64 || !key_hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AppError::Locked("database key has a wrong format".into()));
    }
    // A backup of the current state first - loading a copy can always be undone.
    let backup = db::backup_if_changed(db_path, &crate::clock::file_stamp())?;
    let temp = db_path.with_extension("import");
    let _ = fs::remove_file(&temp);
    {
        let conn = Connection::open(source)?;
        conn.pragma_update(None, "key", password)?;
        let path = temp.to_str().ok_or_else(|| AppError::Invalid("unusual file path".into()))?;
        // Safe to format: the key is validated as hex above.
        conn.execute(&format!("ATTACH DATABASE ?1 AS fresh KEY \"x'{key_hex}'\""), [path])?;
        conn.query_row("SELECT sqlcipher_export('fresh')", [], |_| Ok(()))?;
        let version = schema_version(&conn, "main")?;
        conn.execute_batch(&format!("PRAGMA fresh.user_version = {version};"))?;
        conn.execute("DETACH DATABASE fresh", [])?;
    }
    // The new file must open with this computer's key before it replaces anything.
    drop(db::open(&temp, key_hex)?);
    fs::rename(&temp, db_path)?;
    Ok(backup.unwrap_or_else(|| db_path.to_path_buf()))
}

// ---------- CSV ----------

/// A cell that Excel would run as a formula (`=`, `+`, `-`, `@`, tab, CR) gets a leading
/// apostrophe; every cell is quoted.
pub fn csv_cell(value: &str) -> String {
    let risky = value.starts_with(['=', '+', '-', '@', '\t', '\r']);
    let safe = if risky { format!("'{value}") } else { value.to_string() };
    format!("\"{}\"", safe.replace('"', "\"\""))
}

/// All offers outside the trash, one per row. Semicolons and a BOM - Polish Excel opens it
/// with letters and columns right. `pl` = Polish column names.
pub fn csv(conn: &Connection, pl: bool) -> AppResult<String> {
    let head: &[&str] = if pl {
        &[
            "Stanowisko", "Firma", "Miasto", "Praca zdalna", "Widełki od", "Widełki do", "Waluta", "Okres", "Umowa",
            "Poziom", "Etap", "Zainteresowanie", "Ulubiona", "Dopasowanie", "Źródło oceny", "Źródła", "Link", "Dodano",
            "Notatki",
        ]
    } else {
        &[
            "Title", "Company", "City", "Remote", "Salary from", "Salary to", "Currency", "Period", "Contract", "Level",
            "Stage", "Interest", "Favourite", "Match", "Score source", "Sources", "Link", "Added", "Notes",
        ]
    };
    let mut out = String::from("\u{feff}");
    out.push_str(&head.iter().map(|h| csv_cell(h)).collect::<Vec<_>>().join(";"));
    out.push_str("\r\n");
    let mut stmt = conn.prepare(
        "SELECT o.title, o.company, o.city, o.remote, o.salary_min, o.salary_max, o.salary_currency, o.salary_period,
                o.contract, o.level, o.stage, o.interest, o.favorite, o.score, o.score_source,
                (SELECT group_concat(s.source, ', ') FROM offer_sources s WHERE s.offer_id = o.id),
                o.url, o.added_at, o.notes
         FROM offers o WHERE o.deleted_at IS NULL ORDER BY o.added_at DESC",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        let mut cells = Vec::with_capacity(19);
        for i in 0..19 {
            let value: rusqlite::types::Value = r.get(i)?;
            let text = match value {
                rusqlite::types::Value::Null => String::new(),
                rusqlite::types::Value::Integer(n) if i == 12 => if n == 1 { "tak".into() } else { String::new() },
                rusqlite::types::Value::Integer(n) => n.to_string(),
                rusqlite::types::Value::Real(f) => f.to_string(),
                rusqlite::types::Value::Text(t) => t,
                rusqlite::types::Value::Blob(_) => String::new(),
            };
            cells.push(csv_cell(&text));
        }
        out.push_str(&cells.join(";"));
        out.push_str("\r\n");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::{TEST_KEY, temp_dir, test_db};
    use crate::model::OfferInput;

    fn add(conn: &mut Connection, title: &str) {
        crate::offers::add(conn, OfferInput { title: title.into(), company: "Firma \"Cudzysłów\"".into(), source: "manual".into(), ..Default::default() }).unwrap();
    }

    #[test]
    fn copy_round_trip_with_a_password() {
        let dir = temp_dir("copy");
        let mut conn = test_db();
        add(&mut conn, "Tester");
        let copy = dir.join("kopia.tablica");
        assert!(save_copy(&conn, &copy, "short").is_err());
        save_copy(&conn, &copy, "hasło-do-kopii").unwrap();
        // Encrypted: no plain text inside, wrong password refused.
        assert!(!fs::read(&copy).unwrap().windows(6).any(|w| w == b"Tester"));
        assert!(check_copy(&copy, "inne-hasło-123").is_err());
        assert_eq!(check_copy(&copy, "hasło-do-kopii").unwrap(), 1);

        // Loading replaces another database and keeps a backup of it.
        let target_dir = temp_dir("target");
        let target = target_dir.join(db::DB_FILE);
        {
            let mut other = db::open(&target, TEST_KEY).unwrap();
            db::migrate(&mut other).unwrap();
            add(&mut other, "Inna oferta");
            add(&mut other, "Jeszcze inna");
        }
        load_copy(&target, TEST_KEY, &copy, "hasło-do-kopii").unwrap();
        let loaded = db::open(&target, TEST_KEY).unwrap();
        let titles: Vec<String> = loaded.prepare("SELECT title FROM offers").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        assert_eq!(titles, vec!["Tester"]);
        let version: i64 = loaded.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version, db::latest_version());
        assert!(target_dir.join("kopie").read_dir().unwrap().count() >= 1);
    }

    #[test]
    fn csv_is_safe_for_excel() {
        assert_eq!(csv_cell("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");
        assert_eq!(csv_cell("-5"), "\"'-5\"");
        assert_eq!(csv_cell("zwykły tekst"), "\"zwykły tekst\"");
        let mut conn = test_db();
        add(&mut conn, "=cmd|' /C calc'!A0");
        let text = csv(&conn, true).unwrap();
        assert!(text.starts_with('\u{feff}'));
        assert!(text.contains("\"'=cmd|' /C calc'!A0\""));
        assert!(text.contains("\"Firma \"\"Cudzysłów\"\"\""));
        assert_eq!(text.lines().count(), 2);
    }
}
