//! Daily counter of calls to services with limits (Adzuna, Remotive, Jooble, AI providers)
//! and a counter for the whole life of a key (Jooble). Shown in the settings and used to stop
//! before a free limit is used up.

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::AppResult;

fn today() -> String {
    crate::clock::now().get(..10).unwrap_or_default().to_string()
}

pub fn calls_today(conn: &Connection, key: &str) -> AppResult<i64> {
    Ok(conn
        .query_row("SELECT calls FROM api_usage WHERE key = ?1 AND day = ?2", params![key, today()], |r| r.get(0))
        .optional()?
        .unwrap_or(0))
}

pub fn add_calls(conn: &Connection, key: &str, calls: i64) -> AppResult<()> {
    if calls <= 0 {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO api_usage (key, day, calls) VALUES (?1, ?2, ?3)
         ON CONFLICT (key, day) DO UPDATE SET calls = calls + excluded.calls",
        params![key, today(), calls],
    )?;
    conn.execute(
        "INSERT INTO api_totals (key, calls) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET calls = calls + excluded.calls",
        params![key, calls],
    )?;
    // Old days are not needed - keep about a month.
    conn.execute("DELETE FROM api_usage WHERE day < date('now', '-35 days')", [])?;
    Ok(())
}

/// Today's count starts again from zero (e.g. the mark "the provider said: limit").
pub fn clear_today(conn: &Connection, key: &str) -> AppResult<()> {
    conn.execute("DELETE FROM api_usage WHERE key = ?1 AND day = ?2", params![key, today()])?;
    Ok(())
}

/// Calls in the last 30 days (for limits counted per month).
pub fn calls_month(conn: &Connection, key: &str) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT COALESCE(SUM(calls), 0) FROM api_usage WHERE key = ?1 AND day >= date('now', '-29 days')",
        [key],
        |r| r.get(0),
    )?)
}

/// Calls since the key was saved (all calls, when it was never changed in this app).
pub fn calls_total(conn: &Connection, key: &str) -> AppResult<i64> {
    Ok(conn
        .query_row("SELECT calls FROM api_totals WHERE key = ?1", [key], |r| r.get(0))
        .optional()?
        .unwrap_or(0))
}

/// A new key starts its life from zero.
pub fn reset_total(conn: &Connection, key: &str) -> AppResult<()> {
    conn.execute("DELETE FROM api_totals WHERE key = ?1", [key])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_db;

    #[test]
    fn counts_calls_per_day() {
        let conn = test_db();
        assert_eq!(calls_today(&conn, "adzuna").unwrap(), 0);
        add_calls(&conn, "adzuna", 2).unwrap();
        add_calls(&conn, "adzuna", 3).unwrap();
        add_calls(&conn, "remotive", 1).unwrap();
        assert_eq!(calls_today(&conn, "adzuna").unwrap(), 5);
        assert_eq!(calls_month(&conn, "adzuna").unwrap(), 5);
        assert_eq!(calls_today(&conn, "remotive").unwrap(), 1);
        assert_eq!(calls_total(&conn, "adzuna").unwrap(), 5);
    }

    #[test]
    fn total_survives_old_days_and_resets_for_a_new_key() {
        let conn = test_db();
        conn.execute("INSERT INTO api_usage (key, day, calls) VALUES ('jooble', '2020-01-01', 7)", []).unwrap();
        add_calls(&conn, "jooble", 1).unwrap();
        // The old day is gone from the daily counter, the total keeps counting.
        assert_eq!(calls_month(&conn, "jooble").unwrap(), 1);
        add_calls(&conn, "jooble", 1).unwrap();
        assert_eq!(calls_total(&conn, "jooble").unwrap(), 2);
        reset_total(&conn, "jooble").unwrap();
        assert_eq!(calls_total(&conn, "jooble").unwrap(), 0);
    }
}
