//! TEMPORARY - the author's blind rating of 10 offers for the test of AI ratings (zmiany.md,
//! point 4, 12.11.2026). The app picks 10 offers spread from the best to the worst score once,
//! the author rates each "yes / maybe / no" without seeing the AI's score, and the answers are
//! compared with the models (`ai::eval`). Removed from the app after the test, together with its
//! row in `settings`.

use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::model::{Remote, SalaryPeriod};

/// The test's own row in `settings` - not one of the app's settings (`settings::all` skips it).
const KEY: &str = "rating_test";
const SIZE: usize = 10;
/// A shorter description is no base for a rating.
const MIN_DESCRIPTION: i64 = 300;
const WHY_MAX: usize = 300;

#[derive(Debug, Default, Serialize, Deserialize)]
struct Stored {
    ids: Vec<i64>,
    answers: BTreeMap<i64, Answer>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Answer {
    /// "yes", "maybe" or "no".
    pub verdict: String,
    pub why: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestOffer {
    pub id: i64,
    pub title: String,
    pub company: String,
    pub city: Option<String>,
    pub remote: Remote,
    pub salary_min: Option<i64>,
    pub salary_max: Option<i64>,
    pub salary_currency: Option<String>,
    pub salary_period: Option<SalaryPeriod>,
    pub description: String,
    pub url: Option<String>,
    pub answer: Option<Answer>,
}

fn load(conn: &Connection) -> AppResult<Stored> {
    let raw: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [KEY], |r| r.get(0))
        .optional()?;
    Ok(raw.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default())
}

fn save(conn: &Connection, stored: &Stored) -> AppResult<()> {
    let value = serde_json::to_string(stored).map_err(|e| AppError::Invalid(e.to_string()))?;
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![KEY, value],
    )?;
    Ok(())
}

/// 10 offers with a real description, spread evenly from the highest score to the lowest (offers
/// without a score fill up when there are not enough rated ones).
fn pick(conn: &Connection) -> AppResult<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM offers
         WHERE deleted_at IS NULL AND archived_at IS NULL AND parent_id IS NULL
           AND length(coalesce(description, '')) >= ?1
         ORDER BY score IS NULL, score DESC, id",
    )?;
    let all: Vec<i64> = stmt.query_map([MIN_DESCRIPTION], |r| r.get(0))?.collect::<Result<_, _>>()?;
    if all.len() <= SIZE {
        return Ok(all);
    }
    Ok((0..SIZE).map(|i| all[i * (all.len() - 1) / (SIZE - 1)]).collect())
}

/// The test's offers with the answers given so far; the set is chosen on the first call.
pub fn offers(conn: &Connection) -> AppResult<Vec<TestOffer>> {
    let mut stored = load(conn)?;
    if stored.ids.is_empty() {
        stored.ids = pick(conn)?;
        save(conn, &stored)?;
    }
    let mut out = Vec::new();
    for id in &stored.ids {
        let found = conn
            .query_row(
                "SELECT title, company, city, remote, salary_min, salary_max, salary_currency, salary_period,
                        coalesce(description, ''), url
                 FROM offers WHERE id = ?1",
                [id],
                |r| {
                    Ok(TestOffer {
                        id: *id,
                        title: r.get(0)?,
                        company: r.get(1)?,
                        city: r.get(2)?,
                        remote: r.get(3)?,
                        salary_min: r.get(4)?,
                        salary_max: r.get(5)?,
                        salary_currency: r.get(6)?,
                        salary_period: r.get(7)?,
                        description: r.get(8)?,
                        url: r.get(9)?,
                        answer: stored.answers.get(id).cloned(),
                    })
                },
            )
            .optional()?;
        // An offer purged since the set was chosen just drops out.
        out.extend(found);
    }
    Ok(out)
}

/// The author's rating of one of the test's offers.
pub fn answer(conn: &Connection, id: i64, verdict: &str, why: &str) -> AppResult<()> {
    if !matches!(verdict, "yes" | "maybe" | "no") {
        return Err(AppError::Invalid("verdict".into()));
    }
    let why = why.trim();
    if why.chars().count() > WHY_MAX {
        return Err(AppError::Invalid("why is too long".into()));
    }
    let mut stored = load(conn)?;
    if !stored.ids.contains(&id) {
        return Err(AppError::NotFound);
    }
    stored.answers.insert(id, Answer { verdict: verdict.into(), why: why.into() });
    save(conn, &stored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_db;
    use crate::model::{AddOutcome, OfferInput};

    fn add(conn: &mut Connection, title: &str, score: Option<i64>) -> i64 {
        let input = OfferInput {
            title: title.into(),
            company: "Zmyślona Firma".into(),
            source: "adzuna".into(),
            description: Some(format!("{title}. {}", "Opis stanowiska i wymagań. ".repeat(20))),
            ..Default::default()
        };
        let id = match crate::offers::add(conn, input).unwrap() {
            AddOutcome::Added { id } => id,
            other => panic!("{other:?}"),
        };
        conn.execute("UPDATE offers SET score = ?2 WHERE id = ?1", params![id, score]).unwrap();
        id
    }

    #[test]
    fn picks_ten_offers_from_the_best_to_the_worst_once() {
        let mut conn = test_db();
        let ids: Vec<i64> = (0..30).map(|i| add(&mut conn, &format!("Oferta {i}"), Some(100 - i * 3))).collect();
        let first: Vec<i64> = offers(&conn).unwrap().iter().map(|o| o.id).collect();
        assert_eq!(first.len(), 10);
        assert_eq!(first[0], ids[0]);
        assert_eq!(first[9], ids[29]);
        // The set stays the same, also when new offers come.
        add(&mut conn, "Nowa", Some(100));
        assert_eq!(offers(&conn).unwrap().iter().map(|o| o.id).collect::<Vec<_>>(), first);
    }

    #[test]
    fn keeps_answers_and_checks_them() {
        let mut conn = test_db();
        for i in 0..12 {
            add(&mut conn, &format!("Oferta {i}"), Some(50));
        }
        let id = offers(&conn).unwrap()[3].id;
        answer(&conn, id, "maybe", "  Pasuje, ale daleko.  ").unwrap();
        let got = offers(&conn).unwrap().into_iter().find(|o| o.id == id).unwrap().answer.unwrap();
        assert_eq!(got, Answer { verdict: "maybe".into(), why: "Pasuje, ale daleko.".into() });
        assert!(answer(&conn, id, "bardzo", "").is_err());
        assert!(answer(&conn, id, "yes", &"x".repeat(301)).is_err());
        assert!(matches!(answer(&conn, 999_999, "yes", ""), Err(AppError::NotFound)));
        // Not an app setting: the settings list does not show it.
        assert!(!crate::settings::all(&conn).unwrap().contains_key(KEY));
    }
}
